//! 容忍后端请求失败的 LLM client 包装（与 e2e harness 同构，刻意不共享代码）。
//!
//! 真实公网端点（如 deepseek-flash）偶发连接重置（"error sending request"），属瞬时抖动而非
//! 真实不可用。`complete_structured` 库层**故意不**重试 `BackendUnavailable`（避免放大不可用），
//! 故在 example 这一层做有界重试是合理的——不改库语义，只是让基准不被偶发抖动误判。
//!
//! 公平性：`sophia` 与 `baseline` 两 mode 共用同一个被包装的 client，重试策略一致，不偏袒任一方。

use sophia_llm::{CompletionRequest, CompletionResponse, LlmClient, LlmError, LlmResult};

struct RetryClient<C: LlmClient> {
    inner: C,
    max_attempts: u32,
}

#[async_trait::async_trait]
impl<C: LlmClient> LlmClient for RetryClient<C> {
    async fn complete(&self, req: &CompletionRequest) -> LlmResult<CompletionResponse> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match self.inner.complete(req).await {
                Ok(resp) => return Ok(resp),
                // 仅瞬时网络层不可用才重试；其它错误（解析/schema 等）立即上报。
                Err(LlmError::BackendUnavailable(msg)) if attempt < self.max_attempts => {
                    let backoff = std::time::Duration::from_millis(800 * attempt as u64);
                    eprintln!(
                        "    [retry] 第 {attempt} 次 LLM 请求失败（{msg}），{} ms 后重试…",
                        backoff.as_millis()
                    );
                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// 用有界重试包装一个后端 client。
pub fn with_retry<C: LlmClient>(inner: C, max_attempts: u32) -> impl LlmClient {
    RetryClient {
        inner,
        max_attempts: max_attempts.max(1),
    }
}
