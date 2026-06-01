//! 调用时刻的 prompt 提供者（见 docs/engineering_architecture.md §8.4）。
//!
//! Sophia 的根本设定：prompt 是 LLM 看到的全部世界，必须由**调用时刻**的 active context
//! 渲染（language_design.md §10.7/§10.8），且这正是 `consumed→ ContextSnapshot` 所快照、
//! 所审计的那一份。因此每个工作流 LLM 步骤的请求都在该步即将调用时、据当前图状态**当场
//! 渲染**——不预渲染、不跨轮复用。
//!
//! 分层（§3.3）：`engine` 不持有 prompt 模板，也不含 active-context→文本 的抽取逻辑——
//! 那是协调层（CLI / example）职责。`engine` 只定义本 trait 与回调时机，并在回调时把
//! "该步骤源自当前图状态的输入"交给提供者。

use sophia_graph_db::{ActiveContext, DiagnosticItem, NodeId};
use sophia_llm::CompletionRequest;

/// 调度器在每轮 decision 前据自身状态构造的进度视图。
///
/// 这是 **scheduler-local** 状态（不属 active context），但 decision 必须看到它才能在
/// design→implement→revise 间正确推进（例如"还没有伪代码 → 先 design"、"上次实现概念性
/// 失败 → 考虑 revise_design"）。把它显式建模、传给提供者，避免 decision prompt 因看不到
/// 进度而原地打转。
#[derive(Debug, Clone, Copy)]
pub struct GoalProgress {
    /// 当前是否已有可供实现的伪代码（design 已产出且未被否决）。
    pub has_pseudocode: bool,
    /// 上一次 implement 是否在预算内未通过 code_check（概念/局部错误已耗尽 repair 预算）。
    /// 为真时 decision 可考虑 `revise_design`（重写伪代码）而非再次 implement。
    pub last_implement_failed: bool,
    /// 已用决策轮数。
    pub decisions_used: u32,
    /// 决策轮数上限（剩余 = max_decisions - decisions_used）。
    pub max_decisions: u32,
    /// 已产出的伪代码版本数。
    pub pseudocode_versions: u32,
}

impl GoalProgress {
    /// 剩余决策轮数。
    pub fn remaining_decisions(&self) -> u32 {
        self.max_decisions.saturating_sub(self.decisions_used)
    }
}

/// 工作流步骤的 prompt 提供者。每个方法在对应步骤**即将调用 LLM** 时被回调，
/// 据传入的、源自当前图状态的输入**当场渲染** `CompletionRequest`。
///
/// 约定：所有方法收到的 `ctx` 与该步骤建立的 `ContextSnapshot` **同源**（调度器 / 编排件
/// 用同一份 active-context 计算结果既喂 snapshot 又喂本提供者），保证 prompt 与 snapshot
/// 一致（§10.7 复现保证）。schema 不在此返回——它是结构契约，由编排层按固定步骤经
/// `prompt::schema_for` 选取。
pub trait StepPrompts {
    /// 渲染 decision 步骤请求：据 active context + 进度 + 焦点选择下一步动作。
    fn decision(
        &self,
        ctx: &ActiveContext,
        focus: NodeId,
        progress: GoalProgress,
    ) -> CompletionRequest;

    /// 渲染 decompose 步骤请求：把过大的目标拆成若干子目标。`focus` 是被拆解的目标域。
    /// 产出结构（rationale + children[]）由 `decompose_result` schema 约束。
    fn decompose(&self, ctx: &ActiveContext, focus: NodeId) -> CompletionRequest;

    /// 渲染 design 步骤请求（语义伪代码阶段）。
    fn design(&self, ctx: &ActiveContext, focus: NodeId) -> CompletionRequest;

    /// 渲染 revise_design 步骤请求：据现有伪代码 + 概念性诊断重写伪代码（产出新版本）。
    /// `pseudocode` 是被修订的现有伪代码正文；`diagnostics` 是触发修订的概念性诊断。
    fn revise(
        &self,
        ctx: &ActiveContext,
        focus: NodeId,
        pseudocode: &str,
        diagnostics: &[DiagnosticItem],
    ) -> CompletionRequest;

    /// 渲染 implement 步骤请求。`pseudocode` 是**本轮 design 产出的伪代码正文**——由调度器
    /// / 编排件在运行时取得后传入（根除"静态请求拿不到伪代码"的缺陷）。`libraries` 是 design
    /// 阶段 LLM 从库目录选中的标准库名——implement 据此注入对应库的完整用法资产（S2，按需）。
    fn implement(
        &self,
        ctx: &ActiveContext,
        focus: NodeId,
        pseudocode: &str,
        libraries: &[String],
    ) -> CompletionRequest;

    /// 渲染 repair 步骤请求：据上一候选文件正文 + 结构化诊断。`libraries` 同 implement——
    /// 沿用 design 所选库（修复仍可能涉及库操作），注入对应库资产。
    fn repair(
        &self,
        ctx: &ActiveContext,
        focus: NodeId,
        files: &[(String, String)],
        diagnostics: &[DiagnosticItem],
        libraries: &[String],
    ) -> CompletionRequest;
}
