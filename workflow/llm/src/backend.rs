//! 具体 LLM 后端：OpenAI 兼容与 Ollama 两种模式。
//!
//! 见 docs/engineering_architecture.md 7.1–7.3。后端只实现自由文本 `complete`
//! （单一路线）；结构化输出的重试 + schema 验证由 [`crate::complete_structured`]
//! 在其上统一处理。两种模式共享同一请求形状（system + user 两条 message），并都用
//! streaming 响应，避免把“整段生成耗时”误当成请求超时；仅 endpoint / 流格式不同。
//!
//! 后端不可用（网络错误 / 非 2xx）一律返回 [`LlmError::BackendUnavailable`]，
//! 由上层据此 emit `RawLlmNode`（7.3），绝不伪造成功。

use crate::client::{CompletionRequest, CompletionResponse, LlmClient, LlmError, LlmResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// 后端模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendMode {
    /// OpenAI 兼容：`POST {base}/chat/completions`。
    OpenAiCompatible,
    /// Ollama：`POST {base}/api/chat`（streaming）。
    Ollama,
}

/// HTTP LLM 后端配置。
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub mode: BackendMode,
    /// 基地址（不含具体路径），如 `https://api.openai.com/v1` 或 `http://localhost:11434`。
    pub base_url: String,
    /// API key（OpenAI 兼容用；Ollama 通常留空）。
    pub api_key: Option<String>,
    /// 响应读取空闲超时（秒）。生成类请求不应限制整段输出总耗时；只限制连接 / 读取长期无进展。
    pub timeout_secs: u64,
}

impl BackendConfig {
    /// OpenAI 兼容默认配置（指向官方 v1 端点）。
    pub fn openai(api_key: impl Into<String>) -> Self {
        BackendConfig {
            mode: BackendMode::OpenAiCompatible,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key.into()),
            timeout_secs: 120,
        }
    }

    /// Ollama 本地默认配置。
    pub fn ollama() -> Self {
        BackendConfig {
            mode: BackendMode::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            timeout_secs: 300,
        }
    }

    /// 覆盖 base_url（自定义 OpenAI 兼容网关 / 远端 Ollama）。
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }
}

/// 基于 `reqwest` 的 HTTP LLM 后端。
pub struct HttpLlmClient {
    config: BackendConfig,
    http: reqwest::Client,
}

impl HttpLlmClient {
    /// 构造后端。HTTP client 初始化失败（极少见）视为后端不可用。
    pub fn new(config: BackendConfig) -> LlmResult<Self> {
        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let connect_timeout = std::time::Duration::from_secs(config.timeout_secs.min(30));
        let http = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .read_timeout(timeout)
            .build()
            .map_err(|e| LlmError::BackendUnavailable(format!("HTTP client 初始化失败：{e}")))?;
        Ok(HttpLlmClient { config, http })
    }

    fn endpoint(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        match self.config.mode {
            BackendMode::OpenAiCompatible => format!("{base}/chat/completions"),
            BackendMode::Ollama => format!("{base}/api/chat"),
        }
    }

    /// 构造两条 message（system 可选 + user）。
    fn messages(req: &CompletionRequest) -> Vec<ChatMessage> {
        let mut msgs = Vec::new();
        if let Some(system) = &req.system {
            msgs.push(ChatMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }
        msgs.push(ChatMessage {
            role: "user".to_string(),
            content: req.prompt.clone(),
        });
        msgs
    }
}

#[async_trait]
impl LlmClient for HttpLlmClient {
    async fn complete(&self, req: &CompletionRequest) -> LlmResult<CompletionResponse> {
        let messages = Self::messages(req);
        let url = self.endpoint();

        let mut builder = self.http.post(&url);
        if let Some(key) = &self.config.api_key {
            builder = builder.bearer_auth(key);
        }

        // 请求体按模式区分。
        let resp = match self.config.mode {
            BackendMode::OpenAiCompatible => {
                let body = OpenAiRequest {
                    model: &req.model,
                    messages: &messages,
                    // 低温度以稳定结构化输出（重试 fallback 仍兜底）。
                    temperature: 0.0,
                    stream: true,
                };
                builder.json(&body).send().await
            }
            BackendMode::Ollama => {
                let body = OllamaRequest {
                    model: &req.model,
                    messages: &messages,
                    stream: true,
                };
                builder.json(&body).send().await
            }
        };

        let resp = resp.map_err(|e| {
            LlmError::BackendUnavailable(format!("请求失败：{}", describe_reqwest_error(&e)))
        })?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| {
            LlmError::BackendUnavailable(format!("读取响应失败：{}", describe_reqwest_error(&e)))
        })?;

        if !status.is_success() {
            return Err(LlmError::BackendUnavailable(format!(
                "后端返回 {status}：{}",
                truncate(&text, 300)
            )));
        }

        // 按模式解析响应内容。
        let content = match self.config.mode {
            BackendMode::OpenAiCompatible => parse_openai_stream_or_response(&text)?,
            BackendMode::Ollama => parse_ollama_stream(&text)?,
        };

        Ok(CompletionResponse { content })
    }
}

