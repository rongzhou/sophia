//! 目标树遍历层测试：在线性 spine 之上驱动 decompose 子树 / backtrack 分支。
//!
//! 见 docs/language_design.md 10.8（decompose / backtrack）、10.9（不塞进 spine）。
//! prompt 由 `StaticPrompts`（common）在调用时刻渲染；LLM 响应由 MockClient 队列驱动。

mod common;

use common::{library_policy, seed_objective, MockClient, StaticPrompts};
use serde_json::json;
use sophia_engine::{
    run_goal_tree, AutoAcceptReviewer, GoalResolution, GoalTreeConfig, TreeBudget,
};
use sophia_graph_db::{DiagnosticKind, DiagnosticPayload, EdgeKind, GraphStore, NodeRole};

// ---- LLM 响应构造 ----

fn decide(action: &str) -> String {
    json!({
        "selected_action": action,
        "confidence": 0.9,
        "rationale": "test",
        "state_assessment": {
            "kind": "goal",
            "goal_size": "large",
            "decomposition_pressure": "high",
            "active_milestone_present": false,
            "outstanding_clarifications": 0
        }
    })
    .to_string()
}

fn tree_config() -> GoalTreeConfig {
    GoalTreeConfig {
        budget: TreeBudget::default(),
        library_policy: library_policy(),
    }
}

fn decompose_out(n: usize) -> String {
    let children: Vec<_> = (0..n)
        .map(
            |i| json!({ "title": format!("子目标{i}"), "description": format!("实现第 {i} 部分") }),
        )
        .collect();
    json!({ "rationale": "目标过大，按用例拆分", "children": children }).to_string()
}

fn design_out() -> String {
    json!({
        "purpose": "complete",
        "pseudocode": "<!-- sophia-pseudo: v1 -->\n# Purpose\nDescribe the intended behavior.\n# Inputs\nList the inputs.\n# Outputs\nList the outputs.\n# Algorithm\nDescribe the steps without source code.\n# Constraints\nState relevant limits.\n# Forbidden\nState what must not be used."
    })
    .to_string()
}

fn impl_out() -> String {
    json!({ "files": [{ "path": "D/A.sophia", "content": "entity A {}" }] }).to_string()
}

fn ok_check() -> DiagnosticPayload {
    DiagnosticPayload {
        kind: DiagnosticKind::CodeCheck,
        ok: true,
        diagnostics: vec![],
    }
}

#[tokio::test]
async fn decompose_then_resolve_each_child_to_candidate() {
    // 根目标 decompose 成 2 个子目标；每个子目标各自 design→implement 推进到候选。
    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    let client = MockClient::new(vec![
        // 根目标：decision=decompose → spine 让位 → 遍历层执行 decompose。
        Ok(decide("decompose")),
        Ok(decompose_out(2)),
        // 子目标 0：design → implement → 候选。
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()),
        // 子目标 1：design → implement → 候选。
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()),
    ]);

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut AutoAcceptReviewer,
        &tree_config(),
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    // 根归结为 Decomposed，含 2 个子归结，均为候选。
    match &resolution {
        GoalResolution::Decomposed {
            focus,
            decomposition,
            children,
        } => {
            assert_eq!(*focus, root);
            assert_eq!(store.role_of(*decomposition), Some(NodeRole::Decomposition));
            assert_eq!(children.len(), 2);
            assert!(children
                .iter()
                .all(|c| matches!(c, GoalResolution::Candidate { .. })));
        }
        other => panic!("根应 Decomposed，实际 {other:?}"),
    }

    // 整棵树完全归结；收集到 2 个候选。
    assert!(resolution.is_fully_resolved());
    assert_eq!(resolution.candidates().len(), 2);

    // 图中：parent decomposes→ Decomposition，子目标 member_of→ Decomposition。
    assert!(store
        .edges()
        .iter()
        .any(|e| e.kind == EdgeKind::Decomposes && e.from == root));
    assert_eq!(
        store
            .edges()
            .iter()
            .filter(|e| e.kind == EdgeKind::MemberOf)
            .count(),
        2
    );
    // Decomposition 作为执行产物节点 consumed→ 其 ContextSnapshot（I6 锚点）。
    if let GoalResolution::Decomposed { decomposition, .. } = &resolution {
        assert!(store.edges().iter().any(|e| {
            e.kind == EdgeKind::Consumed
                && e.from == *decomposition
                && store.role_of(e.to) == Some(NodeRole::ContextSnapshot)
        }));
    }
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn leaf_goal_without_decompose_resolves_directly() {
    // 根目标直接 design→implement（不 decompose）→ 单个候选，不建 Decomposition。
    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    let client = MockClient::new(vec![
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()),
    ]);

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut AutoAcceptReviewer,
        &tree_config(),
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match &resolution {
        GoalResolution::Candidate { focus, code, .. } => {
            assert_eq!(*focus, root);
            assert_eq!(store.role_of(*code), Some(NodeRole::Code));
        }
        other => panic!("应直接产出候选，实际 {other:?}"),
    }
    assert!(!store.edges().iter().any(|e| e.kind == EdgeKind::Decomposes));
}

