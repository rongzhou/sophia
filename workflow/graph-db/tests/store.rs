//! GraphStore 不变量与事件溯源测试。
//!
//! 节点创建经 provenance 分组工厂入口（N6）：`as_human` / `as_llm` / `as_deterministic`。

use sophia_graph_db::*;

fn obj_payload(title: &str) -> ObjectivePayload {
    ObjectivePayload {
        title: title.into(),
        description: "desc".into(),
    }
}

fn ctx_snapshot() -> ContextSnapshotPayload {
    ContextSnapshotPayload {
        schema_version: 1,
        snapshot: serde_json::json!({}),
        digest: "a".repeat(64),
    }
}

#[test]
fn append_node_assigns_sequential_ids() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let a = store.as_human().objective("a", obj_payload("A")).unwrap();
    let b = store.as_human().objective("b", obj_payload("B")).unwrap();
    assert_eq!(a.as_string(), "N0001");
    assert_eq!(b.as_string(), "N0002");
}

#[test]
fn empty_summary_rejected() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let err = store
        .as_human()
        .objective("   ", obj_payload("O"))
        .unwrap_err();
    assert!(matches!(err, GraphError::EmptySummary));
}

#[test]
fn rawllm_must_be_failed_and_llm() {
    let mut store = GraphStore::open_in_memory().unwrap();
    // raw_llm 工厂固定 provenance=Llm、creation_status=Failed。
    let ok = store.as_llm().raw_llm(
        "raw",
        RawLlmPayload {
            failure_kind: RawLlmFailureKind::ExecutionError,
            operation: "design".into(),
            error_summary: "boom".into(),
        },
    );
    assert!(ok.is_ok());
    let node = store.node(ok.unwrap()).unwrap();
    assert_eq!(node.meta.creation_status, NodeCreationStatus::Failed);
    assert_eq!(node.meta.provenance, Provenance::Llm);
}

#[test]
fn i3_edge_role_constraint_enforced() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = store.as_human().objective("o", obj_payload("O")).unwrap();
    let crit = store
        .as_human()
        .acceptance_criterion(
            "c",
            AcceptanceCriterionPayload {
                statement: "must".into(),
                verifier: None,
            },
        )
        .unwrap();
    // validated_by: Objective → AcceptanceCriterion 合法。
    assert!(store.append_edge(obj, crit, EdgeKind::ValidatedBy).is_ok());
    // selects: Objective → AcceptanceCriterion 非法。
    let err = store.append_edge(obj, crit, EdgeKind::Selects).unwrap_err();
    assert!(matches!(err, GraphError::InvalidEdge { .. }));
}

#[test]
fn i5_dangling_edge_rejected() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = store.as_human().objective("o", obj_payload("O")).unwrap();
    let ghost = NodeId(999);
    let err = store
        .append_edge(obj, ghost, EdgeKind::ValidatedBy)
        .unwrap_err();
    assert!(matches!(err, GraphError::DanglingReference(_)));
}

#[test]
fn i4_supersedes_same_role_required() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = store.as_human().objective("o", obj_payload("O")).unwrap();
    let ms = store
        .as_human()
        .milestone(
            "m",
            MilestonePayload {
                name: "M".into(),
                summary: "s".into(),
            },
        )
        .unwrap();
    let err = store
        .append_edge(obj, ms, EdgeKind::Supersedes)
        .unwrap_err();
    assert!(matches!(err, GraphError::InvalidEdge { .. }));
}

#[test]
fn i4_supersedes_no_cycle() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let v1 = store.as_human().objective("v1", obj_payload("V1")).unwrap();
    let v2 = store.as_human().objective("v2", obj_payload("V2")).unwrap();
    assert!(store.append_edge(v2, v1, EdgeKind::Supersedes).is_ok());
    let err = store.append_edge(v1, v2, EdgeKind::Supersedes).unwrap_err();
    assert!(matches!(err, GraphError::InvalidSupersedes(_)));
}

#[test]
fn i4_supersedes_single_outgoing() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let v1 = store.as_human().objective("v1", obj_payload("V1")).unwrap();
    let v2 = store.as_human().objective("v2", obj_payload("V2")).unwrap();
    let v3 = store.as_human().objective("v3", obj_payload("V3")).unwrap();
    assert!(store.append_edge(v2, v1, EdgeKind::Supersedes).is_ok());
    let err = store.append_edge(v2, v3, EdgeKind::Supersedes).unwrap_err();
    assert!(matches!(err, GraphError::InvalidSupersedes(_)));
}

#[test]
fn i6_llm_node_requires_consumed_edge() {
    let mut store = GraphStore::open_in_memory().unwrap();
    store
        .as_llm()
        .pseudocode(
            "p",
            PseudocodePayload {
                purpose: "do".into(),
                artifact_path: "content.pseudo".into(),
            },
        )
        .unwrap();
    assert!(store.validate_i6().is_err());

    let snap = store
        .as_deterministic()
        .context_snapshot("snap", ctx_snapshot())
        .unwrap();
    let pseudo = NodeId(1);
    store.append_edge(pseudo, snap, EdgeKind::Consumed).unwrap();
    assert!(store.validate_i6().is_ok());
}

