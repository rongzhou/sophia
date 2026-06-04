//! 差测试（W3 起的核心）：解释器（oracle）vs WASM 后端逐 case 等价。
//!
//! 见 docs/wasm_codegen.md §七。对每个程序 + 每组实参：① 解释器执行得 `Outcome`（oracle）；
//! ② codegen emit `.wasm`、用 `sophia-runtime` 的非浏览器 runner 执行得 `Outcome'`；③ 断言两者等价。
//!
//! **首要不变量**：解释器是唯一语义真相源。任何不一致 = WASM 后端有 bug，**绝不**调和 / 伪造一致。
//!
//! W2 覆盖标量核心（Int / Bool / Null + 算术 / 比较 / 布尔 / 一元 / if-else / let-set / return /
//! 跨调用），故差测试程序限于该子集；含 match / Text / effect 等的程序待后续增量纳入。

use sophia_codegen::{emit_module, CodegenInput};
use sophia_hir::{resolve_program, LibraryContent, LibraryRegistry, ProgramInput};
use sophia_runtime::{run_action, HostRegistry, Outcome, Value, WasmProgramRunner};
use sophia_semantic::{analyze_program, SemanticModel};
use sophia_stdlib::{
    register_mock_hosts, register_wasm_library_hosts, standard_registry, MockBuckets,
};
use sophia_syntax::{parse_ast, Ast};

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
    analyze_with_registry(sources, &standard_registry())
}

fn analyze_with_registry(
    sources: &[(&str, &str)],
    registry: &LibraryRegistry,
) -> (Vec<Ast>, SemanticModel) {
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
    let (index, _d) = resolve_program(&inputs, registry).expect("resolve");
    let refs: Vec<&Ast> = asts.iter().collect();
    let analysis = analyze_program(&refs, &index);
    assert!(
        analysis.diagnostics.is_empty(),
        "测试程序应通过语义检查：{:?}",
        analysis.diagnostics
    );
    (asts, analysis.model)
}

fn wasm_hash_registry() -> LibraryRegistry {
    LibraryRegistry::build(vec![LibraryContent {
        dir_name: "hash_wasm".into(),
        manifest_toml: r#"
[library]
name = "hash_wasm"
summary = "WASM hash test host"
abi_version = 1

[[op]]
family = "WasmHash"
op = "Mix"
params = ["Int", "Int"]
returns = "Int"
effectful = false
host_fn = "wasm_hash_mix"

[prompt]
asset = "hash_wasm.md"
"#
        .into(),
        asset_text: "hash wasm test".into(),
        sophia_sources: vec![],
        host_wasm: None,
    }])
    .expect("hash_wasm registry")
}

fn wasm_hash_provider_registry() -> LibraryRegistry {
    LibraryRegistry::build(vec![LibraryContent {
        dir_name: "hash_wasm".into(),
        manifest_toml: r#"
[library]
name = "hash_wasm"
summary = "WASM hash test host"
abi_version = 1

[[op]]
family = "WasmHash"
op = "Mix"
params = ["Int", "Int"]
returns = "Int"
effectful = false
host_fn = "wasm_hash_mix"

[prompt]
asset = "hash_wasm.md"
"#
        .into(),
        asset_text: "hash wasm test".into(),
        sophia_sources: vec![],
        host_wasm: Some(hash_provider_wasm()),
    }])
    .expect("hash_wasm provider registry")
}

