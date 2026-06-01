//! 结构化输出 fallback 测试：用 mock client（队列化响应）驱动重试 / 验证路径。

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sophia_llm::{
    complete_structured, CompletionRequest, CompletionResponse, LlmClient, LlmError,
    StructuredConfig,
};
use std::sync::Mutex;

/// mock client：按顺序返回预设响应；记录被调用次数。
/// 用 `Mutex` 满足 `LlmClient: Send + Sync`，无需 unsafe。
struct MockClient {
    responses: Mutex<Vec<Result<String, LlmError>>>,
    calls: Mutex<u32>,
}

impl MockClient {
    fn new(responses: Vec<Result<String, LlmError>>) -> Self {
        MockClient {
            responses: Mutex::new(responses),
            calls: Mutex::new(0),
        }
    }
    fn call_count(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl LlmClient for MockClient {
    async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        *self.calls.lock().unwrap() += 1;
        let mut q = self.responses.lock().unwrap();
        if q.is_empty() {
            return Err(LlmError::BackendUnavailable(
                "no more mock responses".into(),
            ));
        }
        let item = q.remove(0);
        item.map(|content| CompletionResponse { content })
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Decision {
    action: String,
    confidence: u32,
}

fn decision_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["action", "confidence"],
        "properties": {
            "action": { "type": "string" },
            "confidence": { "type": "integer" }
        }
    })
}

#[tokio::test]
async fn valid_first_response_succeeds_without_retry() {
    let client = MockClient::new(vec![Ok(r#"{"action":"design","confidence":3}"#.into())]);
    let req = CompletionRequest::new("test", "decide");
    let out: Decision = complete_structured(
        &client,
        &req,
        &decision_schema(),
        &StructuredConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        out,
        Decision {
            action: "design".into(),
            confidence: 3
        }
    );
    assert_eq!(client.call_count(), 1);
}

#[tokio::test]
async fn extracts_json_embedded_in_prose() {
    // 模型在 JSON 前后加了说明文字；提取后仍需通过 schema 严格验证。
    let client = MockClient::new(vec![Ok(
        "Here is my answer:\n{\"action\":\"implement\",\"confidence\":5}\nHope it helps!".into(),
    )]);
    let req = CompletionRequest::new("test", "decide");
    let out: Decision = complete_structured(
        &client,
        &req,
        &decision_schema(),
        &StructuredConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(out.action, "implement");
}

#[tokio::test]
async fn retries_then_succeeds() {
    // 首次缺字段（schema 失败），第二次正确。
    let client = MockClient::new(vec![
        Ok(r#"{"action":"design"}"#.into()),
        Ok(r#"{"action":"design","confidence":1}"#.into()),
    ]);
    let req = CompletionRequest::new("test", "decide");
    let cfg = StructuredConfig { max_retries: 2 };
    let out: Decision = complete_structured(&client, &req, &decision_schema(), &cfg)
        .await
        .unwrap();
    assert_eq!(out.confidence, 1);
    assert_eq!(client.call_count(), 2);
}

#[tokio::test]
async fn additional_properties_rejected_strict() {
    // 多余字段（additionalProperties:false）→ 验证失败，重试耗尽后报错。
    let extra = r#"{"action":"design","confidence":1,"sneaky":true}"#;
    let client = MockClient::new(vec![Ok(extra.into()), Ok(extra.into()), Ok(extra.into())]);
    let req = CompletionRequest::new("test", "decide");
    let cfg = StructuredConfig { max_retries: 2 };
    let err = complete_structured::<Decision, _>(&client, &req, &decision_schema(), &cfg)
        .await
        .unwrap_err();
    match err {
        LlmError::SchemaValidation { attempts, .. } => assert_eq!(attempts, 3),
        other => panic!("期望 SchemaValidation，得到 {other:?}"),
    }
    // 首次 + 2 次重试 = 3 次调用。
    assert_eq!(client.call_count(), 3);
}

#[tokio::test]
async fn exhausted_retries_returns_structured_error_not_fake_success() {
    let client = MockClient::new(vec![
        Ok("not json at all".into()),
        Ok("still not json".into()),
        Ok("nope".into()),
    ]);
    let req = CompletionRequest::new("test", "decide");
    let cfg = StructuredConfig { max_retries: 2 };
    let result = complete_structured::<Decision, _>(&client, &req, &decision_schema(), &cfg).await;
    assert!(matches!(result, Err(LlmError::SchemaValidation { .. })));
}

#[tokio::test]
async fn backend_unavailable_surfaces_immediately() {
    // 后端不可用不重试（避免放大不可用），立即上报。
    let client = MockClient::new(vec![Err(LlmError::BackendUnavailable("down".into()))]);
    let req = CompletionRequest::new("test", "decide");
    let result = complete_structured::<Decision, _>(
        &client,
        &req,
        &decision_schema(),
        &StructuredConfig::default(),
    )
    .await;
    assert!(matches!(result, Err(LlmError::BackendUnavailable(_))));
    assert_eq!(client.call_count(), 1);
}
