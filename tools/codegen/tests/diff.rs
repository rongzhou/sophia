//! 差测试（W3 起的核心）：解释器（oracle）vs WASM 后端逐 case 等价。
//!
//! 见 docs/wasm_codegen.md §七。对每个程序 + 每组实参：① 解释器执行得 `Outcome`（oracle）；
//! ② codegen emit `.wasm`、用纯 Rust 解释执行器 `wasmi` 执行得 `Outcome'`；③ 断言两者等价。
//!
//! **首要不变量**：解释器是唯一语义真相源。任何不一致 = WASM 后端有 bug，**绝不**调和 / 伪造一致。
//!
//! W2 覆盖标量核心（Int / Bool / Null + 算术 / 比较 / 布尔 / 一元 / if-else / let-set / return /
//! 跨调用），故差测试程序限于该子集；含 match / Text / effect 等的程序待后续增量纳入。

use sophia_codegen::{emit_module, CodegenInput};
use sophia_hir::{resolve_program, ProgramInput};
use sophia_runtime::{run_action, HostRegistry, Outcome, Value};
use sophia_semantic::{analyze_program, SemanticModel};
use sophia_stdlib::{register_mock_hosts, standard_registry, MockBuckets};
use sophia_syntax::{parse_ast, Ast};
use wasmi::{Caller, Engine, Linker, Module, Store};

/// 差测试 host 状态：mock 文件 / 网络桶（与 `InMemoryHost` 同语义）+ 读回 stash + console 捕获。
///
/// 与 sophia 解释器 oracle 用**同一份** seed 数据（path→content / url→body），保证两后端公平。
/// `file_read` / `http_get` 未命中 mock → host trap（解释器为硬错误阻断，绝不伪造）。
#[derive(Default)]
struct HostState {
    files: std::collections::BTreeMap<String, String>,
    http: std::collections::BTreeMap<String, String>,
    /// 上次 file_read / http_get 暂存的字节（供 read_copy 拷回 WASM 内存）。
    stash: Vec<u8>,
    console: Vec<String>,
}

/// 在 linker 上注册 5 个 effect host import（字节级 ABI，桥接 mock 文件 / 网络桶）。
fn link_host(linker: &mut Linker<HostState>) {
    linker
        .func_wrap(
            "sophia_host",
            "console_write",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                let s = read_mem_string(&mut caller, ptr, len);
                caller.data_mut().console.push(s);
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "sophia_host",
            "file_write",
            |mut caller: Caller<'_, HostState>, pp: i32, pl: i32, cp: i32, cl: i32| {
                let path = read_mem_string(&mut caller, pp, pl);
                let content = read_mem_string(&mut caller, cp, cl);
                caller.data_mut().files.insert(path, content);
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "sophia_host",
            "file_read",
            |mut caller: Caller<'_, HostState>, pp: i32, pl: i32| -> Result<i32, wasmi::Error> {
                let path = read_mem_string(&mut caller, pp, pl);
                match caller.data().files.get(&path).cloned() {
                    Some(content) => {
                        let bytes = content.into_bytes();
                        let n = bytes.len() as i32;
                        caller.data_mut().stash = bytes;
                        Ok(n)
                    }
                    None => Err(wasmi::Error::new(format!("no mock file: {path}"))),
                }
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "sophia_host",
            "http_get",
            |mut caller: Caller<'_, HostState>, up: i32, ul: i32| -> Result<i32, wasmi::Error> {
                let url = read_mem_string(&mut caller, up, ul);
                match caller.data().http.get(&url).cloned() {
                    Some(body) => {
                        let bytes = body.into_bytes();
                        let n = bytes.len() as i32;
                        caller.data_mut().stash = bytes;
                        Ok(n)
                    }
                    None => Err(wasmi::Error::new(format!("no mock http: {url}"))),
                }
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "sophia_host",
            "read_copy",
            |mut caller: Caller<'_, HostState>, dst: i32| {
                let bytes = std::mem::take(&mut caller.data_mut().stash);
                let mem = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .expect("memory");
                mem.write(&mut caller, dst as usize, &bytes).unwrap();
            },
        )
        .unwrap();
}

