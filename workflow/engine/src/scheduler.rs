//! 工作流总调度器：据 DecisionNode 驱动 design → implement-loop 的 goal 推进
//! （构建顺序 step 13+，对应 CLI `sophia graph` 工作流族的核心循环）。
//!
//! 见 docs/language_design.md 10.1 / 10.8（动作选择与执行分离）/ 10.9（预算）、
//! docs/workflow_graph_spec.md 4.4.2（DecisionNode）/ 第七节接入点。
//!
//! 设计要点：
//! - **动作选择必须由 LLM 产生 DecisionNode**（design 10.8）：每轮先 `run_llm_step`
//!   取一个结构化 decision，emit `DecisionNode`（`considers→ 焦点` + `consumed→ snapshot`），
//!   再据 `selected_action` 分派执行。系统不替 LLM 选动作。
//! - **执行委派给已就绪的单步件**：`design_solution` → Pseudocode；`implement_design` /
//!   `repair_code` 经 [`crate::run_implement_loop`]（implement→check→repair 预算闭环）→
//!   可物化候选。
//! - **预算（design 10.9）**：调度器强制 `max_decisions`（顶层循环上限，对应 max_depth）、
//!   `max_pseudocode_versions`、`max_total_llm_nodes`；超限即 `Outcome::BudgetExhausted`。
//! - **物化是显式收尾**：`select` / `materialize` 这类不可逆写盘动作不在调度器内自动执行，
//!   而是把通过 gate 的候选交回调用方（`Outcome::CandidateReady`），由其调用
//!   [`crate::run_selection_materialize`]（design 10.10 唯一写 `domains/` 路径）。
//! - **更高层动作让位**：`decompose` / `backtrack` / `revise_design` / `needs_clarification`
//!   涉及拆解 / 回退 / 澄清的图操作语义超出本 spine，调度器记录 decision 后以
//!   `Outcome::Yielded` 交回调用方，**不擅自臆造语义**（单一路线：不在此塞入分支实现）。
//!
//! 分层纪律：确定性 code_check 由调用方注入（[`crate::CodeChecker`]），与
//! implement-loop / materialize 消费注入报告同构；调度器不自行运行 checker。
//! LLM 失败仍走 RawLlmNode 兜底，**不伪造成功**。

use sophia_graph_db::{
    ActiveContext, DecisionAction, DecisionPayload, EdgeKind, GraphError, GraphStore, NodeId,
    NodeRole,
};
use sophia_llm::{LlmClient, LlmError, StructuredConfig};
use thiserror::Error;

use crate::implement_loop::{run_implement_loop, CodeChecker, ImplementLoopConfig};
use crate::loop_steps::{design_solution, revise_design, LibrarySelectionPolicy, LoopStepOutcome};
use crate::prompts::{GoalProgress, StepPrompts};
use crate::step::{run_llm_step, step_schema, LlmStepError, LlmStepOutcome};
use crate::ImplementLoopOutcome;

/// 调度器预算（design 10.9 的顶层子集）。
#[derive(Debug, Clone)]
pub struct SchedulerBudget {
    /// 顶层 decision 轮数上限（对应 max_depth）。
    pub max_decisions: u32,
    /// 单 goal 伪代码版本上限（max_pseudocode_versions_per_goal）。
    pub max_pseudocode_versions: u32,
    /// 单 goal LLM 产物节点总数上限（max_total_nodes_per_goal 的 LLM 子集）。
    pub max_total_llm_nodes: u32,
    /// implement-loop 的内部预算。
    pub implement_loop: ImplementLoopConfig,
}

impl Default for SchedulerBudget {
    fn default() -> Self {
        SchedulerBudget {
            max_decisions: 6,
            max_pseudocode_versions: 3,
            max_total_llm_nodes: 40,
            implement_loop: ImplementLoopConfig::default(),
        }
    }
}