fn hash_provider_wasm() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
        GlobalSection, GlobalType, Instruction, MemorySection, MemoryType, Module, TypeSection,
        ValType,
    };

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], [ValType::I32]); // sophia_alloc
    types.ty().function([ValType::I32], []); // sophia_read_copy
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]); // wasm_hash_mix
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    funcs.function(1);
    funcs.function(2);
    module.section(&funcs);

    let mut mems = MemorySection::new();
    mems.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&mems);

    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(1024),
    );
    module.section(&globals);

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("sophia_alloc", ExportKind::Func, 0);
    exports.export("sophia_read_copy", ExportKind::Func, 1);
    exports.export("wasm_hash_mix", ExportKind::Func, 2);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut alloc = Function::new(vec![(1u32, ValType::I32)]);
    alloc.instruction(&Instruction::GlobalGet(0));
    alloc.instruction(&Instruction::LocalSet(1));
    alloc.instruction(&Instruction::GlobalGet(0));
    alloc.instruction(&Instruction::LocalGet(0));
    alloc.instruction(&Instruction::I32Add);
    alloc.instruction(&Instruction::GlobalSet(0));
    alloc.instruction(&Instruction::LocalGet(1));
    alloc.instruction(&Instruction::End);
    code.function(&alloc);

    let mut read_copy = Function::new(vec![]);
    read_copy.instruction(&Instruction::LocalGet(0));
    read_copy.instruction(&Instruction::I32Const(8));
    read_copy.instruction(&Instruction::I32Load8U(mem(0, 0)));
    read_copy.instruction(&Instruction::I32Store8(mem(0, 0)));
    read_copy.instruction(&Instruction::LocalGet(0));
    read_copy.instruction(&Instruction::I32Const(1));
    read_copy.instruction(&Instruction::I32Add);
    read_copy.instruction(&Instruction::I32Const(9));
    read_copy.instruction(&Instruction::I64Load(mem(0, 0)));
    read_copy.instruction(&Instruction::I64Store(mem(0, 0)));
    read_copy.instruction(&Instruction::End);
    code.function(&read_copy);

    let mut mix = Function::new(vec![(3u32, ValType::I64)]);
    mix.instruction(&Instruction::LocalGet(0));
    mix.instruction(&Instruction::I32Const(5));
    mix.instruction(&Instruction::I32Add);
    mix.instruction(&Instruction::I64Load(mem(0, 0)));
    mix.instruction(&Instruction::LocalSet(2));
    mix.instruction(&Instruction::LocalGet(0));
    mix.instruction(&Instruction::I32Const(14));
    mix.instruction(&Instruction::I32Add);
    mix.instruction(&Instruction::I64Load(mem(0, 0)));
    mix.instruction(&Instruction::LocalSet(3));
    mix.instruction(&Instruction::LocalGet(2));
    mix.instruction(&Instruction::LocalSet(4));
    for _ in 0..3 {
        mix.instruction(&Instruction::LocalGet(4));
        mix.instruction(&Instruction::I64Const(31));
        mix.instruction(&Instruction::I64Mul);
        mix.instruction(&Instruction::LocalGet(3));
        mix.instruction(&Instruction::I64Add);
        mix.instruction(&Instruction::LocalSet(4));
    }
    mix.instruction(&Instruction::I32Const(8));
    mix.instruction(&Instruction::I32Const(2));
    mix.instruction(&Instruction::I32Store8(mem(0, 0)));
    mix.instruction(&Instruction::I32Const(9));
    mix.instruction(&Instruction::LocalGet(4));
    mix.instruction(&Instruction::I64Store(mem(0, 0)));
    mix.instruction(&Instruction::I32Const(9));
    mix.instruction(&Instruction::End);
    code.function(&mix);
    module.section(&code);

    module.finish()
}

fn mem(offset: u64, align: u32) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset,
        align,
        memory_index: 0,
    }
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
    outcome_to_scalar(outcome)
}

