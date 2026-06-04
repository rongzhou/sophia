//! 工作流总调度器测试：decision 驱动 design → implement-loop 的 goal 推进。
//!
//! prompt 由 `StaticPrompts`（common）在调用时刻渲染——调度器不再收预渲染静态请求
//! （见 engineering_architecture §8.4：prompt 必须由调用时刻 active context 渲染）。

mod common;

use common::{library_policy, seed_objective, MockClient, StaticPrompts};
use serde_json::json;
use sophia_engine::{run_goal_loop, ImplementLoopConfig, Outcome, SchedulerBudget, SchedulerError};
use sophia_graph_db::{
    DecisionAction, DiagnosticKind, DiagnosticPayload, EdgeKind, GraphStore, NodeRole,
};
use sophia_llm::LlmError;

// ---- decision JSON 构造 ----

fn decide(action: &str) -> String {
    json!({
        "selected_action": action,
        "confidence": 0.9,
        "rationale": "test",
        "state_assessment": {
            "kind": "goal",
            "goal_size": "small",
            "decomposition_pressure": "low",
            "active_milestone_present": false,
            "outstanding_clarifications": 0
        }
    })
    .to_string()
}

fn design_out() -> String {
    json!({ "purpose": "complete", "pseudocode": "# Purpose\n..." }).to_string()
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

fn fail_check() -> DiagnosticPayload {
    use sophia_graph_db::{DiagnosticItem, DiagnosticSeverity};
    DiagnosticPayload {
        kind: DiagnosticKind::CodeCheck,
        ok: false,
        diagnostics: vec![DiagnosticItem {
            code: "CHECK-TYPE-001".into(),
            severity: DiagnosticSeverity::Error,
            problem: "概念性类型不匹配".into(),
            location: Some("D/A.sophia:1".into()),
        }],
    }
}

#[tokio::test]
async fn design_then_implement_yields_candidate() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    // 轮1 decision=design_solution → design 产出；轮2 decision=implement_design → implement-loop 通过。
    let client = MockClient::new(vec![
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()),
    ]);

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &SchedulerBudget::default(),
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::CandidateReady {
            code,
            files,
            decisions,
        } => {
            assert_eq!(decisions, 2);
            assert_eq!(store.role_of(code), Some(NodeRole::Code));
            assert_eq!(files.len(), 1);
            // DecisionNode considers→ obj 存在。
            assert!(store
                .edges()
                .iter()
                .any(|e| e.kind == EdgeKind::Considers && e.to == obj));
        }
        other => panic!("应产出候选，实际 {other:?}"),
    }
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn implement_without_pseudocode_yields() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    // 第一轮就 implement_design，但还没 design 过 → Yielded。
    let client = MockClient::new(vec![Ok(decide("implement_design"))]);

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &SchedulerBudget::default(),
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::Yielded {
            action, decisions, ..
        } => {
            assert_eq!(action, DecisionAction::ImplementDesign);
            assert_eq!(decisions, 1);
        }
        other => panic!("无伪代码应 Yielded，实际 {other:?}"),
    }
}

#[tokio::test]
async fn higher_level_action_yields() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    let client = MockClient::new(vec![Ok(decide("decompose"))]);

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &SchedulerBudget::default(),
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::Yielded {
            action, decision, ..
        } => {
            assert_eq!(action, DecisionAction::Decompose);
            assert_eq!(store.role_of(decision), Some(NodeRole::Decision));
        }
        other => panic!("decompose 应 Yielded，实际 {other:?}"),
    }
}