#[tokio::test]
async fn backtrack_abandons_branch_without_faking_withdrawal() {
    // 根目标 decision=backtrack → 遍历层记 Backtracked；不伪造 WithdrawalEvent（撤销是人类权威）。
    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    let client = MockClient::new(vec![Ok(decide("backtrack"))]);

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut AutoAcceptReviewer,
        &tree_config(),
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match &resolution {
        GoalResolution::Backtracked { focus } => assert_eq!(*focus, root),
        other => panic!("应 Backtracked，实际 {other:?}"),
    }
    assert!(!resolution.is_fully_resolved());
    // 没有 WithdrawalEvent 节点（未伪造撤销）。
    assert!(!store
        .nodes()
        .any(|n| n.meta.role == NodeRole::WithdrawalEvent));
}

#[tokio::test]
async fn nested_decompose_stops_at_max_depth() {
    // max_depth=1：根 decompose（深度0→子在深度1），子再 decompose 时达深度上限 → BudgetExhausted。
    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    let budget = TreeBudget {
        max_depth: 1,
        max_goals: 16,
        scheduler: Default::default(),
    };

    let client = MockClient::new(vec![
        // 根（深度0）decompose 成 2 子。
        Ok(decide("decompose")),
        Ok(decompose_out(2)),
        // 子目标0（深度1）：再 decompose → 达深度上限，不再展开。
        Ok(decide("decompose")),
        // 子目标1（深度1）：再 decompose → 同样达上限。
        Ok(decide("decompose")),
    ]);
    let config = GoalTreeConfig {
        budget,
        library_policy: library_policy(),
    };

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut AutoAcceptReviewer,
        &config,
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match &resolution {
        GoalResolution::Decomposed { children, .. } => {
            assert_eq!(children.len(), 2);
            // 两个子目标都因深度上限 BudgetExhausted。
            for c in children {
                assert!(
                    matches!(c, GoalResolution::BudgetExhausted { .. }),
                    "子目标应因深度上限耗尽，实际 {c:?}"
                );
            }
        }
        other => panic!("根应 Decomposed，实际 {other:?}"),
    }
    // 只建了一层 Decomposition（根那次）。
    assert_eq!(
        store
            .nodes()
            .filter(|n| n.meta.role == NodeRole::Decomposition)
            .count(),
        1
    );
}

#[tokio::test]
async fn max_goals_budget_stops_traversal() {
    // max_goals=1：根目标用掉唯一名额后，子目标不再推进 → BudgetExhausted。
    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    let budget = TreeBudget {
        max_depth: 3,
        max_goals: 1,
        scheduler: Default::default(),
    };

    let client = MockClient::new(vec![Ok(decide("decompose")), Ok(decompose_out(2))]);
    let config = GoalTreeConfig {
        budget,
        library_policy: library_policy(),
    };

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut AutoAcceptReviewer,
        &config,
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match &resolution {
        GoalResolution::Decomposed { children, .. } => {
            // 两个子目标都因目标总数上限未能推进。
            for c in children {
                assert!(
                    matches!(c, GoalResolution::BudgetExhausted { .. }),
                    "子目标应因目标总数上限耗尽，实际 {c:?}"
                );
            }
        }
        other => panic!("根应 Decomposed，实际 {other:?}"),
    }
}