/// 从 WASM 线性内存读出 UTF-8 字符串。
fn read_mem_string(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> String {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .expect("memory");
    let mut buf = vec![0u8; len as usize];
    mem.read(&*caller, ptr as usize, &mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

/// 差测试实参（Int / State / Text；entity 入参在 W2c 经"标量入参 + 内部构造"规避）。
#[derive(Debug, Clone)]
enum Arg {
    Int(i64),
    /// State 值：state 名 + value 名。
    State(&'static str, &'static str),
    /// Text 值。
    Text(&'static str),
}

impl Arg {
    fn to_value(&self) -> Value {
        match self {
            Arg::Int(i) => Value::Int(*i),
            Arg::State(s, v) => Value::State {
                state: s.to_string(),
                value: v.to_string(),
            },
            Arg::Text(t) => Value::Text(t.to_string()),
        }
    }
}

/// 标量 / 聚合 Outcome 投影（差测试比标量核心 + 错误代数 + State/Entity）。
#[derive(Debug, PartialEq)]
enum ScalarOutcome {
    Int(i64),
    Bool(bool),
    Null,
    Unit,
    /// State 值：state 名 + value 名。
    State(String, String),
    /// Text 值。
    Text(String),
    /// Entity 记录：entity 名 + Int 字段（按名排序）。
    Entity(String, Vec<(String, i64)>),
    /// 被返回的 `one of` 失败成员：variant 名 + 字段（仅 Int 字段，按名排序）。
    ErrorValue(String, Vec<(String, i64)>),
    /// `raise` 的领域错误：variant 名 + 字段（仅 Int 字段，按名排序）。
    Raised(String, Vec<(String, i64)>),
}

fn analyze(sources: &[(&str, &str)]) -> (Vec<Ast>, SemanticModel) {
    let asts: Vec<Ast> = sources
        .iter()
        .map(|(_, s)| parse_ast(*s).unwrap())
        .collect();
    let inputs: Vec<ProgramInput> = sources
        .iter()
        .zip(&asts)
        .map(|((path, _), ast)| ProgramInput {
            domain: "d",
            path,
            ast,
        })
        .collect();
    let (index, _d) = resolve_program(&inputs, &standard_registry()).expect("resolve");
    let refs: Vec<&Ast> = asts.iter().collect();
    let analysis = analyze_program(&refs, &index);
    assert!(
        analysis.diagnostics.is_empty(),
        "测试程序应通过语义检查：{:?}",
        analysis.diagnostics
    );
    (asts, analysis.model)
}

/// 解释器 oracle 执行（带 effect 前置：stdlib mock host，与 WASM 后端共享同一份 seed）。
fn run_interp(
    model: &SemanticModel,
    asts: &[&Ast],
    entry: &str,
    args: Vec<Value>,
    seeds: &Seeds,
) -> ScalarOutcome {
    let buckets = MockBuckets::new();
    for (p, c) in &seeds.files {
        buckets.seed_file(*p, *c);
    }
    for (u, b) in &seeds.http {
        buckets.seed_http(*u, *b);
    }
    let mut host = HostRegistry::new();
    register_mock_hosts(&mut host, &buckets);
    let (outcome, _trace) = run_action(model, asts, entry, args, &mut host).expect("解释执行");
    match outcome {
        Outcome::Returned(Value::Int(i)) => ScalarOutcome::Int(i),
        Outcome::Returned(Value::Bool(b)) => ScalarOutcome::Bool(b),
        Outcome::Returned(Value::Null) => ScalarOutcome::Null,
        Outcome::Returned(Value::Unit) => ScalarOutcome::Unit,
        Outcome::Returned(Value::Text(t)) => ScalarOutcome::Text(t),
        Outcome::Returned(Value::State { state, value }) => ScalarOutcome::State(state, value),
        Outcome::Returned(Value::Entity { name, fields }) => {
            ScalarOutcome::Entity(name, int_fields(&fields))
        }
        Outcome::Returned(Value::ErrorValue { variant, fields }) => {
            ScalarOutcome::ErrorValue(variant, int_fields(&fields))
        }
        Outcome::Raised(e) => ScalarOutcome::Raised(e.variant, int_fields(&e.fields)),
        other => panic!("差测试暂只覆盖标量 / 错误代数 / State / Entity 返回，得到 {other:?}"),
    }
}

/// 从 `BTreeMap<String, Value>` 取 Int 字段（按名排序；非 Int 字段 panic——W2b 测试只用 Int 字段）。
fn int_fields(fields: &std::collections::BTreeMap<String, Value>) -> Vec<(String, i64)> {
    fields
        .iter()
        .map(|(k, v)| match v {
            Value::Int(i) => (k.clone(), *i),
            other => panic!("差测试字段暂只支持 Int，得到 {other:?}"),
        })
        .collect()
}

/// WASM 后端执行：emit + wasmi 加载执行 + 读回结局。
fn run_wasm(
    model: &SemanticModel,
    asts: &[&Ast],
    entry: &str,
    args: &[Arg],
    entry_is_action: bool,
    seeds: &Seeds,
) -> ScalarOutcome {
    let input = CodegenInput::new(model, asts, &sophia_stdlib::standard_registry());
    let bytes = emit_module(&input).expect("emit wasm");

    let engine = Engine::default();
    let module = Module::new(&engine, &bytes[..]).expect("wasmi 加载模块");
    let mut linker = Linker::<HostState>::new(&engine);
    link_host(&mut linker);
    let mut state = HostState::default();
    for (p, c) in &seeds.files {
        state.files.insert(p.to_string(), c.to_string());
    }
    for (u, b) in &seeds.http {
        state.http.insert(u.to_string(), b.to_string());
    }
    let mut store = Store::new(&engine, state);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("实例化")
        .start(&mut store)
        .expect("start");

    let memory = instance.get_memory(&store, "memory").expect("memory 导出");

    let make_int = instance
        .get_typed_func::<i64, i32>(&store, "sophia_make_int")
        .expect("make_int");
    let make_state = instance
        .get_typed_func::<(i32, i32, i32, i32), i32>(&store, "sophia_make_state")
        .expect("make_state");
    let make_text = instance
        .get_typed_func::<(i32, i32), i32>(&store, "sophia_make_text")
        .expect("make_text");

    // 构造实参句柄。State / Text 参数：把名字 / 字节写入预留高地址区（page 15，远离 bump 堆），
    // 再 make_state / make_text——差测试夹具向 WASM 注入聚合 / 文本入参的确定性手段。
    let mut scratch_off: i32 = 15 * 65536;
    let mut handles: Vec<i32> = Vec::new();
    for a in args {
        match a {
            Arg::Int(i) => handles.push(make_int.call(&mut store, *i).expect("make_int call")),
            Arg::State(s, v) => {
                let sp = scratch_off;
                memory.write(&mut store, sp as usize, s.as_bytes()).unwrap();
                let vp = sp + s.len() as i32;
                memory.write(&mut store, vp as usize, v.as_bytes()).unwrap();
                scratch_off = vp + v.len() as i32;
                handles.push(
                    make_state
                        .call(&mut store, (sp, s.len() as i32, vp, v.len() as i32))
                        .expect("make_state call"),
                );
            }
            Arg::Text(t) => {
                let tp = scratch_off;
                memory.write(&mut store, tp as usize, t.as_bytes()).unwrap();
                scratch_off = tp + t.len() as i32;
                handles.push(
                    make_text
                        .call(&mut store, (tp, t.len() as i32))
                        .expect("make_text call"),
                );
            }
        }
    }

    // 调用入口。入口参数数 0..=3 覆盖现有差测试。
    let prefix = if entry_is_action {
        "action_"
    } else {
        "transition_"
    };
    let fname = format!("{prefix}{entry}");
    let outcome_handle: i32 = call_entry(&mut store, &instance, &fname, &handles);

    // 读回 Outcome：kind + value。
    let outcome_kind = instance
        .get_typed_func::<i32, i32>(&store, "sophia_outcome_kind")
        .expect("outcome_kind");
    let outcome_value = instance
        .get_typed_func::<i32, i32>(&store, "sophia_outcome_value")
        .expect("outcome_value");
    let value_tag = instance
        .get_typed_func::<i32, i32>(&store, "sophia_value_tag")
        .expect("value_tag");
    let get_int = instance
        .get_typed_func::<i32, i64>(&store, "sophia_get_int")
        .expect("get_int");
    let get_bool = instance
        .get_typed_func::<i32, i32>(&store, "sophia_get_bool")
        .expect("get_bool");

    let kind = outcome_kind.call(&mut store, outcome_handle).unwrap();
    let v = outcome_value.call(&mut store, outcome_handle).unwrap();
    let tag = value_tag.call(&mut store, v).unwrap();
    // kind: 0=Returned, 1=Raised。tag: 2=Int,1=Bool,4=Null,0=Unit,6=ErrorValue,7=Entity,8=State。
    match (kind, tag) {
        (0, 2) => ScalarOutcome::Int(get_int.call(&mut store, v).unwrap()),
        (0, 1) => ScalarOutcome::Bool(get_bool.call(&mut store, v).unwrap() != 0),
        (0, 4) => ScalarOutcome::Null,
        (0, 0) => ScalarOutcome::Unit,
        (0, 3) => ScalarOutcome::Text(read_text(&store, &memory, v)),
        (0, 8) => {
            let (s, val) = read_state(&store, &memory, v);
            ScalarOutcome::State(s, val)
        }
        (0, 7) => {
            let (name, fields) = read_record(&mut store, &instance, &memory, v, &get_int);
            ScalarOutcome::Entity(name, fields)
        }
        (0, 6) => {
            let (variant, fields) = read_record(&mut store, &instance, &memory, v, &get_int);
            ScalarOutcome::ErrorValue(variant, fields)
        }
        (1, 6) => {
            let (variant, fields) = read_record(&mut store, &instance, &memory, v, &get_int);
            ScalarOutcome::Raised(variant, fields)
        }
        other => panic!("差测试读回未知 (kind,tag) {other:?}"),
    }
}

/// 读回一个 Text 值：bytes_ptr/byte_len（从内存读 UTF-8 字节）。
fn read_text(store: &Store<HostState>, memory: &wasmi::Memory, t: i32) -> String {
    let data = memory.data(store);
    let read_i32 = |off: i32| -> i32 {
        let o = off as usize;
        i32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])
    };
    // [tag@0][bytes_ptr@4][byte_len@8]
    let p = read_i32(t + 4);
    let len = read_i32(t + 8);
    String::from_utf8(data[p as usize..(p + len) as usize].to_vec()).unwrap()
}

/// 读回一个 State 值：state 名 + value 名（从内存读字节）。
fn read_state(store: &Store<HostState>, memory: &wasmi::Memory, st: i32) -> (String, String) {
    let data = memory.data(store);
    let read_i32 = |off: i32| -> i32 {
        let o = off as usize;
        i32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])
    };
    let read_str = |ptr: i32, len: i32| -> String {
        let p = ptr as usize;
        String::from_utf8(data[p..p + len as usize].to_vec()).unwrap()
    };
    // [tag@0][state_ptr@4][state_len@8][value_ptr@12][value_len@16]
    let sp = read_i32(st + 4);
    let sl = read_i32(st + 8);
    let vp = read_i32(st + 12);
    let vl = read_i32(st + 16);
    (read_str(sp, sl), read_str(vp, vl))
}

