//! Implement-loop：预算受限的 implement → code_check → repair 闭环
//! （构建顺序 step 13+，对应 CLI `sophia graph implement-loop`，架构 §9.2）。
//!
//! 见 docs/language_design.md 10.1（工作流图）/ 10.8（动作选择）/ 10.9（预算）、
//! docs/workflow_graph_spec.md 4.4.5（DiagnosticNode）/ 第六节（`checks→`）。
//!
//! 本模块把 [`crate::implement_design`] / [`crate::repair_code`] 串成一个收敛循环：
//! 1. `implement_design`：伪代码 → 候选 CodeNode；
//! 2. 注入的**确定性 code_check**（[`CodeChecker`]）对候选文件出具 `DiagnosticPayload`
//!    （kind=CodeCheck）；engine emit `DiagnosticNode` 并连 `checks→ Code`；
//! 3. 若 `ok` → 返回通过的候选（可直接喂 `tools/materialize` gate / [`crate::run_selection_materialize`]）；
//! 4. 否则在 `max_repair_attempts`（design 10.9 `max_repair_attempts_per_code_node`）预算内
//!    `repair_code`（据诊断重渲染 repair prompt），回到第 2 步；
//! 5. 预算耗尽仍未通过 → 返回 `BudgetExhausted`（保留最后候选与诊断节点，供 backtrack / 上报）。
//!
//! 分层纪律：engine 不自行运行 checker（那是 `tools/check`，属 tools 层）。check 由调用方
//! 注入结果——与 `tools/materialize` 消费注入的 `GateReport` 同构。LLM 调用失败仍走
//! RawLlmNode 兜底，**不伪造成功**。

use sophia_graph_db::ActiveContext;
use sophia_graph_db::{
    DiagnosticKind, DiagnosticPayload, EdgeKind, GraphError, GraphStore, NodeId,
};
use sophia_llm::{LlmClient, LlmError, StructuredConfig};
use thiserror::Error;

use crate::loop_steps::{implement_design, repair_code, LoopStepOutcome};
use crate::prompts::StepPrompts;
use crate::step::LlmStepError;

/// 注入的确定性 code_check：对候选文件（path → content）出具诊断。
///
/// 由调用方用 `tools/check` 的结果构造（engine 不自行运行 checker，保持分层）。
/// 返回的 `DiagnosticPayload.kind` 必须为 `CodeCheck`，否则 [`run_implement_loop`]
/// 报 [`ImplementLoopError::WrongDiagnosticKind`]。
pub trait CodeChecker {
    /// 检查候选文件，返回 code_check 诊断（`ok` + 诊断项）。
    fn check(&mut self, files: &[(String, String)]) -> DiagnosticPayload;
}

impl<F> CodeChecker for F
where
    F: FnMut(&[(String, String)]) -> DiagnosticPayload,
{
    fn check(&mut self, files: &[(String, String)]) -> DiagnosticPayload {
        self(files)
    }
}

/// implement-loop 配置。
#[derive(Debug, Clone)]
pub struct ImplementLoopConfig {
    /// 单个 code 节点的最大修复次数（design 10.9）。0 表示只 implement、不修复。
    pub max_repair_attempts: u32,
    /// 每次 LLM 调用的结构化重试配置。
    pub structured: StructuredConfig,
}

impl Default for ImplementLoopConfig {
    fn default() -> Self {
        ImplementLoopConfig {
            max_repair_attempts: 2,
            structured: StructuredConfig::default(),
        }
    }
}

/// implement-loop 结果。
#[derive(Debug)]
pub enum ImplementLoopOutcome {
    /// code_check 通过：返回通过的候选（CodeNode + 候选文件 + 已尝试次数）。
    Passed {
        code: NodeId,
        files: Vec<(String, String)>,
        /// 含初次 implement 在内的总尝试次数（1 = 一次通过）。
        attempts: u32,
    },
    /// 预算耗尽仍未通过：保留最后候选与诊断节点。
    BudgetExhausted {
        last_code: NodeId,
        last_diagnostic: NodeId,
        attempts: u32,
    },
    /// LLM 调用失败：已 emit RawLlmNode（attempted→ 目标域）。
    Failed { raw_llm: NodeId, error: LlmError },
}

