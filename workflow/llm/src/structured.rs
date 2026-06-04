//! 结构化输出：重试 + schema 验证 fallback。
//!
//! 见 docs/engineering_architecture.md 7.2。本地模型无法使用原生 Structured Outputs
//! API，需在此实现：
//! 1. 请求 LLM 输出 JSON（schema 描述已嵌入 prompt）；
//! 2. 从响应中提取 JSON，用 `jsonschema` 验证；
//! 3. 验证失败时携带错误信息重试，最多 N 次；
//! 4. 超过重试次数返回结构化错误，**不伪造成功结果**（失败由上层 emit `RawLlmNode`）。
//!
//! schema 标记 `additionalProperties: false`（strict 模式，workflow_graph_spec 1.3）；
//! 服务端用同一 schema 复验，不接受「宽松解析 + 事后过滤」。

use crate::client::{CompletionRequest, LlmClient, LlmError, LlmResult};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptFailureKind {
    Parse,
    Schema,
}

/// 结构化补全配置。
#[derive(Debug, Clone)]
pub struct StructuredConfig {
    /// 最大重试次数（首次尝试不计入；总尝试 = max_retries + 1）。
    pub max_retries: u32,
}

impl Default for StructuredConfig {
    fn default() -> Self {
        // 与本地模型 fallback 的常见取值一致；可由调用方覆盖。
        StructuredConfig { max_retries: 2 }
    }
}

/// 结构化补全：在 `complete` 之上叠加 JSON 提取 + schema 验证 + 重试。
///
/// 成功返回反序列化后的 `T`。任一失败模式（后端不可用、解析失败、超重试的
/// 验证失败）都返回结构化 [`LlmError`]，由调用方据此 emit `RawLlmNode`。
pub async fn complete_structured<T, C>(
    client: &C,
    req: &CompletionRequest,
    schema: &Value,
    config: &StructuredConfig,
) -> LlmResult<T>
where
    T: DeserializeOwned,
    C: LlmClient + ?Sized,
{
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| LlmError::Parse(format!("schema 非法：{e}")))?;

    let mut attempt: u32 = 0;
    let mut cur_req = req.clone();
    let mut saw_schema_failure = false;

    loop {
        // 后端调用失败立即上报（不重试网络问题，避免放大不可用）。
        let resp = client.complete_with_schema(&cur_req, schema).await?;

        let (last_errors, failure_kind) = match extract_json(&resp.content) {
            Ok(value) => {
                let errors = collect_validation_errors(&validator, &value);
                if errors.is_empty() {
                    // 验证通过，反序列化到目标类型。
                    return serde_json::from_value::<T>(value)
                        .map_err(|e| LlmError::Parse(format!("schema 通过但反序列化失败：{e}")));
                }
                (errors.join("; "), AttemptFailureKind::Schema)
            }
            Err(parse_err) => (parse_err, AttemptFailureKind::Parse),
        };
        saw_schema_failure |= failure_kind == AttemptFailureKind::Schema;

        if attempt >= config.max_retries {
            return if saw_schema_failure {
                Err(LlmError::SchemaValidation {
                    attempts: attempt + 1,
                    last_errors,
                })
            } else {
                Err(LlmError::Parse(format!(
                    "重试 {} 次后仍无法提取 JSON：{last_errors}",
                    attempt + 1
                )))
            };
        }
        // 携带错误信息重试。
        attempt += 1;
        cur_req = req.with_repair_hint(&repair_hint(&last_errors));
    }
}

/// 从 LLM 响应文本中提取 JSON 对象。
///
/// 容忍模型在 JSON 前后附带说明文字；优先取 markdown `json` code block，
/// 否则扫描首个完整 JSON object。提取后仍会用 schema 严格验证。
fn extract_json(content: &str) -> Result<Value, String> {
    let trimmed = content.trim();
    // 优先整体解析。
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }

    if let Some(block) = first_json_code_block(trimmed) {
        return serde_json::from_str::<Value>(block.trim())
            .map_err(|err| format!("JSON code block 解析失败：{err}"));
    }

    if let Some(obj) = first_complete_json_object(trimmed) {
        return serde_json::from_str::<Value>(obj).map_err(|err| format!("JSON 解析失败：{err}"));
    }
    Err("响应中未找到 JSON 对象".to_string())
}

fn first_json_code_block(content: &str) -> Option<&str> {
    let mut rest = content;
    loop {
        let start = rest.find("```")?;
        let after_ticks = &rest[start + 3..];
        let line_end = after_ticks.find('\n')?;
        let lang = after_ticks[..line_end].trim();
        let body = &after_ticks[line_end + 1..];
        let end = body.find("```")?;
        if lang.eq_ignore_ascii_case("json") {
            return Some(&body[..end]);
        }
        rest = &body[end + 3..];
    }
}

fn first_complete_json_object(content: &str) -> Option<&str> {
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in content.char_indices() {
        if start.is_none() {
            if ch == '{' {
                start = Some(idx);
                depth = 1;
            }
            continue;
        }

        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = idx + ch.len_utf8();
                    return Some(&content[start.unwrap()..end]);
                }
            }
            _ => {}
        }
    }
    None
}

/// 收集 schema 验证错误（稳定顺序）。
fn collect_validation_errors(validator: &jsonschema::Validator, instance: &Value) -> Vec<String> {
    validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect()
}

/// 构造重试纠错提示。
fn repair_hint(errors: &str) -> String {
    format!(
        "上一次输出不符合要求的 JSON schema。错误如下：\n{errors}\n\
         请只输出严格符合 schema 的 JSON，不要包含额外字段或说明文字。"
    )
}
