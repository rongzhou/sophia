//! LLM step 编排测试：snapshot → 调用 → 成功 / RawLlmNode 兜底。

mod common;

use common::{seed_objective, MockClient};
use serde::Deserialize;
use serde_json::{json, Value};
use sophia_engine::{run_llm_step, LlmStepOutcome};
use sophia_graph_db::{EdgeKind, GraphStore, NodeRole, Provenance};
use sophia_llm::{CompletionRequest, LlmError, StructuredConfig};

#[derive(Debug, Deserialize)]
struct Decision {
    action: String,
}

/// 一个最小的判别 schema（仅本测试用，无 prompt crate 对应物）。
fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["action"],
        "properties": { "action": { "type": "string" } }
    })
}

#[tokio::test]
async fn success_builds_snapshot_before_call() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let tgt = seed_objective(&mut store);
    let client = MockClient::new(vec![Ok(r#"{"action":"design_solution"}"#.into())]);
    let req = CompletionRequest::new("m", "decide");

    let outcome: LlmStepOutcome<Decision> = run_llm_step(
        &mut store,
        &client,
        |_ctx| req.clone(),
        &schema(),
        &StructuredConfig::default(),
        tgt,
        "decision",
    )
    .await
    .unwrap();

    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            assert_eq!(value.action, "design_solution");
            // snapshot 是 deterministic ContextSnapshot 节点。
            assert_eq!(store.role_of(snapshot), Some(NodeRole::ContextSnapshot));
            assert_eq!(
                store.provenance_of(snapshot),
                Some(Provenance::Deterministic)
            );
        }
        LlmStepOutcome::Failed { .. } => panic!("应成功"),
    }
}

#[tokio::test]
async fn backend_unavailable_emits_raw_llm_with_attempted_edge() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let tgt = seed_objective(&mut store);
    let client = MockClient::new(vec![Err(LlmError::BackendUnavailable("down".into()))]);
    let req = CompletionRequest::new("m", "decide");

    let outcome: LlmStepOutcome<Decision> = run_llm_step(
        &mut store,
        &client,
        |_ctx| req.clone(),
        &schema(),
        &StructuredConfig::default(),
        tgt,
        "decision",
    )
    .await
    .unwrap();

    match outcome {
        LlmStepOutcome::Failed { raw_llm, error } => {
            assert!(matches!(error, LlmError::BackendUnavailable(_)));
            // RawLlmNode 创建且 creation_status=Failed。
            assert_eq!(store.role_of(raw_llm), Some(NodeRole::RawLlm));
            // attempted→ target 边存在。
            assert!(store.has_edge(raw_llm, tgt, EdgeKind::Attempted));
            // 失败调用也连回本次调用前创建的 ContextSnapshot，便于审计复现。
            let snapshot = store
                .edges()
                .iter()
                .find(|e| e.from == raw_llm && e.kind == EdgeKind::Consumed)
                .map(|e| e.to)
                .expect("RawLlm 应有 consumed→ ContextSnapshot");
            assert_eq!(store.role_of(snapshot), Some(NodeRole::ContextSnapshot));
        }
        LlmStepOutcome::Succeeded { .. } => panic!("后端不可用应失败"),
    }
}

#[tokio::test]
async fn schema_validation_failure_emits_raw_llm() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let tgt = seed_objective(&mut store);
    // 始终返回缺字段的 JSON → 超重试后 SchemaValidation。
    let client = MockClient::new(vec![Ok("{}".into()), Ok("{}".into()), Ok("{}".into())]);
    let req = CompletionRequest::new("m", "decide");
    let cfg = StructuredConfig { max_retries: 2 };

    let outcome: LlmStepOutcome<Decision> = run_llm_step(
        &mut store,
        &client,
        |_ctx| req.clone(),
        &schema(),
        &cfg,
        tgt,
        "decision",
    )
    .await
    .unwrap();

    match outcome {
        LlmStepOutcome::Failed { raw_llm, error } => {
            assert!(matches!(error, LlmError::SchemaValidation { .. }));
            let node = store.node(raw_llm).unwrap();
            // failure_kind 映射为 ValidationError。
            match &node.payload {
                sophia_graph_db::NodePayload::RawLlm(p) => {
                    assert_eq!(
                        p.failure_kind,
                        sophia_graph_db::RawLlmFailureKind::ValidationError
                    );
                }
                _ => panic!("应为 RawLlm payload"),
            }
        }
        LlmStepOutcome::Succeeded { .. } => panic!("schema 验证失败应兜底"),
    }
}

#[tokio::test]
async fn snapshot_created_even_on_failure() {
    // 失败路径也应先建 snapshot（在调用前），保证可审计。
    let mut store = GraphStore::open_in_memory().unwrap();
    let tgt = seed_objective(&mut store);
    let client = MockClient::new(vec![Err(LlmError::BackendUnavailable("x".into()))]);
    let req = CompletionRequest::new("m", "decide");

    let _: LlmStepOutcome<Decision> = run_llm_step(
        &mut store,
        &client,
        |_ctx| req.clone(),
        &schema(),
        &StructuredConfig::default(),
        tgt,
        "decision",
    )
    .await
    .unwrap();

    // 图中应存在一个 ContextSnapshot 节点。
    let has_snapshot = store
        .nodes()
        .any(|n| n.meta.role == NodeRole::ContextSnapshot);
    assert!(has_snapshot, "失败路径也应已建 snapshot");
}
