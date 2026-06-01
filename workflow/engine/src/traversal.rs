//! 目标树遍历层：在线性 spine（[`crate::run_goal_loop`]）之上驱动**非线性**目标树
//! （design 10.8 动作 6 `decompose` / 动作 7 `backtrack`，10.9 明确不塞进 spine）。
//!
//! # 分层动机
//!
//! spine 只推进**单个**目标域：decision → design → implement → 可物化候选；遇到
//! `decompose` / `backtrack` 这类树形 / 非线性图操作时它**让位**（`Outcome::Yielded`），
//! 不在 spine 内臆造其语义（单一路线，避免把调度器变成大杂烩）。本遍历层正是承接这两个
//! 让位动作的“图遍历层”：
//!
//! - **decompose**：spine 让位 `Decompose` 后，本层执行 [`crate::decompose_goal`]（LLM 拆解 + 确定性 `build_decomposition` 建 `Decomposition` + 子 `Objective` 子树），再**递归**把 spine 驱动到每个子目标（深度优先）；父目标的“工作”至此委派给子目标。
//! - **backtrack**：spine 让位 `Backtrack` 后，本层**放弃当前分支**（如实记录于 [`GoalResolution::Backtracked`]）；append-only 图保留被放弃的子树，**不伪造 `WithdrawalEvent`**（撤销是人类权威，N4），也不臆造“自动改道恢复”逻辑（那需要让父 spine 看到子失败状态，属后续增强，类比 implement-loop 把诊断喂回 revise）。
//!
//! # 子目标 binding 与人类授权检查点（design 5.3 / N4）
//!
//! decompose 产出的子 `Objective` 是 **LLM provenance、默认未绑定**（`is_bound` 仅对 human
//! provenance 隐式接受、或链上有 `AcceptanceEvent` 才成立）。按 design 5.3，子目标的 binding
//! 只有在**人类接受该 `Decomposition`** 后才沿 `member_of` 继承——也就是说，子目标进入
//! active context（从而下游 design / implement 的 prompt 能看到自己的目标）这件事，**前置于
//! 一次人类授权**。本遍历层不伪造该授权（N4：接受 / 撤销是人类权威），而是把它建模为一个
//! 注入的[审查回调][`DecompositionReviewer`]：decompose 落图后回调它，
//!
//! - 裁决 [`ReviewDecision::Accept`]：本层据此创建一个 human `AcceptanceEvent`
//!   `accepts→ Decomposition`（真实授权落图，binding 随即由 `derive_active_context` 沿
//!   `member_of` 继承到子目标），再递归推进子目标；
//! - 裁决 [`ReviewDecision::Reject`]：本层**不**递归子目标、**不**伪造 withdrawal，如实记于
//!   [`GoalResolution::DecompositionRejected`]。
//!
//! 这样既补齐了「子目标须经人类接受才获得 binding」的 5.3 语义，又把授权权威留在调用方
//! （CLI 真人 / e2e harness 充当人类 / 自动化策略），引擎自身不伪造授权。
//!
//! # 动作选择仍由 LLM 产生
//!
//! 本层不替 LLM 选择 decompose / backtrack——这两个动作仍来自 spine 内的 `DecisionNode`
//! （10.8）。本层只在 spine 让位后**执行**对应的树操作并递归，保持“动作选择 / 执行分离”。
//!
//! # 预算与终止
//!
//! [`TreeBudget`] 增设 `max_depth`（decompose 嵌套深度）与 `max_goals`（spine 调用总数），
//! 防止递归爆炸；每个目标的 spine 推进仍受其 [`SchedulerBudget`] 约束。

use sophia_graph_db::{
    AcceptanceDecision, AcceptancePayload, DecisionAction, EdgeKind, GraphStore, NodeId,
};
use sophia_llm::{LlmClient, LlmError};

use crate::decompose_goal;
use crate::loop_steps::{DecompositionArtifact, LoopStepOutcome};
use crate::prompts::StepPrompts;
use crate::scheduler::{run_goal_loop, Outcome, SchedulerBudget, SchedulerError};

