//! Active Context 推导测试（workflow_graph_spec 第五节）。
//!
//! 节点创建经 provenance 分组工厂入口（N6）。

use sophia_graph_db::*;

fn obj(title: &str) -> ObjectivePayload {
    ObjectivePayload {
        title: title.into(),
        description: "d".into(),
    }
}

/// 人类目标（隐式接受）。
fn human_objective(store: &mut GraphStore, title: &str) -> NodeId {
    store.as_human().objective(title, obj(title)).unwrap()
}

/// LLM 目标（需显式接受）。
fn llm_objective(store: &mut GraphStore, title: &str) -> NodeId {
    store.as_llm().objective(title, obj(title)).unwrap()
}

fn acceptance(store: &mut GraphStore) -> NodeId {
    store
        .as_human()
        .acceptance_event(
            "accept",
            AcceptancePayload {
                decision: AcceptanceDecision::Accepted,
                notes: String::new(),
            },
        )
        .unwrap()
}

fn withdrawal(store: &mut GraphStore) -> NodeId {
    store
        .as_human()
        .withdrawal_event(
            "withdraw",
            WithdrawalPayload {
                reason: "no longer needed".into(),
            },
        )
        .unwrap()
}

fn milestone(store: &mut GraphStore, human: bool) -> NodeId {
    let p = MilestonePayload {
        name: "M".into(),
        summary: "s".into(),
    };
    if human {
        store.as_human().milestone("M", p).unwrap()
    } else {
        store.as_llm().milestone("M", p).unwrap()
    }
}

#[test]
fn human_objective_is_bound_implicitly() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let o = human_objective(&mut store, "Goal");
    let ctx = derive_active_context(&store);
    assert_eq!(ctx.bound_objectives.len(), 1);
    assert_eq!(ctx.bound_objectives[0].id, o);
}

#[test]
fn llm_objective_unbound_until_accepted() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let o = llm_objective(&mut store, "AI goal");
    assert!(derive_active_context(&store).bound_objectives.is_empty());

    let acc = acceptance(&mut store);
    store.append_edge(acc, o, EdgeKind::Accepts).unwrap();
    let ctx = derive_active_context(&store);
    assert_eq!(ctx.bound_objectives.len(), 1);
    assert_eq!(ctx.bound_objectives[0].id, o);
}

#[test]
fn withdrawal_after_acceptance_unbinds() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let o = llm_objective(&mut store, "AI goal");
    let acc = acceptance(&mut store);
    store.append_edge(acc, o, EdgeKind::Accepts).unwrap();
    assert_eq!(derive_active_context(&store).bound_objectives.len(), 1);

    std::thread::sleep(std::time::Duration::from_millis(5));
    let wd = withdrawal(&mut store);
    store.append_edge(wd, o, EdgeKind::Withdraws).unwrap();
    assert!(derive_active_context(&store).bound_objectives.is_empty());
}

#[test]
fn superseded_node_is_not_a_head() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let v1 = human_objective(&mut store, "V1");
    let v2 = human_objective(&mut store, "V2");
    store.append_edge(v2, v1, EdgeKind::Supersedes).unwrap();
    let ctx = derive_active_context(&store);
    assert_eq!(ctx.bound_objectives.len(), 1);
    assert_eq!(ctx.bound_objectives[0].id, v2);
}

#[test]
fn binding_inherits_transitively_decomposition_milestone_objective() {
    // 传递链：bound Decomposition → member Milestone（继承）→ groups Objective（再继承）。
    let mut store = GraphStore::open_in_memory().unwrap();
    let parent = human_objective(&mut store, "parent");
    let d = store
        .as_llm()
        .decomposition(
            "d",
            DecompositionPayload {
                rationale: "split".into(),
                proposed_count: 1,
            },
        )
        .unwrap();
    store.append_edge(parent, d, EdgeKind::Decomposes).unwrap();
    let acc = acceptance(&mut store);
    store.append_edge(acc, d, EdgeKind::Accepts).unwrap();
    let m = milestone(&mut store, false);
    store.append_edge(m, d, EdgeKind::MemberOf).unwrap();
    let o2 = llm_objective(&mut store, "O2");
    store.append_edge(m, o2, EdgeKind::Groups).unwrap();

    let ctx = derive_active_context(&store);
    assert!(
        ctx.bound_objectives.iter().any(|v| v.id == o2),
        "传递继承应使 O2 bound：{:?}",
        ctx.bound_objectives
    );
}

