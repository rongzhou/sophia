//! 完整工作流闭环：design → implement → repair（构建顺序 step 13+）。
//!
//! 见 docs/language_design.md 10.1 / 10.8、docs/workflow_graph_spec.md 4.4.3 / 4.4.4 /
//! 第六节、第七节接入点。
//!
//! 本模块在 [`crate::run_llm_step`] 之上把单步 LLM 调用串成图上的产物节点 + 边，
//! 固化「动作选择与动作执行分离」之后的**执行**侧（动作选择由 DecisionNode 承载，
//! 见 [`crate::run_llm_step`] + decision schema）：
//!
//! - `design_solution`：建 `PseudocodeNode`，连 `addresses→ 目标域`；
//! - `implement_design`：建 `CodeNode`，连 `addresses→ 目标域` + `implements→ Pseudocode`；
//! - `repair_code`：建新 `CodeNode`，连 `addresses→ 目标域` + `repairs→ 旧 Code`。
//!
//! 每个 LLM 节点的 `consumed→ ContextSnapshot` 边由 `run_llm_step` 建立（I6）。
//! 任一步 LLM 调用失败都返回 [`LoopStepOutcome::Failed`]（已 emit RawLlmNode +
//! `attempted→ 目标域`），**不伪造成功**——闭环到此中止，由调用方据图状态决定后续
//! （重试 / backtrack / 上报）。
//!
//! 产物正文（`.pseudo` 文本、候选 `.sophia` 文件内容）作为 LLM 产物**随 outcome 返回**：
//! 图节点本身不存正文（`PseudocodePayload.artifact_path` 固定 `"content.pseudo"`、
//! `CodePayload.files` 只记路径，见 4.4.3 / 4.4.4），但下游 gate 与物化需要正文，故由
//! [`PseudocodeArtifact`] / [`CodeArtifact`] 承载交给调用方（落盘 / 喂 gate）。

use serde::Deserialize;
use sophia_graph_db::{
    build_decomposition, ActiveContext, ChildGoal, CodePayload, DecompositionNodes, EdgeKind,
    GraphStore, NodeId, NodeRole, PseudocodePayload,
};
use sophia_llm::{CompletionRequest, LlmClient, LlmError, StructuredConfig};

use crate::step::{run_llm_step, step_schema, LlmStepError, LlmStepOutcome};

/// `design_solution` 的结构化 LLM 输出（schema：prompt crate `design_result`）。
#[derive(Debug, Clone, Deserialize)]
struct DesignResult {
    purpose: String,
    pseudocode: String,
    /// 本方案 LLM 在 design 阶段从库目录选中的标准库（默认空——不用库）。见 docs/stdlib_design.md §三。
    #[serde(default)]
    libraries: Vec<String>,
}

/// `decompose` 的单个子目标（schema：prompt crate `decompose_result`）。
#[derive(Debug, Clone, Deserialize)]
struct DecomposeChild {
    title: String,
    description: String,
}

/// `decompose` 的结构化 LLM 输出（schema：prompt crate `decompose_result`）。
#[derive(Debug, Clone, Deserialize)]
struct DecomposeResult {
    rationale: String,
    children: Vec<DecomposeChild>,
}

/// `implement_design` / `repair_code` 的单个候选文件。
#[derive(Debug, Clone, Deserialize)]
struct CandidateFile {
    path: String,
    content: String,
}

/// `implement_design` / `repair_code` 的结构化 LLM 输出。
#[derive(Debug, Clone, Deserialize)]
struct ImplementResult {
    files: Vec<CandidateFile>,
}

/// 设计步骤产物：新建的 PseudocodeNode 与其 `.pseudo` 正文（供上层落盘）。
#[derive(Debug, Clone)]
pub struct PseudocodeArtifact {
    /// 新建的 PseudocodeNode。
    pub node: NodeId,
    /// `.pseudo` 结构化正文（图节点不存正文，由上层写入 `content.pseudo`）。
    pub text: String,
    /// design 阶段 LLM 从库目录选中的标准库名（implement / repair 据此注入完整库资产）。
    /// 默认空 = 不用库。见 docs/stdlib_design.md §三。
    pub libraries: Vec<String>,
}