// ---- 人类授权检查点（design 5.3 / N4）----

/// 总是拒绝拆解的审查者（测试 reject 路径）。
struct RejectReviewer;

impl sophia_engine::DecompositionReviewer for RejectReviewer {
    fn review(
        &mut self,
        _store: &GraphStore,
        _parent: sophia_graph_db::NodeId,
        _decomposition: sophia_graph_db::NodeId,
        _children: &[sophia_graph_db::NodeId],
    ) -> sophia_engine::ReviewDecision {
        sophia_engine::ReviewDecision::Reject {
            reason: "人类不接受此拆解".into(),
        }
    }
}

#[tokio::test]
async fn rejected_decomposition_does_not_recurse_or_fake_withdrawal() {
    // 根目标 decompose，但审查者拒绝：不递归子目标、不伪造 WithdrawalEvent、不建 AcceptanceEvent。
    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    // 只需根的 decision=decompose + 拆解结构两次响应；拒绝后不再有子目标调用。
    let client = MockClient::new(vec![Ok(decide("decompose")), Ok(decompose_out(2))]);

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut RejectReviewer,
        &tree_config(),
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match &resolution {
        GoalResolution::DecompositionRejected {
            focus,
            decomposition,
            ..
        } => {
            assert_eq!(*focus, root);
            assert_eq!(store.role_of(*decomposition), Some(NodeRole::Decomposition));
        }
        other => panic!("应 DecompositionRejected，实际 {other:?}"),
    }
    assert!(!resolution.is_fully_resolved());
    // 拒绝不建 AcceptanceEvent，也不伪造 WithdrawalEvent（N4：撤销 / 接受是人类权威）。
    assert!(!store
        .nodes()
        .any(|n| n.meta.role == NodeRole::AcceptanceEvent));
    assert!(!store
        .nodes()
        .any(|n| n.meta.role == NodeRole::WithdrawalEvent));
    // Decomposition 节点本身已 append 落图（append-only），但未获 binding。
    assert_eq!(
        store
            .nodes()
            .filter(|n| n.meta.role == NodeRole::Decomposition)
            .count(),
        1
    );
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn accepted_decomposition_binds_children_into_active_context() {
    // 接受拆解后：建 human AcceptanceEvent accepts→ Decomposition，子目标沿 member_of 继承
    // binding，从而进入 active context（design 5.3）。这正是子目标 design/implement 能看到
    // 自己目标的前提。
    use sophia_graph_db::derive_active_context;

    let mut store = GraphStore::open_in_memory().unwrap();
    let root = seed_objective(&mut store);

    let client = MockClient::new(vec![
        Ok(decide("decompose")),
        Ok(decompose_out(2)),
        // 子目标 0、1 各 design→implement。
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()),
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()),
    ]);

    let resolution = run_goal_tree(
        &mut store,
        &client,
        &StaticPrompts,
        &mut AutoAcceptReviewer,
        &tree_config(),
        root,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    assert!(resolution.is_fully_resolved());

    // 接受落图：恰有一个 human AcceptanceEvent accepts→ Decomposition。
    let accept_count = store
        .nodes()
        .filter(|n| n.meta.role == NodeRole::AcceptanceEvent)
        .count();
    assert_eq!(accept_count, 1, "接受应恰建一个 AcceptanceEvent");
    assert!(store.edges().iter().any(|e| {
        e.kind == EdgeKind::Accepts && store.role_of(e.to) == Some(NodeRole::Decomposition)
    }));

    // binding 继承到子目标：active context 的 bound_objectives 含两个 LLM 派生子目标
    // （root 是 human 隐式 bound，子目标经 member_of 继承 → 共 3 个 Objective）。
    let ctx = derive_active_context(&store);
    let bound_titles: Vec<&str> = ctx
        .bound_objectives
        .iter()
        .map(|o| o.title.as_str())
        .collect();
    assert!(
        bound_titles.contains(&"子目标0") && bound_titles.contains(&"子目标1"),
        "子目标应经 member_of 继承 binding 进入 active context，实际 bound: {bound_titles:?}"
    );
}