fn outcome_to_scalar(outcome: Outcome) -> ScalarOutcome {
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

/// WASM 后端执行：emit + runtime 非浏览器 runner 加载执行 + 读回结局。
fn run_wasm(
    model: &SemanticModel,
    asts: &[&Ast],
    entry: &str,
    args: &[Arg],
    entry_is_action: bool,
    seeds: &Seeds,
    registry: &LibraryRegistry,
) -> ScalarOutcome {
    let input = CodegenInput::new(model, asts, registry);
    let bytes = emit_module(&input).expect("emit wasm");
    let runner = WasmProgramRunner::new(&bytes, registry).expect("wasm runner");
    let buckets = MockBuckets::new();
    for (p, c) in &seeds.files {
        buckets.seed_file(*p, *c);
    }
    for (u, b) in &seeds.http {
        buckets.seed_http(*u, *b);
    }
    let mut host = HostRegistry::new();
    register_mock_hosts(&mut host, &buckets);
    host.register_fn("WasmHash", "Mix", |args| {
        let [Value::Int(a), Value::Int(b)] = args else {
            return Err("WasmHash.Mix 参数 ABI 不匹配".into());
        };
        Ok(Value::Int(a * 31 + b * 17))
    });
    let values: Vec<Value> = args.iter().map(Arg::to_value).collect();
    let outcome = runner
        .run(model, entry, &values, entry_is_action, &mut host)
        .expect("wasm runner 执行");
    outcome_to_scalar(outcome)
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
    let registry = standard_registry();
    let interp = run_interp(
        &model,
        &refs,
        entry,
        args.iter().map(|a| a.to_value()).collect(),
        seeds,
    );
    let wasm = run_wasm(&model, &refs, entry, args, action, seeds, &registry);
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
    assert_equiv(sources, "CountUntil", &[4_294_967_296, 3], true); // 大 i64 计数仍进首轮
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
fn diff_registry_dynamic_import_for_third_party_op() {
    // 非 File/Http 的 registry op：codegen 必须从 registry 派生 import，而不是走标准库硬编码分支。
    let registry = wasm_hash_registry();
    let act = "action ViaWasm {\n\
       input { a: Int; b: Int }\n\
       output { y: Int }\n\
       body { return WasmHash.Mix(a, b) }\n\
     }";
    let sources = &[("d/actions/ViaWasm.sophia", act)];
    let (asts, model) = analyze_with_registry(sources, &registry);
    let refs: Vec<&Ast> = asts.iter().collect();
    let mut host = HostRegistry::new();
    host.register_fn("WasmHash", "Mix", |args| {
        let [Value::Int(a), Value::Int(b)] = args else {
            return Err("WasmHash.Mix 参数 ABI 不匹配".into());
        };
        Ok(Value::Int(a * 31 + b * 17))
    });
    let (outcome, _trace) = run_action(
        &model,
        &refs,
        "ViaWasm",
        vec![Value::Int(7), Value::Int(11)],
        &mut host,
    )
    .expect("解释执行");
    let interp = match outcome {
        Outcome::Returned(Value::Int(i)) => ScalarOutcome::Int(i),
        other => panic!("ViaWasm 应返回 Int，得到 {other:?}"),
    };
    let wasm = run_wasm(
        &model,
        &refs,
        "ViaWasm",
        &[Arg::Int(7), Arg::Int(11)],
        true,
        &Seeds::default(),
        &registry,
    );
    assert_eq!(interp, wasm);
}

#[test]
fn diff_dynamic_import_uses_third_party_value_wire_provider() {
    // VM 模式真实链路：program.wasm import → HostRegistry → 三方 host.wasm provider。
    // 这里不注册 Rust 闭包，避免把 provider ABI 的缺口藏起来。
    let registry = wasm_hash_provider_registry();
    let act = "action ViaWasm {\n\
       input { a: Int; b: Int }\n\
       output { y: Int }\n\
       body { return WasmHash.Mix(a, b) }\n\
     }";
    let sources = &[("d/actions/ViaWasm.sophia", act)];
    let (asts, model) = analyze_with_registry(sources, &registry);
    let refs: Vec<&Ast> = asts.iter().collect();
    let input = CodegenInput::new(&model, &refs, &registry);
    let bytes = emit_module(&input).expect("emit wasm");
    let runner = WasmProgramRunner::new(&bytes, &registry).expect("wasm runner");

    let mut host = HostRegistry::new();
    register_wasm_library_hosts(&mut host, &registry).expect("注册三方 ValueWire provider");
    let outcome = runner
        .run(
            &model,
            "ViaWasm",
            &[Value::Int(7), Value::Int(11)],
            true,
            &mut host,
        )
        .expect("provider VM 执行");
    assert_eq!(
        outcome,
        Outcome::Returned(Value::Int((7 * 31 + 11) * 31 * 31 + 11 * 31 + 11))
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
