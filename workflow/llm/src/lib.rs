//! 模型无关的 LLM 抽象层。
//!
//! 见 docs/engineering_architecture.md 第七节。
//! - [`LlmClient`] 是模型无关后端抽象，只需实现自由文本 `complete`（单一路线）；
//! - [`HttpLlmClient`] 是具体后端，支持两种模式（[`BackendMode`]）：OpenAI 兼容
//!   （`/chat/completions`）与 Ollama（`/api/chat`）；
//! - [`complete_structured`] 在其上叠加 JSON 提取 + `jsonschema` 验证 + 重试 fallback：
//!   验证失败携带错误信息重试，超过次数返回结构化错误，**不伪造成功结果**；
//! - 失败必须由上层 emit `RawLlmNode`（见 docs/workflow_graph_spec.md 4.4.8）。
//!
//! 本 crate 属 workflow 层（异步）。

#![forbid(unsafe_code)]

mod backend;
mod client;
mod structured;

pub use backend::{BackendConfig, BackendMode, HttpLlmClient};
pub use client::{CompletionRequest, CompletionResponse, LlmClient, LlmError, LlmResult};
pub use structured::{complete_structured, StructuredConfig};