fn parse_openai_stream_or_response(text: &str) -> LlmResult<String> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        let parsed: OpenAiResponse = serde_json::from_str(trimmed)
            .map_err(|e| LlmError::Parse(format!("OpenAI 响应解析失败：{e}")))?;
        return parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::Parse("OpenAI 响应无 choices".to_string()));
    }

    let mut content = String::new();
    let mut saw_event = false;
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(content);
        }
        let parsed: OpenAiStreamChunk = serde_json::from_str(data).map_err(|e| {
            LlmError::Parse(format!("OpenAI stream 第 {} 行解析失败：{e}", idx + 1))
        })?;
        saw_event = true;
        for choice in parsed.choices {
            if let Some(delta) = choice.delta {
                if let Some(piece) = delta.content {
                    content.push_str(&piece);
                }
            }
        }
    }
    if saw_event {
        Err(LlmError::Parse(
            "OpenAI stream 未收到 [DONE] 结束标记".to_string(),
        ))
    } else {
        Err(LlmError::Parse("OpenAI stream 响应为空".to_string()))
    }
}

fn parse_ollama_stream(text: &str) -> LlmResult<String> {
    let mut content = String::new();
    let mut saw_chunk = false;
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: OllamaStreamChunk = serde_json::from_str(line).map_err(|e| {
            LlmError::Parse(format!("Ollama stream 第 {} 行解析失败：{e}", idx + 1))
        })?;
        saw_chunk = true;
        if let Some(message) = parsed.message {
            content.push_str(&message.content);
        }
        if parsed.done {
            return Ok(content);
        }
    }
    if saw_chunk {
        Err(LlmError::Parse(
            "Ollama stream 未收到 done=true 结束标记".to_string(),
        ))
    } else {
        Err(LlmError::Parse("Ollama stream 响应为空".to_string()))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}…")
    }
}

fn describe_reqwest_error(e: &reqwest::Error) -> String {
    let mut tags = Vec::new();
    if e.is_timeout() {
        tags.push("timeout");
    }
    if e.is_connect() {
        tags.push("connect");
    }
    if e.is_request() {
        tags.push("request");
    }
    if e.is_body() {
        tags.push("body");
    }
    let tag = if tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", tags.join(","))
    };
    let mut msg = format!("{e}{tag}");
    let mut source = e.source();
    while let Some(err) = source {
        msg.push_str(&format!("；原因：{err}"));
        source = err.source();
    }
    msg
}

// ---- 线上协议 DTO ----

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    stream: bool,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    #[serde(default)]
    delta: Option<OpenAiDelta>,
}

#[derive(Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    done: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompletionRequest;

    #[test]
    fn openai_endpoint() {
        let c =
            HttpLlmClient::new(BackendConfig::openai("k").with_base_url("https://x/v1")).unwrap();
        assert_eq!(c.endpoint(), "https://x/v1/chat/completions");
    }

    #[test]
    fn ollama_endpoint() {
        let c = HttpLlmClient::new(BackendConfig::ollama()).unwrap();
        assert_eq!(c.endpoint(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn trailing_slash_in_base_url_normalized() {
        let c =
            HttpLlmClient::new(BackendConfig::openai("k").with_base_url("https://x/v1/")).unwrap();
        assert_eq!(c.endpoint(), "https://x/v1/chat/completions");
    }

    #[test]
    fn messages_include_system_and_user() {
        let mut req = CompletionRequest::new("m", "do it");
        req.system = Some("you are sophia".into());
        let msgs = HttpLlmClient::messages(&req);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "do it");
    }

    #[test]
    fn messages_user_only_when_no_system() {
        let req = CompletionRequest::new("m", "hi");
        let msgs = HttpLlmClient::messages(&req);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
    }

    #[test]
    fn openai_response_parses() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"hello"}}]}"#;
        assert_eq!(parse_openai_stream_or_response(raw).unwrap(), "hello");
    }

    #[test]
    fn openai_stream_response_parses() {
        let raw = "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
                   data: [DONE]\n";
        assert_eq!(parse_openai_stream_or_response(raw).unwrap(), "hello");
    }

    #[test]
    fn openai_stream_requires_done_marker() {
        let raw = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n";
        let err = parse_openai_stream_or_response(raw).unwrap_err();
        assert!(matches!(err, LlmError::Parse(msg) if msg.contains("[DONE]")));
    }

    #[test]
    fn ollama_stream_response_parses() {
        let raw = "{\"model\":\"qwen3\",\"message\":{\"role\":\"assistant\",\"content\":\"h\"},\"done\":false}\n\
                   {\"model\":\"qwen3\",\"message\":{\"role\":\"assistant\",\"content\":\"i\"},\"done\":false}\n\
                   {\"model\":\"qwen3\",\"done\":true}";
        assert_eq!(parse_ollama_stream(raw).unwrap(), "hi");
    }

    #[test]
    fn ollama_stream_requires_done_marker() {
        let raw =
            "{\"model\":\"qwen3\",\"message\":{\"role\":\"assistant\",\"content\":\"partial\"}}";
        let err = parse_ollama_stream(raw).unwrap_err();
        assert!(matches!(err, LlmError::Parse(msg) if msg.contains("done=true")));
    }
}
