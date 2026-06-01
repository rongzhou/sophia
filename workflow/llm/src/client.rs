//! LLM client trait 与请求 / 响应类型。
//!
//! 见 docs/engineering_architecture.md 第七节。`LlmClient` 是模型无关抽象；
//! `complete` 是底层自由文本补全，`complete_structured`（见 [`crate::structured`]）
//! 在其上叠加 schema 验证与重试 fallback。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// LLM 层结果别名。
pub type LlmResult<T> = Result<T, LlmError>;

/// LLM 层错误。结构化失败不伪造成功，必须由上层 emit `RawLlmNode`。
///
/// 变体与 `RawLlmFailureKind`（workflow_graph_spec 4.4.8）对齐：
/// `BackendUnavailable`→ExecutionError、`Parse`→ParseError、
/// `SchemaValidation`→ValidationError、`SelfCheck`→SelfCheckFailure。
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// 后端不可用（远端 API 或本地服务）。
    #[error("LLM 后端不可用：{0}")]
    BackendUnavailable(String),

    /// 响应不是合法 JSON / 无法解析。
    #[error("LLM 响应解析失败：{0}")]
    Parse(String),

    /// schema 验证失败且超过重试次数。
    #[error("结构化输出 schema 验证失败（已重试 {attempts} 次）：{last_errors}")]
    SchemaValidation { attempts: u32, last_errors: String },

    /// 输出 self-check 不通过（如 AssessmentSelfCheck）。
    #[error("LLM 输出 self-check 失败：{0}")]
    SelfCheck(String),
}

/// LLM 补全请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionRequest {
    /// 模型名（如 `qwen3`、远端模型 id）。
    pub model: String,
    /// 系统 / 指令上下文（可空）。
    #[serde(default)]
    pub system: Option<String>,
    /// 用户 prompt（通常由 prompt 模板渲染）。
    pub prompt: String,
}

impl CompletionRequest {
    /// 便捷构造：仅 model + prompt。
    pub fn new(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        CompletionRequest {
            model: model.into(),
            system: None,
            prompt: prompt.into(),
        }
    }

    /// 在已有请求基础上追加纠错指引（结构化重试用）。
    pub fn with_repair_hint(&self, hint: &str) -> Self {
        CompletionRequest {
            model: self.model.clone(),
            system: self.system.clone(),
            prompt: format!("{}\n\n{}", self.prompt, hint),
        }
    }
}

/// LLM 补全响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionResponse {
    pub content: String,
}

/// 模型无关的 LLM client 抽象。
///
/// 后端（远端 API / 本地 Ollama 等）实现此 trait。结构化输出的重试 + 验证
/// fallback 在 `complete` 之上由 [`crate::structured::complete_structured`] 统一实现，
/// 因此后端只需实现自由文本补全这一条路径（单一路线）。
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 自由文本补全。后端不可用时返回 [`LlmError::BackendUnavailable`]。
    async fn complete(&self, req: &CompletionRequest) -> LlmResult<CompletionResponse>;
}
