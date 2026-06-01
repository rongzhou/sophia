//! 目标拆解图构造测试（workflow_graph_spec 4.1.4、5.3）。

mod common;

use common::snapshot;
use sophia_graph_db::*;

/// 建一个 human ObjectiveNode 作为被拆解的父目标。
fn objective(store: &mut GraphStore) -> NodeId {
    store
        .as_human()
        .objective(
            "big goal",
            ObjectivePayload {
                title: "实现完整待办系统".into(),
                description: "覆盖增删改查与状态流转".into(),
            },
        )
        .unwrap()
}

fn children() -> Vec<ChildGoal> {
    vec![
        ChildGoal {
            title: "新增待办".into(),
            description: "实现 AddTodo".into(),
        },
        ChildGoal {
            title: "完成待办".into(),
            description: "实现 CompleteTodo".into(),
        },
    ]
}

#[test]
fn build_decomposition_creates_decomposition_and_children() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let parent = objective(&mut store);
    let snap = snapshot(&mut store);

    let nodes = build_decomposition(
        &mut store,
        parent,
        snap,
        "目标过大，按用例拆分",
        &children(),
    )
    .unwrap();

    // Decomposition 节点（LLM provenance）。
    assert_eq!(
        store.role_of(nodes.decomposition),
        Some(NodeRole::Decomposition)
    );
    assert_eq!(
        store.provenance_of(nodes.decomposition),
        Some(Provenance::Llm)
    );
    // Decomposition consumed→ snapshot（I6 锚点）。
    assert!(store.has_edge(nodes.decomposition, snap, EdgeKind::Consumed));
    // proposed_count 与子目标数一致。
    match &store.node(nodes.decomposition).unwrap().payload {
        NodePayload::Decomposition(d) => {
            assert_eq!(d.proposed_count, 2);
            assert_eq!(d.rationale, "目标过大，按用例拆分");
        }
        _ => panic!("应为 Decomposition payload"),
    }

    // 两个子目标都是 Objective（LLM provenance）。
    assert_eq!(nodes.children.len(), 2);
    for c in &nodes.children {
        assert_eq!(store.role_of(*c), Some(NodeRole::Objective));
        assert_eq!(store.provenance_of(*c), Some(Provenance::Llm));
    }
}

#[test]
fn build_decomposition_wires_decomposes_and_member_of_edges() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let parent = objective(&mut store);
    let snap = snapshot(&mut store);

    let nodes = build_decomposition(&mut store, parent, snap, "拆分", &children()).unwrap();

    // parent decomposes→ Decomposition。
    assert!(store.has_edge(parent, nodes.decomposition, EdgeKind::Decomposes));
    // 每个子目标 member_of→ Decomposition。
    for c in &nodes.children {
        assert!(store.has_edge(*c, nodes.decomposition, EdgeKind::MemberOf));
    }
}

#[test]
fn build_decomposition_rejects_non_objective_parent() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let snap = snapshot(&mut store);
    // milestone 不是 Objective，不能作为 decomposes→ 的父。
    let ms = store
        .as_human()
        .milestone(
            "m",
            MilestonePayload {
                name: "M1".into(),
                summary: "s".into(),
            },
        )
        .unwrap();

    let err = build_decomposition(&mut store, ms, snap, "拆分", &children()).unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn build_decomposition_rejects_non_snapshot_anchor() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let parent = objective(&mut store);
    // 传一个非 ContextSnapshot 节点当 snapshot → 应拒绝（I6 锚点必须是 snapshot）。
    let not_snap = objective(&mut store);

    let err = build_decomposition(&mut store, parent, not_snap, "拆分", &children()).unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn build_decomposition_rejects_too_few_children() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let parent = objective(&mut store);
    let snap = snapshot(&mut store);
    let one = vec![ChildGoal {
        title: "唯一".into(),
        description: "拆成一个无意义".into(),
    }];

    let err = build_decomposition(&mut store, parent, snap, "拆分", &one).unwrap_err();
    assert!(matches!(err, GraphError::InvalidPayload(_)));
}

#[test]
fn decomposition_children_bind_after_human_accepts() {
    // binding 继承（5.3）：human 接受 Decomposition 后，member_of 的子目标变 bound。
    let mut store = GraphStore::open_in_memory().unwrap();
    let parent = objective(&mut store);
    let snap = snapshot(&mut store);
    let nodes = build_decomposition(&mut store, parent, snap, "拆分", &children()).unwrap();

    // 接受前：子目标未 bound（LLM provenance，无接受事件）。
    let ctx_before = derive_active_context(&store);
    let bound_ids_before: Vec<NodeId> = ctx_before.bound_objectives.iter().map(|o| o.id).collect();
    for c in &nodes.children {
        assert!(
            !bound_ids_before.contains(c),
            "接受前 LLM 派生子目标不应 bound"
        );
    }

    // 人类接受该 Decomposition。
    let acc = store
        .as_human()
        .acceptance_event(
            "accept decomposition",
            AcceptancePayload {
                decision: AcceptanceDecision::Satisfied,
                notes: "认可拆分".into(),
            },
        )
        .unwrap();
    store
        .append_edge(acc, nodes.decomposition, EdgeKind::Accepts)
        .unwrap();

    // 接受后：子目标沿 member_of 继承 binding。
    let ctx_after = derive_active_context(&store);
    let bound_ids_after: Vec<NodeId> = ctx_after.bound_objectives.iter().map(|o| o.id).collect();
    for c in &nodes.children {
        assert!(
            bound_ids_after.contains(c),
            "接受 Decomposition 后子目标应 bound（5.3 继承）"
        );
    }

    // 图整体满足 I6（结构性派生节点不需 consumed→）。
    store.validate_i6().unwrap();
}