/// implement-loop 错误（区别于「LLM 调用失败」——后者走 [`ImplementLoopOutcome::Failed`]）。
#[derive(Debug, Error)]
pub enum ImplementLoopError {
    /// 写图失败（建节点 / 边）。
    #[error("写图失败：{0}")]
    Graph(#[from] GraphError),

    /// 注入的 code_check 诊断 kind 不是 CodeCheck。
    #[error("注入的 code_check 诊断 kind 必须为 CodeCheck，实际为 {0:?}")]
    WrongDiagnosticKind(DiagnosticKind),
}

impl From<LlmStepError> for ImplementLoopError {
    fn from(e: LlmStepError) -> Self {
        match e {
            LlmStepError::Graph(g) => ImplementLoopError::Graph(g),
        }
    }
}

/// 运行 implement-loop。
///
/// 参数：
/// - `prompts`：调用时刻的 prompt 提供者（§8.4）——implement / repair 步骤的请求都由它据
///   当前 active context 当场渲染（与各步 ContextSnapshot 同源）；
/// - `target`：`addresses→` 目标域（Objective | Milestone | FirstSlice）；
/// - `pseudocode`：被实现的 Pseudocode 节点（`implements→` 指向它）；
/// - `pseudocode_text`：该伪代码的正文（喂给 implement 提供者——图节点不存正文,
///   见 4.4.3）；
/// - `libraries`：design 阶段 LLM 选中的标准库名（喂给 implement / repair 提供者，按需注入库资产）；
/// - `check`：注入的确定性 code_check（kind 必须 CodeCheck）。
#[allow(clippy::too_many_arguments)]
pub async fn run_implement_loop<C, P, K>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    config: &ImplementLoopConfig,
    target: NodeId,
    pseudocode: NodeId,
    pseudocode_text: &str,
    libraries: &[String],
    mut check: K,
) -> Result<ImplementLoopOutcome, ImplementLoopError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
    K: CodeChecker,
{
    // 步骤 1：首次 implement（请求在调用时刻据 active context + 伪代码正文 + 所选库渲染）。
    let mut artifact = match implement_design(
        store,
        client,
        |ctx: &ActiveContext| prompts.implement(ctx, target, pseudocode_text, libraries),
        &config.structured,
        target,
        pseudocode,
    )
    .await?
    {
        LoopStepOutcome::Succeeded(a) => a,
        LoopStepOutcome::Failed { raw_llm, error } => {
            return Ok(ImplementLoopOutcome::Failed { raw_llm, error });
        }
    };

    let mut attempts: u32 = 1;
    let mut repairs_done: u32 = 0;

    loop {
        // 步骤 2：注入的 code_check + emit DiagnosticNode（checks→ Code）。
        let payload = check.check(&artifact.files);
        if payload.kind != DiagnosticKind::CodeCheck {
            return Err(ImplementLoopError::WrongDiagnosticKind(payload.kind));
        }
        let passed = payload.ok;
        let diagnostics = payload.diagnostics.clone();
        let diag_node = store
            .as_deterministic()
            .diagnostic(format!("code_check:attempt_{attempts}"), payload)?;
        store.append_edge(diag_node, artifact.node, EdgeKind::Checks)?;

        // 步骤 3：通过即返回。
        if passed {
            return Ok(ImplementLoopOutcome::Passed {
                code: artifact.node,
                files: artifact.files,
                attempts,
            });
        }

        // 步骤 5：预算耗尽。
        if repairs_done >= config.max_repair_attempts {
            return Ok(ImplementLoopOutcome::BudgetExhausted {
                last_code: artifact.node,
                last_diagnostic: diag_node,
                attempts,
            });
        }

        // 步骤 4：据诊断 repair（请求在调用时刻据 active context + 上一候选 + 诊断渲染）。
        let prev_code = artifact.node;
        let prev_files = artifact.files.clone();
        artifact = match repair_code(
            store,
            client,
            |ctx: &ActiveContext| prompts.repair(ctx, target, &prev_files, &diagnostics, libraries),
            &config.structured,
            target,
            prev_code,
        )
        .await?
        {
            LoopStepOutcome::Succeeded(a) => a,
            LoopStepOutcome::Failed { raw_llm, error } => {
                return Ok(ImplementLoopOutcome::Failed { raw_llm, error });
            }
        };
        repairs_done += 1;
        attempts += 1;
    }
}