/// 实现 / 修复步骤产物：新建的 CodeNode 与候选文件（path → content）。
///
/// `files` 携带正文，可直接喂入 `tools/materialize` 的 gate 链
/// （`CodeCandidate::new(files)`）。CodeNode 本身只记路径（4.4.4）。
#[derive(Debug, Clone)]
pub struct CodeArtifact {
    /// 新建的 CodeNode。
    pub node: NodeId,
    /// 候选文件（path → content）。
    pub files: Vec<(String, String)>,
}

/// 拆解步骤产物：新建的 Decomposition 节点与子目标 Objective 节点。
///
/// 子目标可作为遍历层递归推进的新焦点（design 10.8 动作 6）。Decomposition 与子目标
/// 都是 LLM-provenance 的结构性派生节点，其上下文可复现性由触发 decompose 的
/// `DecisionNode`（`consumed→ snapshot`）承载（见 graph-db `decomposition` 模块）。
#[derive(Debug, Clone)]
pub struct DecompositionArtifact {
    /// 新建的 Decomposition 节点。
    pub decomposition: NodeId,
    /// 子目标 Objective 节点（与 LLM 给出的顺序一致）。
    pub children: Vec<NodeId>,
}

/// 一步工作流的结果：成功返回产物 `A`，失败返回 RawLlmNode + 错误。
#[derive(Debug)]
pub enum LoopStepOutcome<A> {
    /// 成功：新建产物（Pseudocode 或 Code 工件）。
    Succeeded(A),
    /// 失败：已 emit RawLlmNode（attempted→ 目标域），返回其 NodeId 与错误。
    Failed { raw_llm: NodeId, error: LlmError },
}

/// 工作流闭环错误（区别于「LLM 调用失败」——后者走 [`LoopStepOutcome::Failed`]）。
pub type LoopError = LlmStepError;

/// `design_solution`：为目标域生成结构化伪代码节点。
///
/// `target` 必须是 `addresses→` 允许的目标域（Objective | Milestone | FirstSlice）。
/// `render` 是调用时刻的 prompt 渲染器（接收与本次 snapshot 同源的 active context，
/// 见 engineering_architecture §8.4）。成功后建 `PseudocodeNode` 并连 `addresses→ target`，
/// 返回 [`PseudocodeArtifact`]（含 `.pseudo` 正文）。schema 固定取内置 `design_result`。
pub async fn design_solution<C, R>(
    store: &mut GraphStore,
    client: &C,
    render: R,
    config: &StructuredConfig,
    target: NodeId,
) -> Result<LoopStepOutcome<PseudocodeArtifact>, LoopError>
where
    C: LlmClient + ?Sized,
    R: FnOnce(&ActiveContext) -> CompletionRequest,
{
    ensure_addressable(store, target)?;

    let outcome: LlmStepOutcome<DesignResult> = run_llm_step(
        store,
        client,
        render,
        &step_schema("design_result"),
        config,
        target,
        "design_solution",
    )
    .await?;

    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            let pseudo = store.as_llm().pseudocode(
                format!("pseudo:{}", value.purpose),
                PseudocodePayload {
                    purpose: value.purpose,
                    artifact_path: "content.pseudo".to_string(),
                },
            )?;
            store.append_edge(pseudo, snapshot, EdgeKind::Consumed)?;
            store.append_edge(pseudo, target, EdgeKind::Addresses)?;
            Ok(LoopStepOutcome::Succeeded(PseudocodeArtifact {
                node: pseudo,
                text: value.pseudocode,
                libraries: value.libraries,
            }))
        }
        LlmStepOutcome::Failed { raw_llm, error } => Ok(LoopStepOutcome::Failed { raw_llm, error }),
    }
}

