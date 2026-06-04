//! engine 集成测试共用工具。
//!
//! 集中放置各测试文件原先各自重复的：队列化 mock client、schema 取用（复用 prompt
//! crate 的权威 schema，避免手写副本漂移）、图节点 seed 助手、临时目录、请求构造。
//!
//! 各测试二进制只用到本模块的一个子集，故允许「未使用」（共享测试模块惯例）。
#![allow(dead_code)]

use async_trait::async_trait;
use serde_json::Value;
use sophia_graph_db::{
    derive_active_context, snapshot_payload, CodePayload, EdgeKind, GraphStore, NodeId,
    ObjectivePayload, PseudocodePayload,
};
use sophia_llm::{CompletionRequest, CompletionResponse, LlmClient, LlmError};
use std::path::PathBuf;
use std::sync::Mutex;

/// 队列化 mock LLM client：按入队顺序返回响应；队列空则报后端不可用。
pub struct MockClient {
    responses: Mutex<Vec<Result<String, LlmError>>>,
}

impl MockClient {
    /// 用一组预置响应（成功内容或错误）构造。
    pub fn new(responses: Vec<Result<String, LlmError>>) -> Self {
        MockClient {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl LlmClient for MockClient {
    async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut q = self.responses.lock().unwrap();
        if q.is_empty() {
            return Err(LlmError::BackendUnavailable("no more responses".into()));
        }
        q.remove(0).map(|content| CompletionResponse { content })
    }
}

/// 取 prompt crate 的权威 schema（单一事实来源，避免测试手写副本漂移）。
pub fn schema(name: &str) -> Value {
    let src = sophia_prompt::schema_for(name)
        .unwrap_or_else(|| panic!("prompt crate 缺 schema `{name}`"));
    serde_json::from_str(src).expect("内置 schema 应为合法 JSON")
}

/// 便捷请求（model + prompt）。
pub fn req() -> CompletionRequest {
    CompletionRequest::new("m", "do it")
}

pub fn library_policy() -> sophia_engine::LibrarySelectionPolicy {
    sophia_engine::LibrarySelectionPolicy::from_names(["http", "file"])
}

/// 测试用的静态 prompt 提供者：每步都返回固定请求（不依赖 ctx）。
///
/// 单元测试用 MockClient 按队列返回响应，请求正文本身不影响测试结果，故用固定请求即可。
/// 它实现 `StepPrompts`，供 `run_implement_loop` / `run_goal_loop` 注入。
pub struct StaticPrompts;

impl sophia_engine::StepPrompts for StaticPrompts {
    fn decision(
        &self,
        _ctx: &sophia_graph_db::ActiveContext,
        _focus: NodeId,
        _progress: sophia_engine::GoalProgress,
    ) -> CompletionRequest {
        CompletionRequest::new("m", "decide")
    }

    fn design(&self, _ctx: &sophia_graph_db::ActiveContext, _focus: NodeId) -> CompletionRequest {
        CompletionRequest::new("m", "design")
    }

    fn decompose(
        &self,
        _ctx: &sophia_graph_db::ActiveContext,
        _focus: NodeId,
    ) -> CompletionRequest {
        CompletionRequest::new("m", "decompose")
    }

    fn revise(
        &self,
        _ctx: &sophia_graph_db::ActiveContext,
        _focus: NodeId,
        _pseudocode: &str,
        _diagnostics: &[sophia_graph_db::DiagnosticItem],
    ) -> CompletionRequest {
        CompletionRequest::new("m", "revise")
    }

    fn implement(
        &self,
        _ctx: &sophia_graph_db::ActiveContext,
        _focus: NodeId,
        _pseudocode: &str,
        _libraries: &[String],
    ) -> CompletionRequest {
        CompletionRequest::new("m", "implement")
    }

    fn repair(
        &self,
        _ctx: &sophia_graph_db::ActiveContext,
        _focus: NodeId,
        _files: &[(String, String)],
        _diagnostics: &[sophia_graph_db::DiagnosticItem],
        _libraries: &[String],
    ) -> CompletionRequest {
        CompletionRequest::new("m", "repair")
    }
}

/// seed 一个 human ObjectiveNode 作为目标域。
pub fn seed_objective(store: &mut GraphStore) -> NodeId {
    store
        .as_human()
        .objective(
            "goal",
            ObjectivePayload {
                title: "Goal".into(),
                description: "d".into(),
            },
        )
        .unwrap()
}

/// seed 一个最小合法 ContextSnapshot 节点（满足下游 I6 的 consumed→ 目标）。
pub fn seed_snapshot(store: &mut GraphStore) -> NodeId {
    let ctx = derive_active_context(store);
    store
        .as_deterministic()
        .context_snapshot("snap", snapshot_payload(&ctx))
        .unwrap()
}

/// seed 一个 Pseudocode 节点并连 `consumed→ snapshot`（满足 I6）。
pub fn seed_pseudocode(store: &mut GraphStore) -> NodeId {
    let snap = seed_snapshot(store);
    let pseudo = store
        .as_llm()
        .pseudocode(
            "p",
            PseudocodePayload {
                purpose: "do".into(),
                artifact_path: "content.pseudo".into(),
            },
        )
        .unwrap();
    store.append_edge(pseudo, snap, EdgeKind::Consumed).unwrap();
    pseudo
}

/// seed 一个 LLM Code 节点并连 `consumed→ snapshot`（满足 I6）。
pub fn seed_code(store: &mut GraphStore, files: Vec<String>) -> NodeId {
    let snap = seed_snapshot(store);
    let code = store.as_llm().code("code", CodePayload { files }).unwrap();
    store.append_edge(code, snap, EdgeKind::Consumed).unwrap();
    code
}

/// 工作区内唯一临时目录（不写 /tmp 外；带 nanos 后缀去碰撞）。
pub fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sophia_engine_{tag}_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
