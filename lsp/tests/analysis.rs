//! LSP 语义分析核心测试（协议无关）。

use sophia_lsp::{DiagnosticSource, Workspace};

/// 计算某文档中某子串首次出现处的字节偏移（模拟光标定位）。
fn byte_of(source: &str, needle: &str) -> usize {
    source.find(needle).expect("substring present")
}

#[test]
fn clean_document_has_no_diagnostics() {
    let mut ws = Workspace::new();
    let src = "entity Todo { fields { id { type: Int } } }";
    ws.upsert("domains/TodoDomain/entities/Todo.sophia", "TodoDomain", src);
    assert!(ws
        .diagnostics("domains/TodoDomain/entities/Todo.sophia")
        .is_empty());
}

#[test]
fn syntax_error_reported() {
    let mut ws = Workspace::new();
    ws.upsert("d/A.sophia", "D", "entity Broken {");
    let diags = ws.diagnostics("d/A.sophia");
    assert!(!diags.is_empty());
    assert!(diags.iter().all(|d| d.source == DiagnosticSource::Syntax));
}

#[test]
fn hir_unresolved_type_reported() {
    let mut ws = Workspace::new();
    // 引用不存在的类型 Ghost。
    ws.upsert(
        "d/A.sophia",
        "D",
        "entity Todo { fields { s { type: Ghost } } }",
    );
    let diags = ws.diagnostics("d/A.sophia");
    assert!(
        diags
            .iter()
            .any(|d| d.source == DiagnosticSource::Hir && d.message.contains("Ghost")),
        "应报 HIR 未解析类型：{diags:?}"
    );
}

#[test]
fn semantic_diagnostic_reported() {
    let mut ws = Workspace::new();
    // action 用 print 但未声明 Console.Write effect → semantic UndeclaredEffect。
    let src = "action M { input { x: Int } output { y: Int } body { print \"hi\" return x } }";
    ws.upsert("d/M.sophia", "D", src);
    let diags = ws.diagnostics("d/M.sophia");
    assert!(
        diags.iter().any(|d| d.source == DiagnosticSource::Semantic),
        "应报 semantic 诊断：{diags:?}"
    );
}

#[test]
fn cross_document_diagnostics_are_attributed_correctly() {
    // 两个文档：A 干净，B 有未解析引用。B 的诊断不应出现在 A 上（span 偏移碰撞防护）。
    let mut ws = Workspace::new();
    ws.upsert(
        "d/A.sophia",
        "D",
        "entity Todo { fields { id { type: Int } } }",
    );
    ws.upsert(
        "d/B.sophia",
        "D",
        "entity Other { fields { g { type: Ghost } } }",
    );

    assert!(
        ws.diagnostics("d/A.sophia").is_empty(),
        "干净文档 A 不应有诊断：{:?}",
        ws.diagnostics("d/A.sophia")
    );
    assert!(!ws.diagnostics("d/B.sophia").is_empty(), "B 应有诊断");
}

#[test]
fn workspace_index_error_is_reported_not_silently_downgraded() {
    let mut ws = Workspace::new();
    ws.upsert(
        "d/A.sophia",
        "D",
        "entity Todo { fields { id { type: Int } } }",
    );
    ws.upsert(
        "d/B.sophia",
        "D",
        "entity Todo { fields { id { type: Int } } }",
    );

    let diags = ws.diagnostics("d/A.sophia");
    assert!(
        diags.iter().any(|d| {
            d.source == DiagnosticSource::Hir && d.message.contains("重复的节点名")
        }),
        "workspace-level HIR 错误应可见：{diags:?}"
    );
}

#[test]
fn hover_returns_symbol_info() {
    let mut ws = Workspace::new();
    let src = "entity Todo { fields { id { type: Int } } }";
    ws.upsert("d/Todo.sophia", "D", src);
    // 光标落在 "Todo" 上。
    let byte = byte_of(src, "Todo");
    let hover = ws.hover("d/Todo.sophia", byte).expect("hover");
    assert!(hover.contains("Todo"));
    assert!(hover.contains("Entity"));
}

#[test]
fn goto_definition_resolves_cross_document() {
    let mut ws = Workspace::new();
    // entity 在 A；action B 的 output 引用 Todo。
    ws.upsert(
        "d/Todo.sophia",
        "D",
        "entity Todo { fields { id { type: Int } } }",
    );
    let action_src = "action Make { input { x: Int } output { t: Todo } body { return x } }";
    ws.upsert("d/Make.sophia", "D", action_src);

    // 光标落在 action 中 output 的 "Todo" 引用上。
    let byte = byte_of(action_src, "Todo");
    let def = ws.goto_definition("d/Make.sophia", byte).expect("goto def");
    assert_eq!(def.name, "Todo");
    assert_eq!(def.uri, "d/Todo.sophia");
}

#[test]
fn symbols_lists_all_top_level_nodes() {
    let mut ws = Workspace::new();
    ws.upsert(
        "d/Todo.sophia",
        "D",
        "entity Todo { fields { id { type: Int } } }",
    );
    ws.upsert(
        "d/Status.sophia",
        "D",
        "state Status { value A { meaning: \"a\" } }",
    );
    let symbols = ws.symbols();
    assert!(symbols.contains_key("D::Todo"));
    assert!(symbols.contains_key("D::Status"));
}

#[test]
fn goto_definition_uses_current_document_domain_for_duplicate_names() {
    let mut ws = Workspace::new();
    ws.upsert(
        "domains/A/entities/Shared.sophia",
        "A",
        "entity Shared { fields { a { type: Int } } }",
    );
    ws.upsert(
        "domains/B/entities/Shared.sophia",
        "B",
        "entity Shared { fields { b { type: Int } } }",
    );
    let action_src = "action Make { input { x: Int } output { t: Shared } body { return x } }";
    ws.upsert("domains/A/actions/Make.sophia", "A", action_src);

    let byte = byte_of(action_src, "Shared");
    let def = ws
        .goto_definition("domains/A/actions/Make.sophia", byte)
        .expect("goto def");
    assert_eq!(def.domain, "A");
    assert_eq!(def.uri, "domains/A/entities/Shared.sophia");

    let symbols = ws.symbols();
    assert!(symbols.contains_key("A::Shared"));
    assert!(symbols.contains_key("B::Shared"));
}

#[test]
fn removed_document_has_no_diagnostics() {
    let mut ws = Workspace::new();
    ws.upsert("d/A.sophia", "D", "entity Broken {");
    assert!(!ws.diagnostics("d/A.sophia").is_empty());
    ws.remove("d/A.sophia");
    assert!(ws.diagnostics("d/A.sophia").is_empty());
}