/// 读回一个 ErrorValue 记录：variant 名（从内存读字节）+ Int 字段（按名排序）。
fn read_record(
    store: &mut Store<HostState>,
    instance: &wasmi::Instance,
    memory: &wasmi::Memory,
    rec: i32,
    get_int: &wasmi::TypedFunc<i32, i64>,
) -> (String, Vec<(String, i64)>) {
    // 先在一个借用作用域内从内存读出 variant 名 + 各字段元信息（key 名 + val 句柄）。
    let (variant, field_meta): (String, Vec<(String, i32)>) = {
        let data = memory.data(&*store);
        let read_i32 = |off: i32| -> i32 {
            let o = off as usize;
            i32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])
        };
        let read_str = |ptr: i32, len: i32| -> String {
            let p = ptr as usize;
            String::from_utf8(data[p..p + len as usize].to_vec()).unwrap()
        };
        // header: [tag@0][name_ptr@4][name_len@8][nfields@12]
        let name_ptr = read_i32(rec + 4);
        let name_len = read_i32(rec + 8);
        let nfields = read_i32(rec + 12);
        let variant = read_str(name_ptr, name_len);
        // 各字段 [key_ptr@0][key_len@4][val@8]，每项 12 字节，从 rec+16 起。
        let mut meta = Vec::new();
        for i in 0..nfields {
            let base = rec + 16 + i * 12;
            let kptr = read_i32(base);
            let klen = read_i32(base + 4);
            let val = read_i32(base + 8);
            meta.push((read_str(kptr, klen), val));
        }
        (variant, meta)
    };
    // 借用结束后再调用 get_int 取各字段 Int 值。
    let mut fields: Vec<(String, i64)> = field_meta
        .into_iter()
        .map(|(k, valh)| (k, get_int.call(&mut *store, valh).unwrap()))
        .collect();
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    let _ = instance;
    (variant, fields)
}

