//! implement-loop 测试：implement → code_check → repair 预算受限闭环。

mod common;

use common::{seed_objective, seed_pseudocode, MockClient, StaticPrompts};
use sophia_engine::{run_implement_loop, ImplementLoopConfig, ImplementLoopOutcome};
use sophia_graph_db::{
    DiagnosticItem, DiagnosticKind, DiagnosticPayload, DiagnosticSeverity, EdgeKind, GraphStore,
    NodeId, NodeRole,
};
use sophia_llm::{LlmError, StructuredConfig};

fn ok_check() -> DiagnosticPayload {
    DiagnosticPayload {
        kind: DiagnosticKind::CodeCheck,
        ok: true,
        diagnostics: vec![],
    }
}

fn fail_check() -> DiagnosticPayload {
    DiagnosticPayload {
        kind: DiagnosticKind::CodeCheck,
        ok: false,
        diagnostics: vec![DiagnosticItem {
            code: "CHECK-TYPE-001".into(),
            severity: DiagnosticSeverity::Error,
            problem: "类型不匹配".into(),
            location: Some("D/A.sophia:1".into()),
        }],
    }
}

/// 建目标 + 伪代码节点（伪代码 consumed→ snapshot 满足 I6）。
fn setup(store: &mut GraphStore) -> (NodeId, NodeId) {
    let obj = seed_objective(store);
    let pseudo = seed_pseudocode(store);
    (obj, pseudo)
}

#[tokio::test]
async fn passes_on_first_check() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let (obj, pseudo) = setup(&mut store);
    let client = MockClient::new(vec![Ok(
        r#"{"files":[{"path":"D/A.sophia","content":"entity A {}"}]}"#.into(),
    )]);

    let outcome = run_implement_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &ImplementLoopConfig::default(),
        obj,
        pseudo,
        "# Purpose\n...",
        &[],
        |_files: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        ImplementLoopOutcome::Passed {
            code,
            files,
            artifacts,
            attempts,
        } => {
            assert_eq!(attempts, 1);
            assert_eq!(store.role_of(code), Some(NodeRole::Code));
            assert_eq!(files.len(), 1);
            assert_eq!(artifacts.len(), 1);
            // 应有一个 code_check DiagnosticNode 连 checks→ code。
            assert!(store
                .edges()
                .iter()
                .any(|e| e.kind == EdgeKind::Checks && e.to == code));
        }
        other => panic!("应一次通过，实际 {other:?}"),
    }
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn repairs_then_passes() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let (obj, pseudo) = setup(&mut store);
    // 首次 implement + 一次 repair。
    let client = MockClient::new(vec![
        Ok(r#"{"files":[{"path":"D/A.sophia","content":"entity A {"}]}"#.into()),
        Ok(
            r#"{"files":[{"path":"D/A.sophia","content":"entity A {}"}],"changes":["闭合花括号"]}"#
                .into(),
        ),
    ]);
    // 第一次 check 失败、第二次通过。
    let mut calls = 0u32;
    let outcome = run_implement_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &ImplementLoopConfig::default(),
        obj,
        pseudo,
        "# Purpose\n...",
        &[],
        |_files: &[(String, String)]| {
            calls += 1;
            if calls == 1 {
                fail_check()
            } else {
                ok_check()
            }
        },
    )
    .await
    .unwrap();

    match outcome {
        ImplementLoopOutcome::Passed {
            attempts,
            code,
            artifacts,
            ..
        } => {
            assert_eq!(attempts, 2);
            assert_eq!(artifacts.len(), 2);
            // 新 code 应有 repairs→ 旧 code 边。
            assert!(store
                .edges()
                .iter()
                .any(|e| e.kind == EdgeKind::Repairs && e.from == code));
        }
        other => panic!("应修复后通过，实际 {other:?}"),
    }
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn budget_exhausted_when_never_passes() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let (obj, pseudo) = setup(&mut store);
    // implement + 2 次 repair（max_repair_attempts=2）：共 3 个候选。
    let client = MockClient::new(vec![
        Ok(r#"{"files":[{"path":"D/A.sophia","content":"v1"}]}"#.into()),
        Ok(r#"{"files":[{"path":"D/A.sophia","content":"v2"}],"changes":["c2"]}"#.into()),
        Ok(r#"{"files":[{"path":"D/A.sophia","content":"v3"}],"changes":["c3"]}"#.into()),
    ]);
    let outcome = run_implement_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &ImplementLoopConfig {
            max_repair_attempts: 2,
            structured: StructuredConfig::default(),
        },
        obj,
        pseudo,
        "# Purpose\n...",
        &[],
        |_files: &[(String, String)]| fail_check(),
    )
    .await
    .unwrap();

    match outcome {
        ImplementLoopOutcome::BudgetExhausted {
            attempts,
            artifacts,
            ..
        } => {
            assert_eq!(attempts, 3); // 1 implement + 2 repair
            assert_eq!(artifacts.len(), 3);
            assert_eq!(artifacts[0].files[0].1, "v1");
            assert_eq!(artifacts[1].files[0].1, "v2");
            assert_eq!(artifacts[2].files[0].1, "v3");
        }
        other => panic!("应预算耗尽，实际 {other:?}"),
    }
    // 应有 3 个 code_check DiagnosticNode。
    let diag_count = store
        .nodes()
        .filter(|n| n.meta.role == NodeRole::Diagnostic)
        .count();
    assert_eq!(diag_count, 3);
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn implement_failure_bubbles_as_failed() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let (obj, pseudo) = setup(&mut store);
    let client = MockClient::new(vec![Err(LlmError::BackendUnavailable("down".into()))]);

    let outcome = run_implement_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &ImplementLoopConfig::default(),
        obj,
        pseudo,
        "# Purpose\n...",
        &[],
        |_files: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        ImplementLoopOutcome::Failed { raw_llm, error } => {
            assert!(matches!(error, LlmError::BackendUnavailable(_)));
            assert_eq!(store.role_of(raw_llm), Some(NodeRole::RawLlm));
            assert!(store.has_edge(raw_llm, obj, EdgeKind::Attempted));
        }
        other => panic!("后端不可用应失败，实际 {other:?}"),
    }
}

#[tokio::test]
async fn rejects_wrong_diagnostic_kind() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let (obj, pseudo) = setup(&mut store);
    let client = MockClient::new(vec![Ok(
        r#"{"files":[{"path":"D/A.sophia","content":"entity A {}"}]}"#.into(),
    )]);

    // 注入错误 kind（ConstraintAudit 而非 CodeCheck）。
    let err = run_implement_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &ImplementLoopConfig::default(),
        obj,
        pseudo,
        "# Purpose\n...",
        &[],
        |_files: &[(String, String)]| DiagnosticPayload {
            kind: DiagnosticKind::ConstraintAudit,
            ok: true,
            diagnostics: vec![],
        },
    )
    .await
    .unwrap_err();

    assert!(matches!(
        err,
        sophia_engine::ImplementLoopError::WrongDiagnosticKind(DiagnosticKind::ConstraintAudit)
    ));
}
