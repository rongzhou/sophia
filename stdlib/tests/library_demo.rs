//! 库插件演示 + 验收（见 docs/stdlib_design.md §六、docs/stdlib_implementation.md §2.3/§2.4）。
//!
//! 两个三方库形态,同一个确定 digest:
//! - `hash_sophia`（纯 Sophia 源码库,host=none）→ action `SophiaDigest(seed,value)`;
//! - `hash_wasm`（WASM-effect 库,host=WASM）→ op `WasmHash.Mix(seed,value)`,由 host.wasm 实现。
//!
//! 全确定（纯计算 + fixture 发现 + wasm 确定执行）→ 进 `cargo test`。验证:发现 + 注册表合并 +
//! 跨 domain 豁免 + 纯 Sophia 库执行 + WASM 库经 WasmHostFn 执行 + 两库等价。

use std::path::{Path, PathBuf};

use sophia_hir::{resolve_program, AsgIndex, IndexInput, LibrarySources, ProgramInput};
use sophia_runtime::{HostRegistry, Outcome, Value, WasmHostFn};
use sophia_semantic::analyze_program;
use sophia_stdlib::full_registry_from;
use sophia_syntax::{parse_ast, Ast};

/// digest(seed, value) = repeat 3 { acc = acc*31 + value }, acc0 = seed。两库共享。
fn expected_digest(seed: i64, value: i64) -> i64 {
    let mut acc = seed;
    for _ in 0..3 {
        acc = acc * 31 + value;
    }
    acc
}

/// fixture 模板三方根目录（含 hash_sophia / hash_wasm），测试只读。
fn fixture_template_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sophia_libs")
}

/// 为单个测试准备临时三方根目录，并在临时 hash_wasm 库目录写入 host.wasm。
fn prepared_fixture_root() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir()
        .join(format!(
            "sophia_stdlib_library_demo_{}_{}",
            std::process::id(),
            nanos()
        ))
        .join("sophia_libs");
    copy_dir(&fixture_template_root(), &root);
    let host_wasm = write_host_wasm(&root);
    (root, host_wasm)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("创建临时 fixture 目录");
    for entry in std::fs::read_dir(src).expect("读取 fixture 模板目录") {
        let entry = entry.expect("读取 fixture 模板条目");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("复制 fixture 模板文件");
        }
    }
}

/// 在指定 hash_wasm 库目录写入 host.wasm（wasm-encoder 手工 emit,见 docs/stdlib_design.md §六.1）。
/// 模块导出 `memory` + `sophia_alloc` + `sophia_read_copy` + `wasm_hash_mix(args_ptr,args_len)`。
/// `wasm_hash_mix` 读 ArgsWire、写 ValueWire result stash。确定字节、自包含。
fn write_host_wasm(root: &Path) -> PathBuf {
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

    // sophia_alloc(len): bump 分配,返回旧 bump。
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

    // sophia_read_copy(dst): 当前 provider 只返回 Int,stash 固定在 [8..17)。
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

    // wasm_hash_mix(args_ptr,args_len): decode ArgsWire(Int,Int),stash = ValueWire::Int(result),return 9。
    // locals: 0=args_ptr,1=args_len,2=seed,3=value,4=acc。
    let mut f = Function::new(vec![(3u32, ValType::I64)]);
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(5)); // argc(4) + tag(1)
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I64Load(mem(0, 0)));
    f.instruction(&Instruction::LocalSet(2));
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::I32Const(14)); // 4 + (tag+int64) + tag
    f.instruction(&Instruction::I32Add);
    f.instruction(&Instruction::I64Load(mem(0, 0)));
    f.instruction(&Instruction::LocalSet(3));
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::LocalSet(4));
    for _ in 0..3 {
        f.instruction(&Instruction::LocalGet(4));
        f.instruction(&Instruction::I64Const(31));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::LocalSet(4));
    }
    f.instruction(&Instruction::I32Const(8));
    f.instruction(&Instruction::I32Const(2)); // ValueWire Int tag
    f.instruction(&Instruction::I32Store8(mem(0, 0)));
    f.instruction(&Instruction::I32Const(9));
    f.instruction(&Instruction::LocalGet(4));
    f.instruction(&Instruction::I64Store(mem(0, 0)));
    f.instruction(&Instruction::I32Const(9));
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    let bytes = module.finish();
    let path = root.join("hash_wasm/host.wasm");
    // 原子写入（temp + rename）：发现层（read_library_dir）会读 host.wasm；即便未来测试并行复用
    // 同一个临时根,读者也只会见到完整文件。仓库 fixture 作为模板只读,避免测试污染工作区。
    let tmp = root.join(format!(
        "hash_wasm/.host.wasm.{}.{}",
        std::process::id(),
        nanos()
    ));
    std::fs::write(&tmp, &bytes).expect("写入临时 host.wasm");
    std::fs::rename(&tmp, &path).expect("原子重命名 host.wasm");
    path
}

fn mem(offset: u64, align: u32) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset,
        align,
        memory_index: 0,
    }
}