/// 按实参个数选用对应 arity 的 typed func 调用入口。
fn call_entry(
    store: &mut Store<HostState>,
    instance: &wasmi::Instance,
    fname: &str,
    handles: &[i32],
) -> i32 {
    match handles.len() {
        0 => instance
            .get_typed_func::<(), i32>(&*store, fname)
            .expect(fname)
            .call(store, ())
            .unwrap(),
        1 => instance
            .get_typed_func::<i32, i32>(&*store, fname)
            .expect(fname)
            .call(store, handles[0])
            .unwrap(),
        2 => instance
            .get_typed_func::<(i32, i32), i32>(&*store, fname)
            .expect(fname)
            .call(store, (handles[0], handles[1]))
            .unwrap(),
        3 => instance
            .get_typed_func::<(i32, i32, i32), i32>(&*store, fname)
            .expect(fname)
            .call(store, (handles[0], handles[1], handles[2]))
            .unwrap(),
        n => panic!("差测试入口暂支持 0..=3 参，得到 {n}"),
    }
}

/// 差测试的执行前置环境（mock 文件 / 网络桶；两后端共享同一份，保证公平）。
#[derive(Default)]
struct Seeds {
    files: Vec<(&'static str, &'static str)>,
    http: Vec<(&'static str, &'static str)>,
}

/// 断言：解释器与 WASM 对同一组 Int 实参等价（便捷版，无 effect 前置）。
fn assert_equiv(sources: &[(&str, &str)], entry: &str, args: &[i64], action: bool) {
    let arg_vec: Vec<Arg> = args.iter().map(|&i| Arg::Int(i)).collect();
    assert_equiv_seeded(sources, entry, &arg_vec, action, &Seeds::default());
}

/// 断言：解释器与 WASM 对同一组实参（Int / State / Text）等价（无 effect 前置）。
fn assert_equiv_args(sources: &[(&str, &str)], entry: &str, args: &[Arg], action: bool) {
    assert_equiv_seeded(sources, entry, args, action, &Seeds::default());
}

/// 断言：解释器与 WASM 等价，带 effect 执行前置（mock 文件 / 网络 seed，两后端共享）。
fn assert_equiv_seeded(
    sources: &[(&str, &str)],
    entry: &str,
    args: &[Arg],
    action: bool,
    seeds: &Seeds,
) {
    let (asts, model) = analyze(sources);
    let refs: Vec<&Ast> = asts.iter().collect();
    let interp = run_interp(
        &model,
        &refs,
        entry,
        args.iter().map(|a| a.to_value()).collect(),
        seeds,
    );
    let wasm = run_wasm(&model, &refs, entry, args, action, seeds);
    assert_eq!(
        interp, wasm,
        "解释器 vs WASM 不一致：entry={entry} args={args:?}（解释器为 oracle）"
    );
}

