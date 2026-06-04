//! W1 契约冻结测试（见 docs/wasm_codegen.md §三 / §九 W1）。
//!
//! 验证：① [`CodegenInput`] 由已检查程序的模型 + AST 构建，执行图与模型 callable 一致
//! （codegen 与解释器看到同一张图）；② W1 阶段 `emit_module` 诚实返回 `NotYetImplemented`
//! （不伪造产出）。

use sophia_codegen::{emit_from_sources, emit_module, CodegenError, CodegenInput};
use sophia_hir::{resolve_program, LibraryRegistry, ProgramInput};
use sophia_semantic::analyze_program;
use sophia_syntax::{parse_ast, Ast};

/// 解析 + 名称解析 + 语义分析一个程序，返回 (AST 集合, 语义模型)。
fn analyzed(sources: &[(&str, &str)]) -> (Vec<Ast>, sophia_semantic::SemanticModel) {
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
    let (index, _diags) = resolve_program(&inputs, &LibraryRegistry::empty()).expect("resolve");
    let refs: Vec<&Ast> = asts.iter().collect();
    let analysis = analyze_program(&refs, &index);
    assert!(
        analysis.diagnostics.is_empty(),
        "测试程序应通过语义检查：{:?}",
        analysis.diagnostics
    );
    (asts, analysis.model)
}

#[test]
fn input_graph_matches_model_callables() {
    // 两个 action，一个调用另一个：执行图应含两个节点 + 一条调用边（与解释器同源构图）。
    let (asts, model) = analyzed(&[
        (
            "d/actions/Double.sophia",
            "action Double { input { n: Int } output { y: Int } body { return n + n } }",
        ),
        (
            "d/actions/Quad.sophia",
            "action Quad { input { n: Int } output { y: Int } body { let d = Double(n) return d + d } }",
        ),
    ]);
    let refs: Vec<&Ast> = asts.iter().collect();
    let registry = sophia_stdlib::standard_registry();
    let input = CodegenInput::new(&model, &refs, &registry);

    // 模型与图一致：每个 callable 一个执行节点。
    for name in model.callables.keys() {
        assert!(
            input.graph().has_node(name),
            "callable `{name}` 应在执行图中有节点"
        );
    }
    // 跨调用边在图中物化（Quad → Double）。
    assert!(
        input.graph().has_call_edge("Quad", "Double"),
        "Quad → Double 调用边应在执行图中"
    );
    // 契约只读暴露三输入。
    assert_eq!(input.asts().len(), 2);
    assert!(input.model().callables.contains_key("Double"));
}

#[test]
fn emit_supported_program_produces_wasm_bytes() {
    // W2：标量核心程序应 emit 出合法的 WASM 字节（魔数 \0asm + 版本 1）。
    let (asts, model) = analyzed(&[(
        "d/actions/Id.sophia",
        "action Id { input { n: Int } output { y: Int } body { return n } }",
    )]);
    let refs: Vec<&Ast> = asts.iter().collect();
    let registry = sophia_stdlib::standard_registry();
    let input = CodegenInput::new(&model, &refs, &registry);

    let bytes = emit_module(&input).expect("W2 标量程序应 emit 出 WASM");
    assert_eq!(&bytes[0..4], b"\0asm", "应以 WASM 魔数开头");
    assert_eq!(&bytes[4..8], &[1, 0, 0, 0], "WASM 版本应为 1");
}

#[test]
fn emit_from_sources_rejects_hir_diagnostics() {
    let registry = LibraryRegistry::empty();
    let err = emit_from_sources(
        &[(
            "d".to_string(),
            "d/actions/Bad.sophia".to_string(),
            "action Bad { input { n: Int } output { y: Int } body { return Missing(n) } }"
                .to_string(),
        )],
        &registry,
        false,
    )
    .unwrap_err();
    assert!(matches!(err, CodegenError::InvalidInput(msg) if msg.contains("名称解析诊断未通过")));
}

#[test]
fn emit_from_sources_rejects_semantic_diagnostics() {
    let registry = LibraryRegistry::empty();
    let err = emit_from_sources(
        &[(
            "d".to_string(),
            "d/actions/Bad.sophia".to_string(),
            "action Bad { input { n: Int } output { y: Int } body { return \"x\" } }".to_string(),
        )],
        &registry,
        false,
    )
    .unwrap_err();
    assert!(matches!(err, CodegenError::InvalidInput(msg) if msg.contains("语义诊断未通过")));
}

#[test]
fn emit_is_honest_not_yet_implemented_for_unsupported() {
    // W2d 未覆盖构造（如 list）应诚实返回 NotYetImplemented，绝不伪造产出。
    let (asts, model) = analyzed(&[(
        "d/actions/Pack.sophia",
        "action Pack { input { a: Int; b: Int } output { xs: list of Int } body { return [a, b] } }",
    )]);
    let refs: Vec<&Ast> = asts.iter().collect();
    let registry = sophia_stdlib::standard_registry();
    let input = CodegenInput::new(&model, &refs, &registry);

    let result = emit_module(&input);
    assert!(
        matches!(result, Err(CodegenError::NotYetImplemented(_))),
        "未覆盖构造（list）应诚实返回 NotYetImplemented，得到 {result:?}"
    );
}
