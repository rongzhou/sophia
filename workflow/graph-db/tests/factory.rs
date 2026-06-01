//! 工厂层测试：验证 provenance 由创建路径强制（N6）。

use sophia_graph_db::*;

#[test]
fn human_factory_fixes_human_provenance() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let id = store
        .as_human()
        .objective(
            "o",
            ObjectivePayload {
                title: "T".into(),
                description: "d".into(),
            },
        )
        .unwrap();
    assert_eq!(store.node(id).unwrap().meta.provenance, Provenance::Human);
}

#[test]
fn llm_factory_fixes_llm_provenance() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let id = store
        .as_llm()
        .decomposition(
            "d",
            DecompositionPayload {
                rationale: "split".into(),
                proposed_count: 2,
            },
        )
        .unwrap();
    assert_eq!(store.node(id).unwrap().meta.provenance, Provenance::Llm);
}

#[test]
fn deterministic_factory_fixes_deterministic_provenance() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let id = store
        .as_deterministic()
        .diagnostic(
            "diag",
            DiagnosticPayload {
                kind: DiagnosticKind::CodeCheck,
                ok: true,
                diagnostics: vec![],
            },
        )
        .unwrap();
    assert_eq!(
        store.node(id).unwrap().meta.provenance,
        Provenance::Deterministic
    );
}

#[test]
fn question_and_answer_get_correct_provenance_and_kind() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let q = store.as_llm().question("q", "why?").unwrap();
    let a = store.as_human().answer("a", "because").unwrap();
    assert_eq!(store.node(q).unwrap().meta.provenance, Provenance::Llm);
    assert_eq!(store.node(a).unwrap().meta.provenance, Provenance::Human);
}

#[test]
fn raw_llm_factory_fixes_failed_status() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let id = store
        .as_llm()
        .raw_llm(
            "raw",
            RawLlmPayload {
                failure_kind: RawLlmFailureKind::ValidationError,
                operation: "decision".into(),
                error_summary: "schema invalid".into(),
            },
        )
        .unwrap();
    assert_eq!(
        store.node(id).unwrap().meta.creation_status,
        NodeCreationStatus::Failed
    );
}

#[test]
fn baseline_decision_is_deterministic_decision() {
    let mut store = GraphStore::open_in_memory().unwrap();
    // 确定性 baseline 决策与 LLM 决策共用 Decision role，但 provenance 不同。
    let id = store
        .as_deterministic()
        .baseline_decision(
            "decide",
            DecisionPayload {
                selected_action: DecisionAction::DesignSolution,
                confidence: 0.5,
                rationale: "baseline".into(),
                state_assessment: StateAssessment::Goal {
                    goal_size: GoalSize::Small,
                    decomposition_pressure: Pressure::Low,
                    active_milestone_present: false,
                    outstanding_clarifications: 0,
                },
            },
        )
        .unwrap();
    let node = store.node(id).unwrap();
    assert_eq!(node.meta.role, NodeRole::Decision);
    assert_eq!(node.meta.provenance, Provenance::Deterministic);
}