#[test]
fn diff_abs_difference_like_arithmetic() {
    // L1 abs_difference 的等价：if (l - r) < 0 then -(l-r) else (l-r)。
    let src = "action AbsDiff {\n\
       input { l: Int; r: Int }\n\
       output { y: Int }\n\
       body {\n\
         let d = l - r\n\
         if d < 0 { return -d } else { return d }\n\
       }\n\
     }";
    let sources = &[("d/actions/AbsDiff.sophia", src)];
    assert_equiv(sources, "AbsDiff", &[9, 2], true);
    assert_equiv(sources, "AbsDiff", &[2, 9], true);
    assert_equiv(sources, "AbsDiff", &[5, 5], true);
}

#[test]
fn diff_within_budget_bool() {
    let src = "action WithinBudget {\n\
       input { spent: Int; budget: Int }\n\
       output { ok: Bool }\n\
       body { return spent <= budget }\n\
     }";
    let sources = &[("d/actions/WithinBudget.sophia", src)];
    assert_equiv(sources, "WithinBudget", &[80, 100], true);
    assert_equiv(sources, "WithinBudget", &[100, 100], true);
    assert_equiv(sources, "WithinBudget", &[120, 100], true);
}

#[test]
fn diff_cross_call() {
    // L3 风格：NetTotal 调用 GrossTotal。
    let gross = "action GrossTotal {\n\
       input { unit_price: Int; quantity: Int }\n\
       output { y: Int }\n\
       body { return unit_price * quantity }\n\
     }";
    let net = "action NetTotal {\n\
       input { unit_price: Int; quantity: Int; discount: Int }\n\
       output { y: Int }\n\
       body {\n\
         let g = GrossTotal(unit_price, quantity)\n\
         return g - discount\n\
       }\n\
     }";
    let sources = &[
        ("d/actions/GrossTotal.sophia", gross),
        ("d/actions/NetTotal.sophia", net),
    ];
    assert_equiv(sources, "NetTotal", &[10, 5, 8], true);
    assert_equiv(sources, "NetTotal", &[7, 3, 0], true);
}

#[test]
fn diff_boolean_and_comparison() {
    let src = "action InRange {\n\
       input { n: Int; lo: Int; hi: Int }\n\
       output { ok: Bool }\n\
       body { return lo <= n and n <= hi }\n\
     }";
    let sources = &[("d/actions/InRange.sophia", src)];
    assert_equiv(sources, "InRange", &[5, 0, 10], true);
    assert_equiv(sources, "InRange", &[15, 0, 10], true);
    assert_equiv(sources, "InRange", &[0, 0, 10], true);
}

#[test]
fn diff_equality_and_not() {
    let src = "action NotEqual {\n\
       input { a: Int; b: Int }\n\
       output { ok: Bool }\n\
       body { return not (a == b) }\n\
     }";
    let sources = &[("d/actions/NotEqual.sophia", src)];
    assert_equiv(sources, "NotEqual", &[3, 3], true);
    assert_equiv(sources, "NotEqual", &[3, 4], true);
}

#[test]
fn diff_error_algebra_raise() {
    // L4 风格：raise 领域错误（带 Int 字段）。
    let err = "error StockError { variant Insufficient { shortfall: Int } }";
    let act = "action RemoveStock {\n\
       input { available: Int; requested: Int }\n\
       output { y: Int }\n\
       errors { Insufficient }\n\
       body {\n\
         if requested > available { raise Insufficient { shortfall = requested - available } }\n\
         else { return available - requested }\n\
       }\n\
     }";
    let sources = &[
        ("d/errors/StockError.sophia", err),
        ("d/actions/RemoveStock.sophia", act),
    ];
    assert_equiv(sources, "RemoveStock", &[50, 8], true); // 返回 42
    assert_equiv(sources, "RemoveStock", &[8, 8], true); // 返回 0
    assert_equiv(sources, "RemoveStock", &[5, 12], true); // raise Insufficient{shortfall:7}
}

#[test]
fn diff_one_of_return_d1() {
    // D1 风格：可失败返回 one of { Int, OutOfRange }（失败是返回值，非 raise）。
    let err = "error RangeError { variant OutOfRange { value: Int } }";
    let act = "action ClampOrReject {\n\
       input { n: Int; limit: Int }\n\
       output { result: one of { Int, OutOfRange } }\n\
       body {\n\
         if n < 0 { return OutOfRange { value = n } }\n\
         else {\n\
           if n > limit { return OutOfRange { value = n } }\n\
           else { return n }\n\
         }\n\
       }\n\
     }";
    let sources = &[
        ("d/errors/RangeError.sophia", err),
        ("d/actions/ClampOrReject.sophia", act),
    ];
    assert_equiv(sources, "ClampOrReject", &[3, 10], true); // 返回 3
    assert_equiv(sources, "ClampOrReject", &[0, 10], true); // 返回 0
    assert_equiv(sources, "ClampOrReject", &[10, 10], true); // 返回 10
    assert_equiv(sources, "ClampOrReject", &[15, 10], true); // 返回 OutOfRange{value:15}
}