/// 目标树遍历预算（在 [`SchedulerBudget`] 之上加树形约束）。
#[derive(Debug, Clone)]
pub struct TreeBudget {
    /// decompose 嵌套深度上限（根为深度 0；达上限仍让位 decompose → 记 `BudgetExhausted`）。
    pub max_depth: u32,
    /// 访问的目标总数上限（spine 调用次数；防子树爆炸）。
    pub max_goals: u32,
    /// 每个目标的 spine 推进预算。
    pub scheduler: SchedulerBudget,
}

impl Default for TreeBudget {
    fn default() -> Self {
        TreeBudget {
            max_depth: 3,
            max_goals: 16,
            scheduler: SchedulerBudget::default(),
        }
    }
}

/// 一个候选的轻量引用：`(焦点目标, Code 节点, 候选文件)`。
pub type CandidateRef<'a> = (NodeId, NodeId, &'a [(String, String)]);

/// 人类对一次 `Decomposition` 的审查裁决（design 5.3 / N4：接受是人类权威）。
#[derive(Debug, Clone)]
pub enum ReviewDecision {
    /// 接受拆解：本层据此建 human `AcceptanceEvent accepts→ Decomposition`，子目标随后
    /// 沿 `member_of` 继承 binding，再递归推进。`notes` 写入 `AcceptancePayload.notes`。
    Accept { notes: String },
    /// 拒绝拆解：不递归子目标、不伪造 withdrawal，如实记 [`GoalResolution::DecompositionRejected`]。
    Reject { reason: String },
}

/// 拆解审查者：在 decompose 落图后、递归子目标前被回调，代表**人类授权检查点**。
///
/// 见模块级文档「子目标 binding 与人类授权检查点」。`decomposition` 是新建的 Decomposition
/// 节点，`children` 是其子 `Objective` 节点（与 LLM 拆解顺序一致）；可据图当前状态裁决。
/// 引擎据裁决创建（或不创建）真实的 human `AcceptanceEvent`——**不在审查者之外伪造授权**。
pub trait DecompositionReviewer {
    /// 审查一次拆解。返回接受 / 拒绝。
    fn review(
        &mut self,
        store: &GraphStore,
        parent: NodeId,
        decomposition: NodeId,
        children: &[NodeId],
    ) -> ReviewDecision;
}

/// 自动接受所有拆解的审查者（用于测试 / 全自动策略：调用方自身充当人类授权）。
///
/// **诚实性说明**：这把「人类接受」自动化，但授权仍经真实 `AcceptanceEvent` 落图（非绕过
/// binding 谓词），故 active context 推导、I6、provenance 矩阵都按真实路径走。适用于
/// e2e harness（harness 即代表人类操作员）与无人值守策略；真人 CLI 应实现交互式审查者。
pub struct AutoAcceptReviewer;

impl DecompositionReviewer for AutoAcceptReviewer {
    fn review(
        &mut self,
        _store: &GraphStore,
        _parent: NodeId,
        _decomposition: NodeId,
        _children: &[NodeId],
    ) -> ReviewDecision {
        ReviewDecision::Accept {
            notes: "自动接受拆解（调用方代表人类授权）".to_string(),
        }
    }
}

/// 一个目标在树遍历中的归结结果（递归结构）。
#[derive(Debug)]
pub enum GoalResolution {
    /// 叶子目标推进到可物化候选（交调用方 select / materialize）。
    Candidate {
        focus: NodeId,
        code: NodeId,
        files: Vec<(String, String)>,
    },
    /// 目标被拆解：建了 Decomposition + 子目标，每个子目标各有归结。
    Decomposed {
        focus: NodeId,
        decomposition: NodeId,
        children: Vec<GoalResolution>,
    },
    /// 拆解被人类审查者拒绝（design 5.3 / N4）：未递归子目标、未伪造 withdrawal。
    /// Decomposition 与子目标节点已落图（append-only），但未获 binding（无 AcceptanceEvent）。
    DecompositionRejected {
        focus: NodeId,
        decomposition: NodeId,
        reason: String,
    },
    /// 目标经 `backtrack` 被放弃（分支留在图中，未伪造撤销）。
    Backtracked { focus: NodeId },
    /// 目标让位了本层也不处理的更高层动作（needs_clarification / select / materialize /
    /// repair_code）——交回调用方（与 spine 让位语义一致）。
    Yielded {
        focus: NodeId,
        decision: NodeId,
        action: DecisionAction,
    },
    /// 该目标预算耗尽（spine 预算，或 decompose 深度 / 目标总数达上限）。
    BudgetExhausted { focus: NodeId, reason: String },
    /// 该目标的某次 LLM 调用失败：已 emit RawLlmNode（attempted→ 焦点），递归到此短路。
    Failed {
        focus: NodeId,
        raw_llm: NodeId,
        error: LlmError,
    },
}