/// 单调纳秒戳（temp 文件唯一后缀）。
fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// 用户程序:一个 action 各调两库,返回二者（演示等价）。这里写两个入口分别取值。
fn user_program() -> Vec<(String, Ast)> {
    // 用户 domain "app";调库节点（SophiaDigest）+ 库 op（WasmHash.Mix）。
    let via_sophia = r#"action ViaSophia {
  input { seed: Int; value: Int }
  output { digest: Int }
  body { return SophiaDigest(seed, value) }
}"#;
    let via_wasm = r#"action ViaWasm {
  input { seed: Int; value: Int }
  output { digest: Int }
  body { return WasmHash.Mix(seed, value) }
}"#;
    vec![
        (
            "app/actions/ViaSophia.sophia".to_string(),
            parse_ast(via_sophia).expect("parse ViaSophia"),
        ),
        (
            "app/actions/ViaWasm.sophia".to_string(),
            parse_ast(via_wasm).expect("parse ViaWasm"),
        ),
    ]
}

#[test]
fn discovers_both_demo_libraries() {
    let (root, _) = prepared_fixture_root();
    let reg = full_registry_from(&[root]).expect("发现 + 合并注册表");
    // 标准库仍在。
    assert!(reg.op("Http", "Get").is_some());
    assert!(reg.op("File", "Read").is_some());
    // 三方库:hash_wasm 的 effect-op + hash_sophia/json 的 Sophia 源码。
    let mix = reg.op("WasmHash", "Mix").expect("WasmHash.Mix");
    assert!(!mix.effectful, "Mix 是纯计算 op（effectful=false）");
    assert_eq!(mix.host_fn, "wasm_hash_mix");
    assert!(reg.is_library_family("WasmHash"));
    let srcs = reg.sophia_sources();
    assert!(
        srcs.iter()
            .any(|s| s.lib == "hash_sophia" && s.domain == "hash_sophia"),
        "hash_sophia 源码库应被发现"
    );
    assert!(
        srcs.iter().any(|s| s.lib == "json" && s.domain == "json"),
        "json 源码库应被发现"
    );
}

fn json_user_program() -> Vec<(String, Ast)> {
    let check = r#"action CheckJson {
  input { text: Raw<Text> }
  output { result: one of { JsonValid, JsonInvalid } }
  body { return ValidateJson(text) }
}"#;
    vec![(
        "app/actions/CheckJson.sophia".to_string(),
        parse_ast(check).expect("parse CheckJson"),
    )]
}

fn analyze_with_library_sources<'a>(
    reg: &sophia_hir::LibraryRegistry,
    lib_srcs: &'a LibrarySources,
    user: &'a [(String, Ast)],
) -> (sophia_semantic::SemanticModel, Vec<&'a Ast>) {
    let mut all_inputs: Vec<IndexInput> = user
        .iter()
        .map(|(p, a)| IndexInput {
            domain: "app",
            path: p,
            ast: a,
        })
        .collect();
    let lib_inputs = lib_srcs.program_inputs();
    for pi in &lib_inputs {
        all_inputs.push(IndexInput {
            domain: pi.domain,
            path: pi.path,
            ast: pi.ast,
        });
    }
    let index = AsgIndex::build(all_inputs, reg).expect("index");
    let mut refs: Vec<&Ast> = user.iter().map(|(_, a)| a).collect();
    refs.extend(lib_srcs.asts());
    let analysis = analyze_program(&refs, &index);
    assert!(
        analysis.diagnostics.is_empty(),
        "程序应通过语义检查:{:?}",
        analysis.diagnostics
    );
    (analysis.model, refs)
}

fn json_outcome_name(outcome: Outcome) -> (String, i64, Option<String>) {
    match outcome {
        Outcome::Returned(Value::Entity { name, fields }) => {
            let position = match fields.get("position") {
                Some(Value::Int(i)) => *i,
                other => panic!("Json 结果缺 Int position 字段:{other:?}"),
            };
            let reason = match fields.get("reason") {
                Some(Value::Text(t)) => Some(t.clone()),
                None => None,
                other => panic!("JsonInvalid reason 应为 Text:{other:?}"),
            };
            (name, position, reason)
        }
        other => panic!("期望 JsonValid/JsonInvalid entity 返回，得到 {other:?}"),
    }
}

