//! 评估拆解协议测试（workflow_graph_spec 4.2.2）。

mod common;

use common::snapshot;
use sophia_graph_db::*;

fn good_self_check() -> AssessmentSelfCheck {
    AssessmentSelfCheck {
        affects_only_visible_targets: true,
        no_hidden_answers: true,
        no_pseudocode_or_code: true,
    }
}

fn base_output() -> AssessmentLlmOutput {
    AssessmentLlmOutput {
        head: AssessmentPayload {
            risk: Risk::Medium,
            blast_radius: BlastRadius::Module,
            recommended_strategy: RecommendedStrategy::VerticalSlice,
            affected_systems: vec!["TodoDomain".into()],
            unknowns: vec![],
            notes: String::new(),
        },
        proposed_first_slice: None,
        proposed_invariants: vec![],
        proposed_recommended_action: DecisionAction::Decompose,
        self_check: good_self_check(),
    }
}

/// 建一个被评估的 ChangeRequest（human）。
fn change_request(store: &mut GraphStore) -> NodeId {
    store
        .as_human()
        .change_request(
            "cr",
            ChangeRequestPayload {
                kind: ChangeRequestKind::NewRequirement,
                request: "add feature".into(),
                priority: ChangePriority::Should,
            },
        )
        .unwrap()
}

#[test]
fn minimal_decomposition_creates_assessment_and_decision() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let cr = change_request(&mut store);
    let snap = snapshot(&mut store);

    let nodes = decompose_assessment(&mut store, &base_output(), cr, snap).unwrap();

    // Assessment + Decision 都创建了。
    assert_eq!(store.role_of(nodes.assessment), Some(NodeRole::Assessment));
    assert_eq!(store.role_of(nodes.decision), Some(NodeRole::Decision));
    assert!(nodes.first_slice.is_none());
    assert!(nodes.invariants.is_empty());

    // assesses→ cr、proposes→ decision、consumed→ snapshot 边存在。
    assert!(store.has_edge(nodes.assessment, cr, EdgeKind::Assesses));
    assert!(store.has_edge(nodes.assessment, nodes.decision, EdgeKind::Proposes));
    assert!(store.has_edge(nodes.assessment, snap, EdgeKind::Consumed));
    assert!(store.has_edge(nodes.decision, snap, EdgeKind::Consumed));

    // I6：LLM-provenance 的 Assessment / Decision 都有 consumed→ ContextSnapshot。
    assert!(store.validate_i6().is_ok());
}

#[test]
fn full_decomposition_with_first_slice_and_invariants() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let cr = change_request(&mut store);
    let snap = snapshot(&mut store);

    let mut output = base_output();
    output.proposed_first_slice = Some(FirstSlicePayload {
        purpose: "先实现核心切片".into(),
    });
    output.proposed_invariants = vec![
        ConstraintPayload {
            kind: ConstraintKind::Invariant,
            statement: "保持现有 API".into(),
            verifier: None,
        },
        ConstraintPayload {
            kind: ConstraintKind::Invariant,
            statement: "不破坏旧数据".into(),
            verifier: None,
        },
    ];

    let nodes = decompose_assessment(&mut store, &output, cr, snap).unwrap();

    let fs = nodes.first_slice.expect("first slice");
    assert_eq!(store.role_of(fs), Some(NodeRole::FirstSlice));
    assert!(store.has_edge(nodes.assessment, fs, EdgeKind::Proposes));

    assert_eq!(nodes.invariants.len(), 2);
    for inv in &nodes.invariants {
        assert_eq!(store.role_of(*inv), Some(NodeRole::Constraint));
        assert!(store.has_edge(nodes.assessment, *inv, EdgeKind::Proposes));
    }
    assert!(store.validate_i6().is_ok());
}

#[test]
fn failed_self_check_rejects_decomposition() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let cr = change_request(&mut store);
    let snap = snapshot(&mut store);

    let mut output = base_output();
    output.self_check.no_hidden_answers = false;

    let err = decompose_assessment(&mut store, &output, cr, snap).unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn non_invariant_proposed_constraint_rejected() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let cr = change_request(&mut store);
    let snap = snapshot(&mut store);

    let mut output = base_output();
    output.proposed_invariants = vec![ConstraintPayload {
        kind: ConstraintKind::Preference, // 必须是 Invariant。
        statement: "尽量简洁".into(),
        verifier: None,
    }];

    let err = decompose_assessment(&mut store, &output, cr, snap).unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn decision_carries_change_state_assessment() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let cr = change_request(&mut store);
    let snap = snapshot(&mut store);

    let nodes = decompose_assessment(&mut store, &base_output(), cr, snap).unwrap();
    let node = store.node(nodes.decision).unwrap();
    match &node.payload {
        NodePayload::Decision(d) => {
            assert_eq!(d.selected_action, DecisionAction::Decompose);
            assert!(matches!(d.state_assessment, StateAssessment::Change { .. }));
        }
        other => panic!("期望 Decision payload，得到 {other:?}"),
    }
}

#[test]
fn assessment_llm_output_strict_schema_rejects_extra_fields() {
    // deny_unknown_fields：多余字段应反序列化失败（strict 模式 1.3）。
    let json = r#"{
        "risk": "low",
        "blast_radius": "local",
        "recommended_strategy": "direct_change",
        "proposed_recommended_action": "design_solution",
        "self_check": {
            "affects_only_visible_targets": true,
            "no_hidden_answers": true,
            "no_pseudocode_or_code": true
        },
        "sneaky": true
    }"#;
    let parsed: Result<AssessmentLlmOutput, _> = serde_json::from_str(json);
    assert!(parsed.is_err(), "多余字段应被 strict schema 拒绝");
}

#[test]
fn assessment_llm_output_flatten_head_parses() {
    // head 通过 #[serde(flatten)] 内联。
    let json = r#"{
        "risk": "high",
        "blast_radius": "subsystem",
        "recommended_strategy": "staged_rollout",
        "proposed_recommended_action": "decompose",
        "self_check": {
            "affects_only_visible_targets": true,
            "no_hidden_answers": true,
            "no_pseudocode_or_code": true
        }
    }"#;
    let parsed: AssessmentLlmOutput = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.head.risk, Risk::High);
    assert_eq!(
        parsed.proposed_recommended_action,
        DecisionAction::Decompose
    );
}
