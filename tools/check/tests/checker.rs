//! Checker 集成测试：名称解析 + 语义 + strip-assist 等价门禁。

use sophia_check::check_program;

/// 构造 `(domain, path, source)` 列表。
fn src(domain: &str, path: &str, source: &str) -> (String, String, String) {
    (domain.into(), path.into(), source.into())
}

#[test]
fn clean_program_passes_all_checks() {
    let sources = vec![src(
        "Math",
        "domains/Math/actions/AddOne.sophia",
        "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }",
    )];
    let report = check_program(&sources).expect("check");
    assert!(report.hir.is_empty(), "无 HIR 诊断：{:?}", report.hir);
    assert!(
        report.semantic.is_empty(),
        "无语义诊断：{:?}",
        report.semantic
    );
    assert!(report.strip_assist.equivalent, "strip-assist 应等价");
    assert!(report.passed());
}

#[test]
fn strip_assist_equivalent_for_rich_assists() {
    // entity 带大量 Semantic Assist（meaning / not / semantic_identity / evolution）；
    // 移除后形式核心不变 → 门禁通过。
    let entity = r#"entity Todo {
  meaning: "A user task."
  not:
    "Not a calendar event."
    "Not auth data."
  fields {
    id { type: Int }
    title { type: Sanitized<Text> }
  }
  semantic_identity {
    core_capability: [ "task.lifecycle" ]
    forbidden_drift: [ "user.auth" ]
    drift_tolerance: 0.15
  }
  evolution {
    allowed: [ "add metadata fields" ]
    forbidden: [ "add network effects" ]
    requires_gate: [ "new top-level fields" ]
  }
}"#;
    let action = r#"action MakeTodo {
  meaning: "Construct a Todo."
  input { i: Int; t: Sanitized<Text> }
  output { todo: Todo }
  body { return Todo { id = i, title = t } }
}"#;
    let sources = vec![
        src(
            "TodoDomain",
            "domains/TodoDomain/entities/Todo.sophia",
            entity,
        ),
        src(
            "TodoDomain",
            "domains/TodoDomain/actions/MakeTodo.sophia",
            action,
        ),
    ];
    let report = check_program(&sources).expect("check");
    assert!(
        report.strip_assist.equivalent,
        "丰富 assist 移除后形式核心应不变：{:?}",
        report.strip_assist.detail
    );
    assert!(report.passed(), "整体应通过：{:?}", report.semantic);
}

#[test]
fn state_value_assists_stripped_equivalently() {
    let state = r#"state TodoStatus {
  value Pending { meaning: "未完成" }
  value Done { meaning: "已完成" }
}"#;
    let action = r#"action Classify {
  input { s: TodoStatus }
  output { y: Int }
  body {
    match s {
      TodoStatus.Pending => return 0
      TodoStatus.Done => return 1
    }
  }
}"#;
    let sources = vec![
        src("D", "domains/D/states/TodoStatus.sophia", state),
        src("D", "domains/D/actions/Classify.sophia", action),
    ];
    let report = check_program(&sources).expect("check");
    assert!(report.strip_assist.equivalent);
    assert!(report.passed());
}

#[test]
fn semantic_diagnostics_surface() {
    // 未声明 effect → 语义诊断；strip-assist 仍等价（assist 与该错误无关）。
    let sources = vec![src(
        "D",
        "domains/D/actions/Bad.sophia",
        "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }",
    )];
    let report = check_program(&sources).expect("check");
    assert!(!report.semantic.is_empty(), "应有语义诊断");
    assert!(!report.passed());
    // strip-assist 门禁独立于语义诊断，仍应等价。
    assert!(report.strip_assist.equivalent);
}

#[test]
fn hir_diagnostics_surface() {
    // 未解析类型 → HIR 诊断。
    let sources = vec![src(
        "D",
        "domains/D/entities/E.sophia",
        "entity E { fields { x { type: Ghost } } }",
    )];
    let report = check_program(&sources).expect("check");
    assert!(!report.hir.is_empty(), "应有 HIR 诊断");
    assert!(!report.passed());
}
