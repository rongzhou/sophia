//! 工作流编排：把 active context、LLM 调用与图节点写入串成一次「LLM step」。
//!
//! 见 docs/workflow_graph_spec.md 第七节接入点：
//! 1. **任何 LLM 调用前必须先创建 `ContextSnapshotNode`**；下游 LLM-provenance 节点
//!    必须 `consumed→` 该 snapshot（I6）；
//! 2. LLM 调用失败（后端不可用 / 解析失败 / 超重试验证失败）必须 emit `RawLlmNode`
//!    并通过 `attempted→` 边指向意图目标节点（4.4.8、7.3），**不伪造成功结果**。
//!
//! 本 crate 是 workflow 层协调者，依赖 graph-db + llm + prompt。它把「确定性建
//! snapshot」与「非确定 LLM 调用」的边界固化为单一代码路径，使任何 LLM 产物都可
//! 100% 复现其 context。

#![forbid(unsafe_code)]

mod code_check;
mod implement_loop;
mod loop_steps;
mod prompts;
mod scheduler;
mod select_materialize;
mod step;
mod traversal;

pub use code_check::{code_check, domain_of_path};
pub use implement_loop::{
    run_implement_loop, CodeChecker, ImplementLoopConfig, ImplementLoopError, ImplementLoopOutcome,
};
pub use loop_steps::{
    decompose_goal, design_solution, implement_design, repair_code, revise_design, CodeArtifact,
    DecompositionArtifact, LoopError, LoopStepOutcome, PseudocodeArtifact,
};
pub use prompts::{GoalProgress, StepPrompts};
pub use scheduler::{run_goal_loop, Outcome, SchedulerBudget, SchedulerError};
pub use select_materialize::{
    run_materialization, run_ranked_selection, run_selection, run_selection_materialize,
    RankedCandidate, RankedSelection, SelectMaterializeError, SelectMaterializeOutcome,
};
pub use sophia_materialize::{Score, ScoreWeights};
pub use step::{run_llm_step, LlmStepError, LlmStepOutcome};
pub use traversal::{
    run_goal_tree, AutoAcceptReviewer, DecompositionReviewer, GoalResolution, ReviewDecision,
    TreeBudget,
};