/// 调度结束的原因。
#[derive(Debug)]
pub enum Outcome {
    /// 产出一个通过 code_check 的可物化候选（交调用方 select/materialize）。
    CandidateReady {
        code: NodeId,
        files: Vec<(String, String)>,
        decisions: u32,
    },
    /// LLM 选择了超出本 spine 的高层动作（decompose / backtrack / revise_design /
    /// needs_clarification），记录 decision 后交回调用方处理。
    Yielded {
        decision: NodeId,
        action: DecisionAction,
        decisions: u32,
    },
    /// 预算耗尽（decision 轮数 / 伪代码版本 / 节点总数）。
    BudgetExhausted { reason: String, decisions: u32 },
    /// LLM 调用失败：已 emit RawLlmNode（attempted→ 焦点）。
    Failed { raw_llm: NodeId, error: LlmError },
}

/// 调度器错误（区别于 LLM 调用失败——后者走 [`Outcome::Failed`]）。
#[derive(Debug, Error)]
pub enum SchedulerError {
    /// 写图失败。
    #[error("写图失败：{0}")]
    Graph(#[from] GraphError),

    /// implement-loop 内部错误（保留 typed 变体，不 stringize）。
    #[error("implement-loop 错误：{0}")]
    ImplementLoop(#[from] crate::ImplementLoopError),

    /// 焦点不是目标推进循环允许的目标域。`considers→`（Decision 焦点）与
    /// `addresses→`（design/implement 目标域）的交集为 Objective | Milestone。
    #[error("焦点 {0} 不是目标推进循环允许的目标域（Objective | Milestone）")]
    InvalidFocus(NodeId),
}

impl From<LlmStepError> for SchedulerError {
    fn from(e: LlmStepError) -> Self {
        match e {
            LlmStepError::Graph(g) => SchedulerError::Graph(g),
        }
    }
}

/// 运行 goal 推进调度循环。
///
/// `focus` 是当前焦点目标域（Objective | Milestone）：DecisionNode 经 `considers→` 指向它，
/// design / implement 经 `addresses→` 指向它。`prompts` 是调用时刻的 prompt 提供者
/// （§8.4）——每步请求都据当前 active context 当场渲染。`check` 是注入的确定性 code_check。
///
/// 循环：decision → 分派。`design_solution` 建/更新当前 Pseudocode；`implement_design`
/// 用当前 Pseudocode 跑 implement-loop，通过即 `CandidateReady` 返回。每轮消耗一次
/// decision 预算。
pub async fn run_goal_loop<C, P, K>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    budget: &SchedulerBudget,
    library_policy: &LibrarySelectionPolicy,
    focus: NodeId,
    mut check: K,
) -> Result<Outcome, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
    K: CodeChecker,
{
    ensure_focus(store, focus)?;

    // `max_total_llm_nodes` 是本次 goal 推进预算，历史图状态不消耗本轮预算。
    let baseline_llm_nodes = count_llm_nodes(store);
    let mut decisions: u32 = 0;
    let mut pseudo_versions: u32 = 0;
    // 当前可用的 Pseudocode（design 产出，implement 消费）：节点 ID + 正文 + design 所选库。
    // 正文必须随节点一起持有——图节点不存正文（4.4.3），而 implement / revise 提供者需要它；
    // 所选库（design 阶段 LLM 决策）随之传给 implement / repair 按需注入库资产（S2）。
    let mut current_pseudocode: Option<(NodeId, String, Vec<String>)> = None;
    // 上一次 implement 是否在预算内未通过 code_check，及触发的诊断节点（供 revise_design）。
    let mut last_implement_failed = false;
    let mut last_diagnostic: Option<NodeId> = None;

    loop {
        // 预算门：任一上限触发即结束。
        if let Some(reason) = budget_exceeded(store, budget, decisions, baseline_llm_nodes) {
            return Ok(Outcome::BudgetExhausted { reason, decisions });
        }

        // 步骤 1：LLM 决策（动作选择必须由 LLM 产生）。请求据当前 active context + 进度渲染。
        let progress = GoalProgress {
            has_pseudocode: current_pseudocode.is_some(),
            last_implement_failed,
            decisions_used: decisions,
            max_decisions: budget.max_decisions,
            pseudocode_versions: pseudo_versions,
        };
        let (decision_node, action) = match make_decision(
            store,
            client,
            prompts,
            &budget.implement_loop.structured,
            focus,
            progress,
        )
        .await?
        {
            DecisionResult::Decided { node, action } => (node, action),
            DecisionResult::Failed { raw_llm, error } => {
                return Ok(Outcome::Failed { raw_llm, error });
            }
        };
        decisions += 1;

        // 步骤 2：据动作分派。`Dispatch` 表达「结束 / 更新状态后继续」。
        let dispatched = match action {
            DecisionAction::DesignSolution => {
                dispatch_design(
                    store,
                    client,
                    prompts,
                    budget,
                    library_policy,
                    focus,
                    decisions,
                    &mut pseudo_versions,
                )
                .await?
            }
            DecisionAction::ReviseDesign => {
                dispatch_revise(
                    store,
                    client,
                    prompts,
                    budget,
                    library_policy,
                    focus,
                    decisions,
                    current_pseudocode.as_ref(),
                    last_diagnostic,
                    decision_node,
                    action,
                    &mut pseudo_versions,
                )
                .await?
            }
            DecisionAction::ImplementDesign => {
                dispatch_implement(
                    store,
                    client,
                    prompts,
                    budget,
                    focus,
                    decisions,
                    current_pseudocode.as_ref(),
                    decision_node,
                    action,
                    &mut check,
                )
                .await?
            }
            DecisionAction::NeedsClarification => {
                // emit 一个 Clarification(Question) `asks_about→ 焦点`（真正落图，而非空让位），
                // 再交回调用方：自动循环中无人类回答，提问后只能 yield（design 4.3 / 第七节 1）。
                let question = store.as_llm().question(
                    "clarification:scheduler",
                    "调度器：当前信息不足以继续，需要澄清。",
                )?;
                store.append_edge(question, focus, EdgeKind::AsksAbout)?;
                Dispatch::Done(Outcome::Yielded {
                    decision: decision_node,
                    action,
                    decisions,
                })
            }
            // decompose / backtrack 等非线性图操作超出本 spine：记录 decision 后交回调用方，
            // 不在 spine 内臆造其树形语义（design 10.9）。
            DecisionAction::Decompose
            | DecisionAction::Backtrack
            | DecisionAction::RepairCode
            | DecisionAction::Select
            | DecisionAction::Materialize => Dispatch::Done(Outcome::Yielded {
                decision: decision_node,
                action,
                decisions,
            }),
        };
        match dispatched {
            Dispatch::Done(outcome) => return Ok(outcome),
            Dispatch::DesignedPseudocode {
                node,
                text,
                libraries,
            } => {
                current_pseudocode = Some((node, text, libraries));
                last_implement_failed = false;
                last_diagnostic = None;
            }
            Dispatch::ImplementExhausted {
                last_diagnostic: diag,
            } => {
                last_implement_failed = true;
                last_diagnostic = Some(diag);
            }
        }
    }
}

/// 一次动作分派的结果：要么结束循环（`Done`），要么更新调度器状态后继续。
enum Dispatch {
    /// 结束循环并返回最终结果。
    Done(Outcome),
    /// design / revise 产出了（新）伪代码版本，记为当前伪代码后继续。`libraries` 为 design
    /// 阶段 LLM 所选标准库（随伪代码传给 implement / repair）。
    DesignedPseudocode {
        node: NodeId,
        text: String,
        libraries: Vec<String>,
    },
    /// implement-loop 在预算内未通过 code_check：记录失败 + 最后的诊断节点，回到 decision
    /// 让 LLM 决定 revise_design（概念问题重写伪代码）/ 再试 / 放弃。
    ImplementExhausted { last_diagnostic: NodeId },
}

/// 预算门：decision 轮数 / LLM 节点总数任一上限触发返回原因说明（design 10.9 顶层子集）。
/// 伪代码版本上限在 design 分派内单独判定（仅该动作消费）。
fn budget_exceeded(
    store: &GraphStore,
    budget: &SchedulerBudget,
    decisions: u32,
    baseline_llm_nodes: u32,
) -> Option<String> {
    if decisions >= budget.max_decisions {
        return Some(format!("decision 轮数达上限 {}", budget.max_decisions));
    }
    let run_llm_nodes = count_llm_nodes(store).saturating_sub(baseline_llm_nodes);
    if run_llm_nodes >= budget.max_total_llm_nodes {
        return Some(format!(
            "本轮 LLM 节点总数达上限 {}",
            budget.max_total_llm_nodes
        ));
    }
    None
}

/// 分派 `design_solution`：受伪代码版本预算约束，成功则携带新建的 Pseudocode（节点 + 正文）。
#[allow(clippy::too_many_arguments)]
async fn dispatch_design<C, P>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    budget: &SchedulerBudget,
    library_policy: &LibrarySelectionPolicy,
    focus: NodeId,
    decisions: u32,
    pseudo_versions: &mut u32,
) -> Result<Dispatch, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
{
    if *pseudo_versions >= budget.max_pseudocode_versions {
        return Ok(Dispatch::Done(Outcome::BudgetExhausted {
            reason: format!("伪代码版本达上限 {}", budget.max_pseudocode_versions),
            decisions,
        }));
    }
    match design_solution(
        store,
        client,
        |ctx: &ActiveContext| prompts.design(ctx, focus),
        &budget.implement_loop.structured,
        library_policy,
        focus,
    )
    .await?
    {
        LoopStepOutcome::Succeeded(art) => {
            *pseudo_versions += 1;
            Ok(Dispatch::DesignedPseudocode {
                node: art.node,
                text: art.text,
                libraries: art.libraries,
            })
        }
        LoopStepOutcome::Failed { raw_llm, error } => {
            Ok(Dispatch::Done(Outcome::Failed { raw_llm, error }))
        }
    }
}