impl GoalResolution {
    /// 收集本归结（含子树）里所有可物化候选（深度优先、左到右顺序）。
    pub fn candidates(&self) -> Vec<CandidateRef<'_>> {
        let mut out = Vec::new();
        self.collect_candidates(&mut out);
        out
    }

    fn collect_candidates<'a>(&'a self, out: &mut Vec<CandidateRef<'a>>) {
        match self {
            GoalResolution::Candidate { focus, code, files } => {
                out.push((*focus, *code, files.as_slice()))
            }
            GoalResolution::Decomposed { children, .. } => {
                for c in children {
                    c.collect_candidates(out);
                }
            }
            _ => {}
        }
    }

    /// 整棵子树是否“完全归结”：每个叶子都产出了候选（无 backtrack / 拒绝 / yield / 预算 / 失败）。
    pub fn is_fully_resolved(&self) -> bool {
        match self {
            GoalResolution::Candidate { .. } => true,
            GoalResolution::Decomposed { children, .. } => {
                children.iter().all(|c| c.is_fully_resolved())
            }
            _ => false,
        }
    }
}

/// 在目标树上驱动 spine：从 `root_focus` 出发，深度优先处理 decompose 子树。
///
/// `root_focus` 必须是 spine 允许的目标域（Objective | Milestone）；子目标（decompose 产物）
/// 均为 Objective。`prompts` 是调用时刻 prompt 提供者（§8.4）；`reviewer` 是拆解的**人类授权
/// 检查点**（design 5.3 / N4，见模块文档）；`check` 是注入的确定性 code_check（按可变借用在
/// 多个目标间复用）。
///
/// 返回根目标的 [`GoalResolution`]（递归含全部子目标的归结）。LLM 调用失败会短路整棵树
/// 并返回 [`GoalResolution::Failed`]（不伪造成功）；图写入错误以 `Err(SchedulerError)` 返回。
pub async fn run_goal_tree<C, P, R, F>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    reviewer: &mut R,
    budget: &TreeBudget,
    root_focus: NodeId,
    mut check: F,
) -> Result<GoalResolution, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
    R: DecompositionReviewer + ?Sized,
    F: FnMut(&[(String, String)]) -> sophia_graph_db::DiagnosticPayload,
{
    let mut goals_visited: u32 = 0;
    drive_goal(
        store,
        client,
        prompts,
        reviewer,
        budget,
        root_focus,
        0,
        &mut goals_visited,
        &mut check,
    )
    .await
}

/// 递归驱动单个目标（深度优先）。`depth` 是当前 decompose 嵌套深度；`goals_visited` 是
/// 跨整棵树共享的目标计数（防爆炸）。返回该目标的归结。
///
/// async 递归需装箱（`Box::pin`）以使返回类型可命名。
#[allow(clippy::too_many_arguments)]
fn drive_goal<'a, C, P, R, F>(
    store: &'a mut GraphStore,
    client: &'a C,
    prompts: &'a P,
    reviewer: &'a mut R,
    budget: &'a TreeBudget,
    focus: NodeId,
    depth: u32,
    goals_visited: &'a mut u32,
    check: &'a mut F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<GoalResolution, SchedulerError>> + 'a>>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
    R: DecompositionReviewer + ?Sized,
    F: FnMut(&[(String, String)]) -> sophia_graph_db::DiagnosticPayload,
{
    Box::pin(async move {
        // 目标总数预算门。
        if *goals_visited >= budget.max_goals {
            return Ok(GoalResolution::BudgetExhausted {
                focus,
                reason: format!("目标总数达上限 {}", budget.max_goals),
            });
        }
        *goals_visited += 1;

        // 用线性 spine 推进当前目标。
        let outcome = run_goal_loop(
            store,
            client,
            prompts,
            &budget.scheduler,
            focus,
            &mut *check,
        )
        .await?;

        match outcome {
            Outcome::CandidateReady { code, files, .. } => {
                Ok(GoalResolution::Candidate { focus, code, files })
            }
            Outcome::BudgetExhausted { reason, .. } => {
                Ok(GoalResolution::BudgetExhausted { focus, reason })
            }
            Outcome::Failed { raw_llm, error } => Ok(GoalResolution::Failed {
                focus,
                raw_llm,
                error,
            }),
            Outcome::Yielded {
                decision, action, ..
            } => match action {
                DecisionAction::Decompose => {
                    drive_decompose(
                        store,
                        client,
                        prompts,
                        reviewer,
                        budget,
                        focus,
                        depth,
                        goals_visited,
                        check,
                    )
                    .await
                }
                DecisionAction::Backtrack => Ok(GoalResolution::Backtracked { focus }),
                // 其余高层动作本层也不处理：交回调用方（与 spine 让位一致）。
                _ => Ok(GoalResolution::Yielded {
                    focus,
                    decision,
                    action,
                }),
            },
        }
    })
}