#[test]
fn pseudocode_artifact_path_enforced() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let err = store
        .as_llm()
        .pseudocode(
            "p",
            PseudocodePayload {
                purpose: "do".into(),
                artifact_path: "wrong.pseudo".into(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn context_snapshot_digest_format_enforced() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let err = store
        .as_deterministic()
        .context_snapshot(
            "s",
            ContextSnapshotPayload {
                schema_version: 1,
                snapshot: serde_json::json!({}),
                digest: "TOOSHORT".into(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn answers_requires_answer_to_question() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let q1 = store.as_llm().question("q1", "q1?").unwrap();
    let q2 = store.as_llm().question("q2", "q2?").unwrap();
    let ans = store.as_human().answer("a", "ans").unwrap();
    // question answers question → 拒绝（kind 约束）。
    assert!(matches!(
        store.append_edge(q1, q2, EdgeKind::Answers).unwrap_err(),
        GraphError::InvalidPayload(_)
    ));
    // answer answers question → 合法。
    assert!(store.append_edge(ans, q1, EdgeKind::Answers).is_ok());
}

#[test]
fn requires_only_invariant_constraint() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let ms = store
        .as_human()
        .milestone(
            "m",
            MilestonePayload {
                name: "M".into(),
                summary: "s".into(),
            },
        )
        .unwrap();
    let pref = store
        .as_human()
        .constraint(
            "c",
            ConstraintPayload {
                kind: ConstraintKind::Preference,
                statement: "soft".into(),
                verifier: None,
            },
        )
        .unwrap();
    let inv = store
        .as_human()
        .constraint(
            "i",
            ConstraintPayload {
                kind: ConstraintKind::Invariant,
                statement: "keep".into(),
                verifier: None,
            },
        )
        .unwrap();
    assert!(matches!(
        store.append_edge(ms, pref, EdgeKind::Requires).unwrap_err(),
        GraphError::InvalidPayload(_)
    ));
    assert!(store.append_edge(ms, inv, EdgeKind::Requires).is_ok());
}

#[test]
fn excludes_only_out_of_scope_constraint() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let ms = store
        .as_human()
        .milestone(
            "m",
            MilestonePayload {
                name: "M".into(),
                summary: "s".into(),
            },
        )
        .unwrap();
    let inv = store
        .as_human()
        .constraint(
            "i",
            ConstraintPayload {
                kind: ConstraintKind::Invariant,
                statement: "keep".into(),
                verifier: None,
            },
        )
        .unwrap();
    assert!(matches!(
        store.append_edge(ms, inv, EdgeKind::Excludes).unwrap_err(),
        GraphError::InvalidPayload(_)
    ));
}

#[test]
fn event_sourcing_replay_round_trips() {
    let dir = std::env::temp_dir().join(format!("sophia_graph_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("graph.sqlite");

    let (o, c);
    {
        let mut store = GraphStore::open(&db).unwrap();
        o = store.as_human().objective("o", obj_payload("O")).unwrap();
        c = store
            .as_human()
            .constraint(
                "c",
                ConstraintPayload {
                    kind: ConstraintKind::Invariant,
                    statement: "keep".into(),
                    verifier: None,
                },
            )
            .unwrap();
        store.append_edge(o, c, EdgeKind::ConstrainedBy).unwrap();
    }

    {
        let store = GraphStore::open(&db).unwrap();
        assert!(store.node(o).is_some());
        assert!(store.node(c).is_some());
        assert_eq!(store.edges().len(), 1);
        assert_eq!(store.edges()[0].kind, EdgeKind::ConstrainedBy);
        let mut s2 = store;
        let next = s2.as_human().objective("o3", obj_payload("O3")).unwrap();
        assert_eq!(next.as_string(), "N0003");
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn node_id_roundtrip() {
    assert_eq!(NodeId(1).as_string(), "N0001");
    assert_eq!(NodeId::parse("N0042"), Some(NodeId(42)));
    assert_eq!(NodeId::parse("bad"), None);
}

#[test]
fn provenance_role_matrix_logic() {
    // I2 矩阵直接覆盖（工厂已封住伪造路径，这里校验底层判定）。
    assert!(Provenance::Human.allowed_for(NodeRole::Objective));
    assert!(Provenance::Llm.allowed_for(NodeRole::Objective));
    assert!(!Provenance::Deterministic.allowed_for(NodeRole::Objective));

    assert!(Provenance::Llm.allowed_for(NodeRole::Decomposition));
    assert!(!Provenance::Human.allowed_for(NodeRole::Decomposition));

    assert!(Provenance::Human.allowed_for(NodeRole::ChangeRequest));
    assert!(!Provenance::Llm.allowed_for(NodeRole::ChangeRequest));

    assert!(Provenance::Deterministic.allowed_for(NodeRole::Diagnostic));
    assert!(!Provenance::Llm.allowed_for(NodeRole::Diagnostic));

    // Decision 接受 Llm 或 Deterministic（baseline）。
    assert!(Provenance::Llm.allowed_for(NodeRole::Decision));
    assert!(Provenance::Deterministic.allowed_for(NodeRole::Decision));
    assert!(!Provenance::Human.allowed_for(NodeRole::Decision));
}