/// 分派 `implement_design`：用当前伪代码跑 implement-loop，通过即产出可物化候选。
#[allow(clippy::too_many_arguments)]
async fn dispatch_implement<C, P, K>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    budget: &SchedulerBudget,
    focus: NodeId,
    decisions: u32,
    current_pseudocode: Option<&(NodeId, String, Vec<String>)>,
    decision_node: NodeId,
    action: DecisionAction,
    check: &mut K,
) -> Result<Dispatch, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
    K: CodeChecker,
{
    let Some((pseudo, pseudo_text, libraries)) = current_pseudocode else {
        // 无伪代码可实现：本 spine 不臆造 design，交回调用方（让 LLM 下一轮先 design）。
        return Ok(Dispatch::Done(Outcome::Yielded {
            decision: decision_node,
            action,
            decisions,
        }));
    };
    let outcome = run_implement_loop(
        store,
        client,
        prompts,
        &budget.implement_loop,
        focus,
        *pseudo,
        pseudo_text,
        libraries,
        |files: &[(String, String)]| check.check(files),
    )
    .await?;
    let done = match outcome {
        ImplementLoopOutcome::Passed { code, files, .. } => Outcome::CandidateReady {
            code,
            files,
            decisions,
        },
        // 预算内未通过：不直接结束 goal 循环，而是回到 decision 让 LLM 决定 revise_design /
        // 再 implement / 放弃（revise 由此变得可达，design 10.8）。
        ImplementLoopOutcome::BudgetExhausted {
            last_diagnostic, ..
        } => return Ok(Dispatch::ImplementExhausted { last_diagnostic }),
        ImplementLoopOutcome::Failed { raw_llm, error } => Outcome::Failed { raw_llm, error },
    };
    Ok(Dispatch::Done(done))
}