#[test]
fn json_validator_discovers_checks_and_runs_hidden_cases() {
    let (root, _) = prepared_fixture_root();
    let reg = full_registry_from(&[root]).expect("注册表");
    let lib_srcs = LibrarySources::from_registry(&reg).expect("解析库源码");
    let user = json_user_program();
    let (model, refs) = analyze_with_library_sources(&reg, &lib_srcs, &user);

    let valid = [
        "{}",
        "[]",
        "{\"ok\":true}",
        "{\"items\":[1,2,3]}",
        "{\"nested\":{\"a\":null}}",
        "\"hello\"",
        "-42",
        "true",
        "null",
        "  [ {\"x\": -12}, false, null, \"a\\\\b\" ] \n",
    ];
    let invalid = [
        "{",
        "[1,]",
        "{\"a\":1,}",
        "\"unterminated",
        "@",
        "{} x",
        "{\"bad\":\"\\u1234\"}",
        "1.2",
    ];

    for case in valid {
        let mut host = HostRegistry::new();
        let (outcome, _) = sophia_runtime::run_action(
            &model,
            &refs,
            "CheckJson",
            vec![Value::Text(case.into())],
            &mut host,
        )
        .expect("运行 valid JSON case");
        let (name, _, reason) = json_outcome_name(outcome);
        assert_eq!(name, "JsonValid", "应接受合法 JSON:{case}");
        assert!(reason.is_none(), "JsonValid 不应有 reason:{case}");
    }

    for case in invalid {
        let mut host = HostRegistry::new();
        let (outcome, _) = sophia_runtime::run_action(
            &model,
            &refs,
            "CheckJson",
            vec![Value::Text(case.into())],
            &mut host,
        )
        .expect("运行 invalid JSON case");
        let (name, _, reason) = json_outcome_name(outcome);
        assert_eq!(name, "JsonInvalid", "应拒绝非法 JSON:{case}");
        assert!(reason.is_some(), "JsonInvalid 应带 reason:{case}");
    }
}

#[test]
fn pure_sophia_library_resolves_and_runs_with_cross_domain_exemption() {
    let (root, _) = prepared_fixture_root();
    let reg = full_registry_from(&[root]).expect("注册表");
    let lib_srcs = LibrarySources::from_registry(&reg).expect("解析库源码");

    let user = user_program();
    // 合并:用户 inputs + 库源码 inputs → resolve（含跨 domain 豁免 + 库特殊根/op）。
    let mut inputs: Vec<ProgramInput> = user
        .iter()
        .map(|(p, a)| ProgramInput {
            domain: "app",
            path: p,
            ast: a,
        })
        .collect();
    inputs.extend(lib_srcs.program_inputs());

    let (_index, diags) = resolve_program(&inputs, &reg).expect("resolve");
    // 关键:用户 domain "app" 调库 domain "hash_sophia" 的 SophiaDigest,不应报 ImplicitCrossDomain。
    assert!(
        diags.is_empty(),
        "纯 Sophia 库跨 domain 调用应豁免,无诊断:{diags:?}"
    );
}

#[test]
fn both_libraries_compute_equal_digest() {
    let (root, host_wasm_path) = prepared_fixture_root();
    let reg = full_registry_from(&[root]).expect("注册表");
    let lib_srcs = LibrarySources::from_registry(&reg).expect("解析库源码");

    // 组装全程序 AST（用户 + 库源码）。
    let user = user_program();
    let mut all_inputs: Vec<IndexInput> = user
        .iter()
        .map(|(p, a)| IndexInput {
            domain: "app",
            path: p,
            ast: a,
        })
        .collect();
    let lib_inputs = lib_srcs.program_inputs();
    for pi in &lib_inputs {
        all_inputs.push(IndexInput {
            domain: pi.domain,
            path: pi.path,
            ast: pi.ast,
        });
    }
    let index = AsgIndex::build(all_inputs, &reg).expect("index");

    let mut refs: Vec<&Ast> = user.iter().map(|(_, a)| a).collect();
    refs.extend(lib_srcs.asts());
    let analysis = analyze_program(&refs, &index);
    assert!(
        analysis.diagnostics.is_empty(),
        "演示程序应通过语义检查:{:?}",
        analysis.diagnostics
    );

    // host:hash_wasm 的 WasmHash.Mix 由 host.wasm 经 WasmHostFn 提供。
    let wasm_bytes = std::fs::read(&host_wasm_path).expect("读 host.wasm");
    let mix = reg.op("WasmHash", "Mix").expect("WasmHash.Mix");
    let mut host = HostRegistry::new();
    host.register(
        "WasmHash",
        "Mix",
        Box::new(WasmHostFn::new(&wasm_bytes, mix).expect("WasmHostFn")),
    );

    let (seed, value) = (7i64, 2i64);
    let want = expected_digest(seed, value);

    // 纯 Sophia 库路径:ViaSophia 调 SophiaDigest（库节点,解释执行,无需 host）。
    let (out_sophia, _) = sophia_runtime::run_action(
        &analysis.model,
        &refs,
        "ViaSophia",
        vec![Value::Int(seed), Value::Int(value)],
        &mut host,
    )
    .expect("运行 ViaSophia");
    assert_eq!(
        out_sophia,
        Outcome::Returned(Value::Int(want)),
        "纯 Sophia 库 digest"
    );

    // WASM 库路径:ViaWasm 调 WasmHash.Mix（经 WasmHostFn 调 host.wasm）。
    let (out_wasm, _) = sophia_runtime::run_action(
        &analysis.model,
        &refs,
        "ViaWasm",
        vec![Value::Int(seed), Value::Int(value)],
        &mut host,
    )
    .expect("运行 ViaWasm");
    assert_eq!(
        out_wasm,
        Outcome::Returned(Value::Int(want)),
        "WASM 库 digest"
    );

    // 两库等价（同一确定 digest,surface 不同、host 不同,行为相同）。
    assert_eq!(out_sophia, out_wasm, "两库应计算相同 digest");
}