/// `decompose`：把过大的目标拆成若干子目标。
///
/// `target` 必须是 `Objective`（只有目标可被拆解为子目标，6.1 `decomposes` 边
/// from=Objective）。`render` 是调用时刻的 prompt 渲染器（§8.4）。LLM 给出拆解结构
/// （rationale + children），本步用确定性 helper [`build_decomposition`] 落为图：建
/// `Decomposition` 节点、`parent decomposes→ Decomposition`、每个子目标建 `Objective`
/// 并 `member_of→ Decomposition`。schema 固定取内置 `decompose_result`。
///
/// 触发本步的 `DecisionNode(decompose)` 是"该不该拆"的决策调用；本步是"怎么拆"的执行
/// 调用——两次独立的 LLM 调用、各有 snapshot（§10.8 动作选择与执行分离）。因此本步产出的
/// `Decomposition` 作为执行产物节点 `consumed→` 本次调用的 snapshot（I6，与 design 的
/// Pseudocode、implement 的 Code 同构）；子 `Objective` 作为结构性派生节点经 `member_of`
/// 间接锚定，不单独携带 snapshot。子目标过少（<2）由 helper 拒绝（视为无效拆解）。
pub async fn decompose_goal<C, R>(
    store: &mut GraphStore,
    client: &C,
    render: R,
    config: &StructuredConfig,
    target: NodeId,
) -> Result<LoopStepOutcome<DecompositionArtifact>, LoopError>
where
    C: LlmClient + ?Sized,
    R: FnOnce(&ActiveContext) -> CompletionRequest,
{
    if store.role_of(target) != Some(NodeRole::Objective) {
        return Err(role_mismatch(target, "Objective"));
    }

    let outcome: LlmStepOutcome<DecomposeResult> = run_llm_step(
        store,
        client,
        render,
        &step_schema("decompose_result"),
        config,
        target,
        "decompose",
    )
    .await?;

    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            let children: Vec<ChildGoal> = value
                .children
                .into_iter()
                .map(|c| ChildGoal {
                    title: c.title,
                    description: c.description,
                })
                .collect();
            // 确定性落图：build_decomposition 校验 parent 是 Objective、snapshot 合法、
            // children >= 2，并把 Decomposition consumed→ snapshot（I6）。
            let DecompositionNodes {
                decomposition,
                children,
            } = build_decomposition(store, target, snapshot, value.rationale, &children)?;
            Ok(LoopStepOutcome::Succeeded(DecompositionArtifact {
                decomposition,
                children,
            }))
        }
        LlmStepOutcome::Failed { raw_llm, error } => Ok(LoopStepOutcome::Failed { raw_llm, error }),
    }
}

/// `revise_design`：当实现暴露**概念性**问题时重写伪代码，产出新 Pseudocode 版本。
///
/// `prev_pseudocode` 是被修订的现有 Pseudocode 节点；`target` 是 `addresses→` 目标域。
/// 成功后建新 `PseudocodeNode` 并连 `addresses→ target` + `revises→ prev_pseudocode`
/// （旧版本保留，见 4.4.3）。`render` 是调用时刻的 prompt 渲染器（§8.4）。schema 固定
/// 取内置 `design_result`（修订产物与 design 同形：purpose + pseudocode）。
pub async fn revise_design<C, R>(
    store: &mut GraphStore,
    client: &C,
    render: R,
    config: &StructuredConfig,
    target: NodeId,
    prev_pseudocode: NodeId,
) -> Result<LoopStepOutcome<PseudocodeArtifact>, LoopError>
where
    C: LlmClient + ?Sized,
    R: FnOnce(&ActiveContext) -> CompletionRequest,
{
    ensure_addressable(store, target)?;
    if store.role_of(prev_pseudocode) != Some(NodeRole::Pseudocode) {
        return Err(role_mismatch(prev_pseudocode, "Pseudocode"));
    }

    let outcome: LlmStepOutcome<DesignResult> = run_llm_step(
        store,
        client,
        render,
        &step_schema("design_result"),
        config,
        target,
        "revise_design",
    )
    .await?;

    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            let pseudo = store.as_llm().pseudocode(
                format!("pseudo(revised):{}", value.purpose),
                PseudocodePayload {
                    purpose: value.purpose,
                    artifact_path: "content.pseudo".to_string(),
                },
            )?;
            store.append_edge(pseudo, snapshot, EdgeKind::Consumed)?;
            store.append_edge(pseudo, target, EdgeKind::Addresses)?;
            store.append_edge(pseudo, prev_pseudocode, EdgeKind::Revises)?;
            Ok(LoopStepOutcome::Succeeded(PseudocodeArtifact {
                node: pseudo,
                text: value.pseudocode,
                libraries: value.libraries,
            }))
        }
        LlmStepOutcome::Failed { raw_llm, error } => Ok(LoopStepOutcome::Failed { raw_llm, error }),
    }
}
///
/// `pseudocode` 必须是 Pseudocode 节点；`target` 是 `addresses→` 目标域。
/// `render` 是调用时刻的 prompt 渲染器（§8.4）。成功后建 `CodeNode` 并连 `addresses→
/// target` + `implements→ pseudocode`，返回 [`CodeArtifact`]。schema 固定取 `implement_result`。
pub async fn implement_design<C, R>(
    store: &mut GraphStore,
    client: &C,
    render: R,
    config: &StructuredConfig,
    target: NodeId,
    pseudocode: NodeId,
) -> Result<LoopStepOutcome<CodeArtifact>, LoopError>
where
    C: LlmClient + ?Sized,
    R: FnOnce(&ActiveContext) -> CompletionRequest,
{
    ensure_addressable(store, target)?;
    if store.role_of(pseudocode) != Some(NodeRole::Pseudocode) {
        return Err(role_mismatch(pseudocode, "Pseudocode"));
    }

    let outcome: LlmStepOutcome<ImplementResult> = run_llm_step(
        store,
        client,
        render,
        &step_schema("implement_result"),
        config,
        target,
        "implement_design",
    )
    .await?;

    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            let artifact = build_code_node(store, value, snapshot, target)?;
            store.append_edge(artifact.node, pseudocode, EdgeKind::Implements)?;
            Ok(LoopStepOutcome::Succeeded(artifact))
        }
        LlmStepOutcome::Failed { raw_llm, error } => Ok(LoopStepOutcome::Failed { raw_llm, error }),
    }
}