#[test]
fn diff_match_on_one_of() {
    // match 一个 one of { Int, OutOfRange }：成功成员（Int 类型 pattern）/ 失败成员（variant pattern）。
    let err = "error RangeError { variant OutOfRange { value: Int } }";
    let checker = "action Check {\n\
       input { n: Int; limit: Int }\n\
       output { result: one of { Int, OutOfRange } }\n\
       body {\n\
         if n > limit { return OutOfRange { value = n } } else { return n }\n\
       }\n\
     }";
    // Classify 调用 Check 并 match：成功返回 1、失败返回 0（验证 match 分派 + 跨调用 + variant 绑定）。
    let classify = "action Classify {\n\
       input { n: Int; limit: Int }\n\
       output { code: Int }\n\
       body {\n\
         let r = Check(n, limit)\n\
         match r {\n\
           Int ok => return ok\n\
           OutOfRange { value } => return value\n\
         }\n\
       }\n\
     }";
    let sources = &[
        ("d/errors/RangeError.sophia", err),
        ("d/actions/Check.sophia", checker),
        ("d/actions/Classify.sophia", classify),
    ];
    assert_equiv(sources, "Classify", &[3, 10], true); // Check 返回 3 → Int ok → 3
    assert_equiv(sources, "Classify", &[15, 10], true); // Check 返回 OutOfRange{15} → value → 15
}

#[test]
fn diff_entity_construct_and_field() {
    // L2 rectangle_area 等价（标量入参 + 内部构造 entity + 字段访问）。
    let ent = "entity Rectangle { fields { width { type: Int } height { type: Int } } }";
    let act = "action RectArea {\n\
       input { w: Int; h: Int }\n\
       output { area: Int }\n\
       body {\n\
         let r = Rectangle { width = w, height = h }\n\
         return r.width * r.height\n\
       }\n\
     }";
    let sources = &[
        ("d/entities/Rectangle.sophia", ent),
        ("d/actions/RectArea.sophia", act),
    ];
    assert_equiv(sources, "RectArea", &[6, 6], true); // 36
    assert_equiv(sources, "RectArea", &[10, 3], true); // 30
    assert_equiv(sources, "RectArea", &[1, 1], true); // 1
}

#[test]
fn diff_entity_returned() {
    // 返回 entity 值（构造后直接返回，验证 Entity 值读回等价）。
    let ent = "entity Point { fields { x { type: Int } y { type: Int } } }";
    let act = "action MakePoint {\n\
       input { a: Int; b: Int }\n\
       output { p: Point }\n\
       body { return Point { x = a, y = b } }\n\
     }";
    let sources = &[
        ("d/entities/Point.sophia", ent),
        ("d/actions/MakePoint.sophia", act),
    ];
    assert_equiv(sources, "MakePoint", &[3, 7], true);
}

#[test]
fn diff_state_match_and_return() {
    // L2 traffic_next 等价（State 入参 + state 值 pattern match + 返回 State 值）。
    let st = "state TrafficLight {\n\
       value Red { meaning: \"r\" }\n\
       value Green { meaning: \"g\" }\n\
       value Yellow { meaning: \"y\" }\n\
     }";
    let act = "action NextLight {\n\
       input { current: TrafficLight }\n\
       output { next: TrafficLight }\n\
       body {\n\
         match current {\n\
           TrafficLight.Red => return TrafficLight.Green\n\
           TrafficLight.Green => return TrafficLight.Yellow\n\
           TrafficLight.Yellow => return TrafficLight.Red\n\
         }\n\
       }\n\
     }";
    let sources = &[
        ("d/states/TrafficLight.sophia", st),
        ("d/actions/NextLight.sophia", act),
    ];
    assert_equiv_args(
        sources,
        "NextLight",
        &[Arg::State("TrafficLight", "Red")],
        true,
    );
    assert_equiv_args(
        sources,
        "NextLight",
        &[Arg::State("TrafficLight", "Green")],
        true,
    );
    assert_equiv_args(
        sources,
        "NextLight",
        &[Arg::State("TrafficLight", "Yellow")],
        true,
    );
}

#[test]
fn diff_checkout_limit_l5() {
    // L5 checkout_limit 等价（标量入参 + 内部构造 entity + 跨调用 + 错误代数）。
    let ent = "entity OrderLine { fields { unit_price { type: Int } quantity { type: Int } } }";
    let err = "error CreditError { variant OverLimit { excess: Int } }";
    let line_amount = "action LineAmount {\n\
       input { line: OrderLine }\n\
       output { amount: Int }\n\
       body { return line.unit_price * line.quantity }\n\
     }";
    let checkout = "action Checkout {\n\
       input { unit_price: Int; quantity: Int; credit_limit: Int }\n\
       output { y: Int }\n\
       errors { OverLimit }\n\
       body {\n\
         let line = OrderLine { unit_price = unit_price, quantity = quantity }\n\
         let amount = LineAmount(line)\n\
         if amount > credit_limit { raise OverLimit { excess = amount - credit_limit } }\n\
         else { return amount }\n\
       }\n\
     }";
    let sources = &[
        ("d/entities/OrderLine.sophia", ent),
        ("d/errors/CreditError.sophia", err),
        ("d/actions/LineAmount.sophia", line_amount),
        ("d/actions/Checkout.sophia", checkout),
    ];
    assert_equiv(sources, "Checkout", &[6, 7, 100], true); // 42
    assert_equiv(sources, "Checkout", &[5, 5, 25], true); // 25（边界）
    assert_equiv(sources, "Checkout", &[9, 4, 30], true); // raise OverLimit{excess:6}
}