/// 分派 `revise_design`：据现有伪代码 + 上次实现的概念性诊断重写伪代码，产出新版本
/// （`revises→` 旧版本）。受伪代码版本预算约束。无伪代码 / 无诊断可依据时让位。
#[allow(clippy::too_many_arguments)]
async fn dispatch_revise<C, P>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    budget: &SchedulerBudget,
    library_policy: &LibrarySelectionPolicy,
    focus: NodeId,
    decisions: u32,
    current_pseudocode: Option<&(NodeId, String, Vec<String>)>,
    last_diagnostic: Option<NodeId>,
    decision_node: NodeId,
    action: DecisionAction,
    pseudo_versions: &mut u32,
) -> Result<Dispatch, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
{
    // 需要有现有伪代码可修订；否则让位（应先 design）。
    let Some((prev_pseudo, prev_text, _prev_libs)) = current_pseudocode else {
        return Ok(Dispatch::Done(Outcome::Yielded {
            decision: decision_node,
            action,
            decisions,
        }));
    };
    // 伪代码版本预算（revise 产出新版本，同样计入）。
    if *pseudo_versions >= budget.max_pseudocode_versions {
        return Ok(Dispatch::Done(Outcome::BudgetExhausted {
            reason: format!("伪代码版本达上限 {}", budget.max_pseudocode_versions),
            decisions,
        }));
    }
    // 取触发修订的概念性诊断（若有）：从最后的 DiagnosticNode 读出诊断项。
    let diagnostics = last_diagnostic
        .and_then(|d| diagnostics_of(store, d))
        .unwrap_or_default();

    match revise_design(
        store,
        client,
        |ctx: &ActiveContext| prompts.revise(ctx, focus, prev_text, &diagnostics),
        &budget.implement_loop.structured,
        library_policy,
        focus,
        *prev_pseudo,
    )
    .await?
    {
        LoopStepOutcome::Succeeded(art) => {
            *pseudo_versions += 1;
            Ok(Dispatch::DesignedPseudocode {
                node: art.node,
                text: art.text,
                libraries: art.libraries,
            })
        }
        LoopStepOutcome::Failed { raw_llm, error } => {
            Ok(Dispatch::Done(Outcome::Failed { raw_llm, error }))
        }
    }
}