#[test]
fn binding_inherits_through_milestone_groups() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let ms = milestone(&mut store, true);
    let o = llm_objective(&mut store, "child");
    store.append_edge(ms, o, EdgeKind::Groups).unwrap();
    let ctx = derive_active_context(&store);
    assert!(ctx.bound_objectives.iter().any(|v| v.id == o));
}

#[test]
fn active_milestone_requires_activation_event() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let ms = milestone(&mut store, true);
    assert!(derive_active_context(&store).active_milestone.is_none());

    let act = store
        .as_human()
        .activation_event(
            "activate",
            ActivationPayload {
                reason: String::new(),
            },
        )
        .unwrap();
    store.append_edge(act, ms, EdgeKind::Activates).unwrap();
    let ctx = derive_active_context(&store);
    assert_eq!(ctx.active_milestone.map(|m| m.id), Some(ms));
}

#[test]
fn bound_constraints_via_objective_constrained_by() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let o = human_objective(&mut store, "Goal");
    let c = store
        .as_human()
        .constraint(
            "c",
            ConstraintPayload {
                kind: ConstraintKind::Forbidden,
                statement: "no network".into(),
                verifier: None,
            },
        )
        .unwrap();
    store.append_edge(o, c, EdgeKind::ConstrainedBy).unwrap();
    let ctx = derive_active_context(&store);
    assert_eq!(ctx.bound_constraints.len(), 1);
    assert_eq!(ctx.bound_constraints[0].id, c);
}

#[test]
fn open_change_request_listed_until_accepted() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let cr = store
        .as_human()
        .change_request(
            "cr",
            ChangeRequestPayload {
                kind: ChangeRequestKind::NewRequirement,
                request: "add feature".into(),
                priority: ChangePriority::Should,
            },
        )
        .unwrap();
    assert_eq!(derive_active_context(&store).open_change_requests.len(), 1);

    let acc = acceptance(&mut store);
    store.append_edge(acc, cr, EdgeKind::Accepts).unwrap();
    assert!(derive_active_context(&store)
        .open_change_requests
        .is_empty());
}

#[test]
fn outstanding_question_until_answered() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let q = store.as_llm().question("q", "which db?").unwrap();
    assert_eq!(derive_active_context(&store).outstanding_questions.len(), 1);

    let a = store.as_human().answer("a", "sqlite").unwrap();
    store.append_edge(a, q, EdgeKind::Answers).unwrap();
    assert!(derive_active_context(&store)
        .outstanding_questions
        .is_empty());
}

#[test]
fn digest_is_deterministic_and_valid_hex() {
    let mut store = GraphStore::open_in_memory().unwrap();
    human_objective(&mut store, "Goal");
    let ctx1 = derive_active_context(&store);
    let ctx2 = derive_active_context(&store);
    assert_eq!(ctx1.digest, ctx2.digest);
    assert_eq!(ctx1.digest.len(), 64);
    assert!(ctx1
        .digest
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
}

#[test]
fn snapshot_payload_passes_store_validation() {
    let mut store = GraphStore::open_in_memory().unwrap();
    human_objective(&mut store, "Goal");
    let ctx = derive_active_context(&store);
    let payload = snapshot_payload(&ctx);
    let id = store
        .as_deterministic()
        .context_snapshot("snapshot", payload);
    assert!(id.is_ok(), "snapshot payload 应通过 digest 校验：{id:?}");
}