#[test]
fn diff_text_concat_and_length() {
    // Text 拼接 + .length（Unicode 标量计数）。
    let act = "action Greet {\n\
       input { name: Sanitized<Text> }\n\
       output { n: Int }\n\
       body {\n\
         let msg = \"Hi \" + name\n\
         return msg.length\n\
       }\n\
     }";
    let sources = &[("d/actions/Greet.sophia", act)];
    // "Hi " + "Bob" = "Hi Bob" 长 6；"Hi " + "" = "Hi " 长 3。
    assert_equiv_args(sources, "Greet", &[Arg::Text("Bob")], true);
    assert_equiv_args(sources, "Greet", &[Arg::Text("")], true);
    // 多字节字符：".length" 按 Unicode 标量计数（"Hi 世界" = 5 标量，非字节数）。
    assert_equiv_args(sources, "Greet", &[Arg::Text("世界")], true);
}

#[test]
fn diff_text_return_and_equality() {
    // 返回 Text 值 + Text 相等比较。
    let echo = "action Echo {\n\
       input { s: Sanitized<Text> }\n\
       output { out: Sanitized<Text> }\n\
       body { return s + \"!\" }\n\
     }";
    let sources = &[("d/actions/Echo.sophia", echo)];
    assert_equiv_args(sources, "Echo", &[Arg::Text("hello")], true);

    let same = "action IsSame {\n\
       input { a: Sanitized<Text>; b: Sanitized<Text> }\n\
       output { ok: Bool }\n\
       body { return a == b }\n\
     }";
    let s2 = &[("d/actions/IsSame.sophia", same)];
    assert_equiv_args(s2, "IsSame", &[Arg::Text("x"), Arg::Text("x")], true);
    assert_equiv_args(s2, "IsSame", &[Arg::Text("x"), Arg::Text("y")], true);
}

#[test]
fn diff_repeat_loop() {
    // repeat n times：累加循环（验证有界循环 + set 在循环体内）。
    let act = "action SumTo {\n\
       input { n: Int }\n\
       output { total: Int }\n\
       body {\n\
         let acc = 0\n\
         let i = 0\n\
         repeat n times {\n\
           set i = i + 1\n\
           set acc = acc + i\n\
         }\n\
         return acc\n\
       }\n\
     }";
    let sources = &[("d/actions/SumTo.sophia", act)];
    assert_equiv(sources, "SumTo", &[5], true); // 1+2+3+4+5 = 15
    assert_equiv(sources, "SumTo", &[0], true); // 0
    assert_equiv(sources, "SumTo", &[1], true); // 1
    assert_equiv(sources, "SumTo", &[-3], true); // n<=0 → 0
}

#[test]
fn diff_repeat_with_early_return() {
    // repeat 体内 if-return：达到阈值提前返回（验证循环内 return 经 Outcome 提前退出函数）。
    let act = "action CountUntil {\n\
       input { limit: Int; cap: Int }\n\
       output { y: Int }\n\
       body {\n\
         let c = 0\n\
         repeat limit times {\n\
           set c = c + 1\n\
           if c > cap { return c } else { return c }\n\
         }\n\
         return c\n\
       }\n\
     }";
    let sources = &[("d/actions/CountUntil.sophia", act)];
    assert_equiv(sources, "CountUntil", &[5, 3], true); // 第一轮即 return 1
    assert_equiv(sources, "CountUntil", &[0, 3], true); // 不进循环 → return 0
}

#[test]
fn diff_console_write_effect() {
    // G2 风格：print（Console.Write effect）+ 返回长度。验证 effect 经 host import 执行。
    let cap = "capability AuditCap { allow { Console.Write } }";
    let act = "action LogNotice {\n\
       capability: AuditCap\n\
       input { message: Sanitized<Text> }\n\
       output { n: Int }\n\
       effects { Console.Write }\n\
       body {\n\
         print message\n\
         return message.length\n\
       }\n\
     }";
    let sources = &[
        ("d/capabilities/AuditCap.sophia", cap),
        ("d/actions/LogNotice.sophia", act),
    ];
    // "hello" 长 5；console 副作用不进结局比对（结局是返回值），但 host import 必须真执行不 trap。
    assert_equiv_args(sources, "LogNotice", &[Arg::Text("hello")], true);
}

#[test]
fn diff_http_get_intent_d2() {
    // D2 旗舰：Http.Get → Raw<Text> 经 intent_conversion 转 Sanitized 后取长度。mock host 注入。
    let cap = "capability NetCap { allow { Http.Get } }";
    let trust = "action Trust {\n\
       intent_conversion: true\n\
       input { raw: Raw<Text> }\n\
       output { clean: Sanitized<Text> }\n\
       effects { Pure }\n\
       body { return raw }\n\
     }";
    let fetch = "action FetchLength {\n\
       capability: NetCap\n\
       input { url: Text }\n\
       output { len: Int }\n\
       effects { Http.Get }\n\
       body {\n\
         let raw = Http.Get(url)\n\
         let clean = Trust(raw)\n\
         return clean.length\n\
       }\n\
     }";
    let sources = &[
        ("d/capabilities/NetCap.sophia", cap),
        ("d/actions/Trust.sophia", trust),
        ("d/actions/FetchLength.sophia", fetch),
    ];
    let seeds = Seeds {
        files: vec![],
        http: vec![("https://example.test/doc", "hello")], // 长 5
    };
    assert_equiv_seeded(
        sources,
        "FetchLength",
        &[Arg::Text("https://example.test/doc")],
        true,
        &seeds,
    );
}