/// 从一个 DiagnosticNode 读出其诊断项（用于 revise 的概念性诊断输入）。
fn diagnostics_of(
    store: &GraphStore,
    node: NodeId,
) -> Option<Vec<sophia_graph_db::DiagnosticItem>> {
    match store.node(node).map(|n| &n.payload) {
        Some(sophia_graph_db::NodePayload::Diagnostic(d)) => Some(d.diagnostics.clone()),
        _ => None,
    }
}

/// 一次决策的结果。
enum DecisionResult {
    Decided {
        node: NodeId,
        action: DecisionAction,
    },
    Failed {
        raw_llm: NodeId,
        error: LlmError,
    },
}

/// 执行一次 LLM 决策：取结构化 `DecisionPayload`，emit DecisionNode（considers→ focus）。
/// 请求据调用时刻的 active context + 进度由提供者渲染（与 snapshot 同源）。
async fn make_decision<C, P>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    config: &StructuredConfig,
    focus: NodeId,
    progress: GoalProgress,
) -> Result<DecisionResult, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
{
    let outcome: LlmStepOutcome<DecisionPayload> = run_llm_step(
        store,
        client,
        |ctx: &ActiveContext| prompts.decision(ctx, focus, progress),
        &step_schema("decision"),
        config,
        focus,
        "decision",
    )
    .await?;
    match outcome {
        LlmStepOutcome::Succeeded { value, snapshot } => {
            let action = value.selected_action;
            let node = store.as_llm().decision("decision", value)?;
            store.append_edge(node, snapshot, EdgeKind::Consumed)?;
            store.append_edge(node, focus, EdgeKind::Considers)?;
            Ok(DecisionResult::Decided { node, action })
        }
        LlmStepOutcome::Failed { raw_llm, error } => Ok(DecisionResult::Failed { raw_llm, error }),
    }
}

/// 焦点必须是目标推进循环允许的目标域：`considers→`（Decision 焦点）与 `addresses→`
/// （design/implement 目标域）的交集为 Objective | Milestone。
fn ensure_focus(store: &GraphStore, focus: NodeId) -> Result<(), SchedulerError> {
    match store.role_of(focus) {
        Some(NodeRole::Objective | NodeRole::Milestone) => Ok(()),
        Some(_) | None => Err(SchedulerError::InvalidFocus(focus)),
    }
}

/// 统计 LLM-provenance 节点数（预算 max_total_nodes_per_goal 的 LLM 子集）。
fn count_llm_nodes(store: &GraphStore) -> u32 {
    store
        .nodes()
        .filter(|n| n.meta.provenance == sophia_graph_db::Provenance::Llm)
        .count() as u32
}
