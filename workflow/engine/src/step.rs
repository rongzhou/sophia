//! 单次 LLM step 的编排。

use serde::de::DeserializeOwned;
use serde_json::Value;
use sophia_graph_db::{
    derive_active_context, snapshot_payload, ActiveContext, GraphStore, NodeId, RawLlmFailureKind,
    RawLlmPayload,
};
use sophia_llm::{complete_structured, CompletionRequest, LlmClient, LlmError, StructuredConfig};
use thiserror::Error;

/// LLM step 编排错误（区别于「LLM 调用本身失败」——后者会 emit RawLlmNode 并以
/// [`LlmStepOutcome::Failed`] 返回，而非 `Err`）。
#[derive(Debug, Error)]
pub enum LlmStepError {
    /// 写图失败（建 snapshot / RawLlmNode / 边）。
    #[error("写图失败：{0}")]
    Graph(#[from] sophia_graph_db::GraphError),
}

/// 一次 LLM step 的结果。
pub enum LlmStepOutcome<T> {
    /// 成功：返回校验后的结构化输出与本次调用所建的 ContextSnapshot 节点。
    /// 调用方据 `T` 创建对应的 LLM-provenance 节点，并连 `consumed→ snapshot`。
    Succeeded { value: T, snapshot: NodeId },
    /// 失败：已 emit `RawLlmNode`（attempted→ target），返回其 NodeId 与错误。
    Failed { raw_llm: NodeId, error: LlmError },
}

/// 执行一次结构化 LLM step（snapshot → 调用 → 成功值 / RawLlmNode 兜底）。
///
/// 流程：
/// 1. 由图当前状态推导 active context，建 `ContextSnapshot` 节点（确定性，I10）；
/// 2. **用同一份 active context 渲染请求**（`render`）——保证 prompt 与 snapshot 同源
///    （§10.7：snapshot 必须 100% 复现 LLM 当时所见，见 engineering_architecture §8.4）；
/// 3. 调用 `complete_structured`（重试 + schema 验证）；
/// 4. 成功 → 返回值 + snapshot（调用方建下游节点并连 `consumed→` 边）；
///    失败 → emit `RawLlmNode` 并连 `attempted→ target`，返回 [`LlmStepOutcome::Failed`]。
///
/// `render` 是调用时刻的 prompt 渲染器：接收本次 snapshot 同源的 active context，产出
/// `CompletionRequest`。`target` 是本次意图执行的目标节点；`operation` 是操作名
/// （写入 RawLlmNode）。
pub async fn run_llm_step<T, C, R>(
    store: &mut GraphStore,
    client: &C,
    render: R,
    schema: &Value,
    config: &StructuredConfig,
    target: NodeId,
    operation: &str,
) -> Result<LlmStepOutcome<T>, LlmStepError>
where
    T: DeserializeOwned,
    C: LlmClient + ?Sized,
    R: FnOnce(&ActiveContext) -> CompletionRequest,
{
    // 步骤 1：确定性推导 active context，建 ContextSnapshot（任何 LLM 调用前必做，I6 接入点）。
    let ctx = derive_active_context(store);
    let snapshot = store
        .as_deterministic()
        .context_snapshot(format!("snapshot:{operation}"), snapshot_payload(&ctx))?;

    // 步骤 2：用**同一份** ctx 渲染请求（prompt 与 snapshot 同源）。
    let req = render(&ctx);

    // 步骤 3：结构化调用。
    match complete_structured::<T, C>(client, &req, schema, config).await {
        Ok(value) => Ok(LlmStepOutcome::Succeeded { value, snapshot }),
        Err(error) => {
            // 步骤 4（失败）：emit RawLlmNode + attempted→ target。
            let raw_llm = store.as_llm().raw_llm(
                format!("raw_llm:{operation}"),
                RawLlmPayload {
                    failure_kind: failure_kind_of(&error),
                    operation: operation.to_string(),
                    error_summary: error.to_string(),
                },
            )?;
            store.append_edge(raw_llm, target, sophia_graph_db::EdgeKind::Attempted)?;
            store.append_edge(raw_llm, snapshot, sophia_graph_db::EdgeKind::Consumed)?;
            Ok(LlmStepOutcome::Failed { raw_llm, error })
        }
    }
}

/// 把 `LlmError` 映射为 `RawLlmFailureKind`（4.4.8）。
fn failure_kind_of(e: &LlmError) -> RawLlmFailureKind {
    match e {
        LlmError::BackendUnavailable(_) => RawLlmFailureKind::ExecutionError,
        LlmError::Parse(_) => RawLlmFailureKind::ParseError,
        LlmError::SchemaValidation { .. } => RawLlmFailureKind::ValidationError,
        LlmError::SelfCheck(_) => RawLlmFailureKind::SelfCheckFailure,
    }
}

/// 取某工作流步骤的内置严格 schema（解析后的 JSON）。
///
/// schema 是每个步骤固定的结构契约（design→`design_result`、implement/repair→
/// `implement_result`/`repair_result`、decision→`decision`），由 `prompt` crate 内置并经
/// snapshot 守护，故按名取用并 `expect`（缺失即内部构建错误）。
pub(crate) fn step_schema(name: &str) -> Value {
    let src = sophia_prompt::schema_for(name)
        .unwrap_or_else(|| panic!("prompt crate 缺内置 schema `{name}`"));
    serde_json::from_str(src).unwrap_or_else(|e| panic!("内置 schema `{name}` 非法 JSON：{e}"))
}