#[test]
fn diff_file_read_write_d3() {
    // D3 风格：File.Read 取回 Raw<Text> → 经 intent 转换 → File.Write 写出 → File.Read 读回长度。
    let cap = "capability ArchiveCap { allow { File.Read; File.Write } }";
    let trust = "action Trust {\n\
       intent_conversion: true\n\
       input { raw: Raw<Text> }\n\
       output { clean: Sanitized<Text> }\n\
       effects { Pure }\n\
       body { return raw }\n\
     }";
    let archive = "action Archive {\n\
       capability: ArchiveCap\n\
       input { source: Text; dest: Text }\n\
       output { len: Int }\n\
       effects { File.Read; File.Write }\n\
       body {\n\
         let raw = File.Read(source)\n\
         let clean = Trust(raw)\n\
         File.Write(dest, clean)\n\
         let back = File.Read(dest)\n\
         let verified = Trust(back)\n\
         return verified.length\n\
       }\n\
     }";
    let sources = &[
        ("d/capabilities/ArchiveCap.sophia", cap),
        ("d/actions/Trust.sophia", trust),
        ("d/actions/Archive.sophia", archive),
    ];
    let seeds = Seeds {
        files: vec![("inbox/note.txt", "hello")], // 长 5
        http: vec![],
    };
    assert_equiv_seeded(
        sources,
        "Archive",
        &[Arg::Text("inbox/note.txt"), Arg::Text("archive/note.txt")],
        true,
        &seeds,
    );
}

// ---- W5：strip-assist artifact 层等价门禁（A5）----

#[test]
fn artifact_strip_assist_byte_identical() {
    use sophia_codegen::check_artifact_strip_equivalence;
    use sophia_stdlib::standard_registry;
    // 带丰富 Semantic Assist（meaning / not / because）的程序：移除 assist 前后 emit 的 .wasm
    // 必须逐字节相等（assist 不参与形式核心 / 值布局 / emit）。
    let src = "action Compute {\n\
       meaning: \"业务含义说明\"\n\
       because: \"设计理由\"\n\
       input { n: Int }\n\
       output { y: Int }\n\
       body { return n + n }\n\
     }";
    let sources = vec![(
        "d".to_string(),
        "d/actions/Compute.sophia".to_string(),
        src.to_string(),
    )];
    let registry = standard_registry();
    let outcome = check_artifact_strip_equivalence(&sources, &registry).expect("artifact diff");
    assert!(
        outcome.equivalent,
        "移除 assist 前后 .wasm 应逐字节相等：{:?}",
        outcome.detail
    );
}

#[test]
fn artifact_emit_is_deterministic() {
    use sophia_codegen::emit_from_sources;
    use sophia_stdlib::standard_registry;
    // 同一源码 emit 两次字节相等（确定性是 artifact 门禁的前提）。
    let src = "action Pair {\n\
       input { a: Int; b: Int }\n\
       output { y: Int }\n\
       body { return a * b }\n\
     }";
    let sources = vec![(
        "d".to_string(),
        "d/actions/Pair.sophia".to_string(),
        src.to_string(),
    )];
    let registry = standard_registry();
    let first = emit_from_sources(&sources, &registry, false).expect("emit 1");
    let second = emit_from_sources(&sources, &registry, false).expect("emit 2");
    assert_eq!(first, second, "同源码两次 emit 应字节相等（确定性）");
    assert_eq!(&first[0..4], b"\0asm", "应是合法 WASM");
}

#[test]
fn artifact_emit_uses_registry_library_sources() {
    use sophia_codegen::emit_from_sources;
    use sophia_library::{LibraryContent, LibraryRegistry};

    let content = LibraryContent {
        dir_name: "math_sophia".into(),
        manifest_toml: r#"
[library]
name = "math_sophia"
summary = "测试用纯 Sophia 数学库"
abi_version = 1

[surface]
sophia_sources = ["src/double.sophia"]

[prompt]
asset = "math_sophia.md"
"#
        .into(),
        asset_text: "测试资产".into(),
        sophia_sources: vec![(
            "src/double.sophia".into(),
            "action LibDouble { input { n: Int } output { y: Int } body { return n + n } }".into(),
        )],
        host_wasm: None,
    };
    let registry = LibraryRegistry::build(vec![content]).expect("registry");
    let sources = vec![(
        "d".to_string(),
        "d/actions/UseLib.sophia".to_string(),
        "action UseLib { input { n: Int } output { y: Int } body { return LibDouble(n) } }"
            .to_string(),
    )];

    let bytes = emit_from_sources(&sources, &registry, false).expect("emit with library source");
    assert_eq!(&bytes[0..4], b"\0asm", "应是合法 WASM");
}
