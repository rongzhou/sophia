//! 库插件演示 + 验收（见 docs/stdlib_design.md §六、docs/stdlib_implementation.md §2.3/§2.4）。
//!
//! 两个三方库形态,同一个确定 digest:
//! - `hash_sophia`（纯 Sophia 源码库,host=none）→ action `SophiaDigest(seed,value)`;
//! - `hash_wasm`（WASM-effect 库,host=WASM）→ op `WasmHash.Mix(seed,value)`,由 host.wasm 实现。
//!
//! 全确定（纯计算 + fixture 发现 + wasm 确定执行）→ 进 `cargo test`。验证:发现 + 注册表合并 +
//! 跨 domain 豁免 + 纯 Sophia 库执行 + WASM 库经 WasmHostFn 执行 + 两库等价。

use std::path::PathBuf;

use sophia_hir::{
    resolve_program_with_libraries, AsgIndex, IndexInput, LibrarySources, ProgramInput,
};
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

/// fixture 三方根目录（含 hash_sophia / hash_wasm）。
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sophia_libs")
}

/// 在 hash_wasm 库目录写入 host.wasm（wasm-encoder 手工 emit,见 docs/stdlib_design.md §六.1）。
/// 模块导出 `memory` + `wasm_hash_mix(i64,i64)->i64`,body 复刻 digest。确定字节、自包含。
fn ensure_host_wasm() -> PathBuf {
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
        MemorySection, MemoryType, Module, TypeSection, ValType,
    };

    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I64, ValType::I64], [ValType::I64]);
    module.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    // 导出 memory（ABI 要求:host 模块导出 "memory";本 demo 标量直传不实际用它,但 ABI 约定须备）。
    let mut mems = MemorySection::new();
    mems.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&mems);

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("wasm_hash_mix", ExportKind::Func, 0);
    module.section(&exports);

    // body: local acc(i64) = seed(param0); repeat 3 { acc = acc*31 + value(param1) }; return acc。
    // 局部 0,1 = params(seed, value);局部 2 = acc。
    let mut code = CodeSection::new();
    let locals = vec![(1u32, ValType::I64)]; // 1 个 i64 局部（acc）
    let mut f = Function::new(locals);
    // acc = seed
    f.instruction(&Instruction::LocalGet(0));
    f.instruction(&Instruction::LocalSet(2));
    // 展开 3 次（demo 不必用循环结构,展开更简单确定）
    for _ in 0..3 {
        // acc = acc*31 + value
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::I64Const(31));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::LocalSet(2));
    }
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    let bytes = module.finish();
    let path = fixture_root().join("hash_wasm/host.wasm");
    // 原子写入（temp + rename）：三个测试并行各自 ensure_host_wasm,而发现层（read_library_dir）
    // 现在会读 host.wasm——非原子写会让并发读撞上半写文件（unexpected EOF）。rename 同盘原子,
    // 并发读者要么见旧完整文件、要么见新完整文件,绝不见截断。temp 名带唯一后缀避免互撞。
    let tmp = fixture_root().join(format!(
        "hash_wasm/.host.wasm.{}.{}",
        std::process::id(),
        nanos()
    ));
    std::fs::write(&tmp, &bytes).expect("写入临时 host.wasm");
    std::fs::rename(&tmp, &path).expect("原子重命名 host.wasm");
    path
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
    ensure_host_wasm();
    let reg = full_registry_from(&[fixture_root()]).expect("发现 + 合并注册表");
    // 标准库仍在。
    assert!(reg.op("Http", "Get").is_some());
    assert!(reg.op("File", "Read").is_some());
    // 三方库:hash_wasm 的 effect-op + hash_sophia 的 Sophia 源码。
    let mix = reg.op("WasmHash", "Mix").expect("WasmHash.Mix");
    assert!(!mix.effectful, "Mix 是纯计算 op（effectful=false）");
    assert_eq!(mix.host_fn, "wasm_hash_mix");
    assert!(reg.is_library_family("WasmHash"));
    let srcs = reg.sophia_sources();
    assert_eq!(srcs.len(), 1, "hash_sophia 一个源码节点");
    assert_eq!(srcs[0].lib, "hash_sophia");
    assert_eq!(srcs[0].domain, "hash_sophia", "库名即 domain（隔离）");
}

#[test]
fn pure_sophia_library_resolves_and_runs_with_cross_domain_exemption() {
    ensure_host_wasm();
    let reg = full_registry_from(&[fixture_root()]).expect("注册表");
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

    let (_index, diags) = resolve_program_with_libraries(&inputs, &reg).expect("resolve");
    // 关键:用户 domain "app" 调库 domain "hash_sophia" 的 SophiaDigest,不应报 ImplicitCrossDomain。
    assert!(
        diags.is_empty(),
        "纯 Sophia 库跨 domain 调用应豁免,无诊断:{diags:?}"
    );
}

#[test]
fn both_libraries_compute_equal_digest() {
    let host_wasm_path = ensure_host_wasm();
    let reg = full_registry_from(&[fixture_root()]).expect("注册表");
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
    let index = AsgIndex::build(all_inputs)
        .expect("index")
        .with_libraries(&reg);

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
    let mut host = HostRegistry::new();
    host.register(
        "WasmHash",
        "Mix",
        Box::new(WasmHostFn::new_i64_i64_i64(&wasm_bytes, "wasm_hash_mix").expect("WasmHostFn")),
    );

    let (seed, value) = (7i64, 2i64);
    let want = expected_digest(seed, value);

    // 纯 Sophia 库路径:ViaSophia 调 SophiaDigest（库节点,解释执行,无需 host）。
    let (out_sophia, _) = sophia_runtime::run_action_with_host(
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
    let (out_wasm, _) = sophia_runtime::run_action_with_host(
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