#[tokio::test]
async fn revise_design_is_reachable_after_implement_exhausted() {
    // implement 在预算内未过 → 不结束 goal 循环，回到 decision；LLM 选 revise_design 重写
    // 伪代码（revises→ 旧），再 implement 通过 → CandidateReady。验证 revise 可达（design 10.8）。
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    let client = MockClient::new(vec![
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("implement_design")),
        Ok(impl_out()), // 首次实现：check 将失败
        Ok(decide("revise_design")),
        Ok(design_out()), // 修订后的新伪代码
        Ok(decide("implement_design")),
        Ok(impl_out()), // 二次实现：check 将通过
    ]);

    // implement-loop 不修复（max_repair_attempts=0）：第一次 check 失败即 BudgetExhausted。
    let budget = SchedulerBudget {
        implement_loop: ImplementLoopConfig {
            max_repair_attempts: 0,
            structured: Default::default(),
        },
        ..SchedulerBudget::default()
    };

    // 第一次 implement 的 check 失败，之后通过（按 check 调用次数切换）。
    let mut checks = 0u32;
    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &budget,
        &library_policy(),
        obj,
        |_f: &[(String, String)]| {
            checks += 1;
            if checks == 1 {
                fail_check()
            } else {
                ok_check()
            }
        },
    )
    .await
    .unwrap();

    match outcome {
        Outcome::CandidateReady { decisions, .. } => {
            // design + implement(失败) + revise + implement(成功) = 4 轮决策。
            assert_eq!(decisions, 4);
            // 应有一条 revises→ 边（修订伪代码连旧伪代码）。
            assert!(
                store.edges().iter().any(|e| e.kind == EdgeKind::Revises),
                "应有 revises→ 边"
            );
        }
        other => panic!("revise 后应产出候选，实际 {other:?}"),
    }
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn needs_clarification_emits_question_and_yields() {
    // needs_clarification：emit 一个 Clarification(Question) asks_about→ 焦点，再让位。
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    let client = MockClient::new(vec![Ok(decide("needs_clarification"))]);

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &SchedulerBudget::default(),
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::Yielded { action, .. } => {
            assert_eq!(action, DecisionAction::NeedsClarification);
            // 应真正建了一个 Clarification 节点，并连 asks_about→ obj。
            let q = store
                .nodes()
                .find(|n| n.meta.role == NodeRole::Clarification)
                .expect("应 emit Clarification 节点");
            assert!(store.has_edge(q.meta.id, obj, EdgeKind::AsksAbout));
        }
        other => panic!("needs_clarification 应 Yielded，实际 {other:?}"),
    }
}

#[tokio::test]
async fn decision_budget_exhausted() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    // 持续 design_solution，但限制 max_decisions=2，使 decision 轮数先达上限。
    let client = MockClient::new(vec![
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("design_solution")),
        Ok(design_out()),
    ]);

    let budget = SchedulerBudget {
        max_decisions: 2,
        max_pseudocode_versions: 10,
        max_total_llm_nodes: 100,
        implement_loop: ImplementLoopConfig::default(),
    };

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &budget,
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::BudgetExhausted { decisions, .. } => assert_eq!(decisions, 2),
        other => panic!("应预算耗尽，实际 {other:?}"),
    }
}

#[tokio::test]
async fn pseudocode_version_budget_exhausted() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    // max_pseudocode_versions=1：第二次 design_solution 触发预算。
    let client = MockClient::new(vec![
        Ok(decide("design_solution")),
        Ok(design_out()),
        Ok(decide("design_solution")),
    ]);

    let budget = SchedulerBudget {
        max_decisions: 10,
        max_pseudocode_versions: 1,
        max_total_llm_nodes: 100,
        implement_loop: ImplementLoopConfig::default(),
    };

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &budget,
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    assert!(matches!(outcome, Outcome::BudgetExhausted { .. }));
}

#[tokio::test]
async fn llm_node_budget_ignores_history_before_run() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let _history = store.as_llm().question("old question", "history").unwrap();
    let obj = seed_objective(&mut store);

    let client = MockClient::new(vec![Ok(decide("decompose"))]);
    let budget = SchedulerBudget {
        max_decisions: 10,
        max_pseudocode_versions: 10,
        max_total_llm_nodes: 1,
        implement_loop: ImplementLoopConfig::default(),
    };

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &budget,
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::Yielded {
            action, decisions, ..
        } => {
            assert_eq!(action, DecisionAction::Decompose);
            assert_eq!(decisions, 1);
        }
        other => panic!("历史 LLM 节点不应耗尽本轮预算，实际 {other:?}"),
    }
}

#[tokio::test]
async fn decision_backend_failure_emits_raw_llm() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    let client = MockClient::new(vec![Err(LlmError::BackendUnavailable("down".into()))]);

    let outcome = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &SchedulerBudget::default(),
        &library_policy(),
        obj,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap();

    match outcome {
        Outcome::Failed { raw_llm, error } => {
            assert!(matches!(error, LlmError::BackendUnavailable(_)));
            assert!(store.has_edge(raw_llm, obj, EdgeKind::Attempted));
        }
        other => panic!("后端不可用应 Failed，实际 {other:?}"),
    }
}

#[tokio::test]
async fn rejects_invalid_focus() {
    let mut store = GraphStore::open_in_memory().unwrap();
    // 用 Constraint 当焦点（非 Objective/Milestone）。
    let c = store
        .as_human()
        .constraint(
            "c",
            sophia_graph_db::ConstraintPayload {
                kind: sophia_graph_db::ConstraintKind::Invariant,
                statement: "keep".into(),
                verifier: None,
            },
        )
        .unwrap();
    let client = MockClient::new(vec![]);

    let err = run_goal_loop(
        &mut store,
        &client,
        &StaticPrompts,
        &SchedulerBudget::default(),
        &library_policy(),
        c,
        |_f: &[(String, String)]| ok_check(),
    )
    .await
    .unwrap_err();

    assert!(matches!(err, SchedulerError::InvalidFocus(_)));
}