/// `repair_code`：根据诊断生成修复后的新 CodeNode。
///
/// `prev_code` 是被修复的旧 Code 节点；`target` 是 `addresses→` 目标域。
/// `render` 是调用时刻的 prompt 渲染器（§8.4）。成功后建新 `CodeNode` 并连 `addresses→
/// target` + `repairs→ prev_code`，返回 [`CodeArtifact`]。schema 固定取 `repair_result`。
pub async fn repair_code<C, R>(
    store: &mut GraphStore,
    client: &C,
    render: R,
    config: &StructuredConfig,
    target: NodeId,
    prev_code: NodeId,
) -> Result<LoopStepOutcome<CodeArtifact>, LoopError>
where
    C: LlmClient + ?Sized,
    R: FnOnce(&ActiveContext) -> CompletionRequest,
{
    ensure_addressable(store, target)?;
    if store.role_of(prev_code) != Some(NodeRole::Code) {
        return Err(role_mismatch(prev_code, "Code"));
    }

    let outcome: LlmStepOutcome<ImplementResult> = run_llm_step(
        store,
        client,
        render,
        &step_schema("repair_result"),
        config,
        target,
        "repair_code",
    )
    .await?;

    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            let artifact = build_code_node(store, value, snapshot, target)?;
            store.append_edge(artifact.node, prev_code, EdgeKind::Repairs)?;
            Ok(LoopStepOutcome::Succeeded(artifact))
        }
        LlmStepOutcome::Failed { raw_llm, error } => Ok(LoopStepOutcome::Failed { raw_llm, error }),
    }
}

/// 建一个 CodeNode 并连 `consumed→ snapshot` + `addresses→ target`（implement / repair 共用）。
/// 返回携带候选文件正文的 [`CodeArtifact`]。
fn build_code_node(
    store: &mut GraphStore,
    value: ImplementResult,
    snapshot: NodeId,
    target: NodeId,
) -> Result<CodeArtifact, LoopError> {
    let files: Vec<(String, String)> = value
        .files
        .into_iter()
        .map(|f| (f.path, f.content))
        .collect();
    let paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let code = store.as_llm().code(
        format!("code:{} files", paths.len()),
        CodePayload { files: paths },
    )?;
    store.append_edge(code, snapshot, EdgeKind::Consumed)?;
    store.append_edge(code, target, EdgeKind::Addresses)?;
    Ok(CodeArtifact { node: code, files })
}

/// 校验 `target` 是 `addresses→` 允许的目标域（Objective | Milestone | FirstSlice）。
fn ensure_addressable(store: &GraphStore, target: NodeId) -> Result<(), LoopError> {
    match store.role_of(target) {
        Some(NodeRole::Objective | NodeRole::Milestone | NodeRole::FirstSlice) => Ok(()),
        _ => Err(LlmStepError::Graph(
            sophia_graph_db::GraphError::InvalidPayload(format!(
                "{target} 不是 addresses→ 允许的目标域（Objective | Milestone | FirstSlice）"
            )),
        )),
    }
}

/// 构造 role 不匹配错误。
fn role_mismatch(node: NodeId, expected: &str) -> LoopError {
    LlmStepError::Graph(sophia_graph_db::GraphError::InvalidPayload(format!(
        "{node} 不是预期的 {expected} 节点"
    )))
}