/// 执行 decompose 并递归驱动每个子目标。
#[allow(clippy::too_many_arguments)]
async fn drive_decompose<C, P, R, F>(
    store: &mut GraphStore,
    client: &C,
    prompts: &P,
    reviewer: &mut R,
    budget: &TreeBudget,
    focus: NodeId,
    depth: u32,
    goals_visited: &mut u32,
    check: &mut F,
) -> Result<GoalResolution, SchedulerError>
where
    C: LlmClient + ?Sized,
    P: StepPrompts + ?Sized,
    R: DecompositionReviewer + ?Sized,
    F: FnMut(&[(String, String)]) -> sophia_graph_db::DiagnosticPayload,
{
    // 深度门：达上限则不再拆解（如实记 BudgetExhausted，不强行展开）。
    if depth >= budget.max_depth {
        return Ok(GoalResolution::BudgetExhausted {
            focus,
            reason: format!("decompose 深度达上限 {}", budget.max_depth),
        });
    }

    // 执行拆解动作（LLM 拆解结构 → 确定性 build_decomposition 落图）。
    let art = match decompose_goal(
        store,
        client,
        |ctx: &sophia_graph_db::ActiveContext| prompts.decompose(ctx, focus),
        &budget.scheduler.implement_loop.structured,
        focus,
    )
    .await?
    {
        LoopStepOutcome::Succeeded(art) => art,
        LoopStepOutcome::Failed { raw_llm, error } => {
            return Ok(GoalResolution::Failed {
                focus,
                raw_llm,
                error,
            })
        }
    };
    let DecompositionArtifact {
        decomposition,
        children,
    } = art;

    // 人类授权检查点（design 5.3 / N4）：子目标须经人类接受 Decomposition 才获得 binding，
    // 进而进入 active context。引擎不伪造授权，回调注入的审查者裁决。
    match reviewer.review(store, focus, decomposition, &children) {
        ReviewDecision::Accept { notes } => {
            // 真实落图一个 human AcceptanceEvent accepts→ Decomposition（非绕过 binding 谓词）。
            let event = store.as_human().acceptance_event(
                "accept:decomposition",
                AcceptancePayload {
                    decision: AcceptanceDecision::Accepted,
                    notes,
                },
            )?;
            store.append_edge(event, decomposition, EdgeKind::Accepts)?;
        }
        ReviewDecision::Reject { reason } => {
            // 拒绝：不递归、不伪造 withdrawal（撤销是人类权威，N4）。Decomposition 与子目标
            // 已 append 落图但未获 binding（无 AcceptanceEvent）。
            return Ok(GoalResolution::DecompositionRejected {
                focus,
                decomposition,
                reason,
            });
        }
    }

    // 深度优先递归每个子目标。任一子目标 LLM 失败即短路整棵树（不伪造其余子目标成功）。
    let mut child_resolutions = Vec::with_capacity(children.len());
    for child in children {
        let res = drive_goal(
            store,
            client,
            prompts,
            reviewer,
            budget,
            child,
            depth + 1,
            goals_visited,
            check,
        )
        .await?;
        let is_failed = matches!(res, GoalResolution::Failed { .. });
        child_resolutions.push(res);
        if is_failed {
            break;
        }
    }

    Ok(GoalResolution::Decomposed {
        focus,
        decomposition,
        children: child_resolutions,
    })
}
