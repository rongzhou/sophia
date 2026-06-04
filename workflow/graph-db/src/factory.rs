//! 节点工厂：按 provenance 创建路径分组的入口，强制 N6。
//!
//! 见 docs/workflow_graph_spec.md 1.2、第二节、N6。provenance 必须由**创建路径**
//! 强制，schema 自身不能伪造：
//! - `Human` 必须由 CLI 显式人类输入或 scenario 文件创建 → [`HumanFactory`]；
//! - `Llm` 必须经过 LLM 调用 helper → [`LlmFactory`]；
//! - `Deterministic` 必须由确定性 helper 生成 → [`DeterministicFactory`]。
//!
//! `GraphStore::append_node` 是 crate 内部原语；外部只能经这些工厂创建节点，
//! 因此调用方无法自由设定 provenance。每个工厂只暴露其 provenance 矩阵允许的 role。

use crate::edge::EdgeKind;
use crate::error::GraphResult;
use crate::ids::{NodeCreationStatus, NodeId, NodeRole, Provenance};
use crate::payload::*;
use crate::store::GraphStore;

/// 人类创建路径：provenance 固定为 `Human`。
///
/// 仅由 CLI 的人类输入或 scenario 文件加载调用，对应「人类授权事件 / 人类提出的
/// 目标与约束」。
pub struct HumanFactory<'s> {
    store: &'s mut GraphStore,
}

/// LLM 创建路径：provenance 固定为 `Llm`。
///
/// 仅由 LLM 调用 helper 在收到并校验模型输出后调用。
pub struct LlmFactory<'s> {
    store: &'s mut GraphStore,
}

/// 确定性创建路径：provenance 固定为 `Deterministic`。
///
/// 仅由确定性管线（检查器 / 选择 / 物化 / 快照）调用。
pub struct DeterministicFactory<'s> {
    store: &'s mut GraphStore,
}

impl GraphStore {
    /// 进入人类创建路径。
    pub fn as_human(&mut self) -> HumanFactory<'_> {
        HumanFactory { store: self }
    }

    /// 进入 LLM 创建路径。
    pub fn as_llm(&mut self) -> LlmFactory<'_> {
        LlmFactory { store: self }
    }

    /// 进入确定性创建路径。
    pub fn as_deterministic(&mut self) -> DeterministicFactory<'_> {
        DeterministicFactory { store: self }
    }
}

// ============ Human 路径 ============
//
// 矩阵允许 Human 的 role：Objective、Constraint、AcceptanceCriterion、Milestone、
// ChangeRequest、AcceptanceEvent、WithdrawalEvent、ActivationEvent、
// Clarification(Answer)。

impl HumanFactory<'_> {
    /// 人类提出的目标。
    pub fn objective(
        &mut self,
        summary: impl Into<String>,
        p: ObjectivePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Objective,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Objective(p),
        )
    }

    /// 人类提出的约束。
    pub fn constraint(
        &mut self,
        summary: impl Into<String>,
        p: ConstraintPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Constraint,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Constraint(p),
        )
    }

    /// 人类提出的验收条件。
    pub fn acceptance_criterion(
        &mut self,
        summary: impl Into<String>,
        p: AcceptanceCriterionPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::AcceptanceCriterion,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::AcceptanceCriterion(p),
        )
    }

    /// 人类创建的 milestone。
    pub fn milestone(
        &mut self,
        summary: impl Into<String>,
        p: MilestonePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Milestone,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Milestone(p),
        )
    }

    /// 人类提出的变更请求。
    pub fn change_request(
        &mut self,
        summary: impl Into<String>,
        p: ChangeRequestPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::ChangeRequest,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::ChangeRequest(p),
        )
    }

    /// 人类接受事件。
    pub fn acceptance_event(
        &mut self,
        summary: impl Into<String>,
        p: AcceptancePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::AcceptanceEvent,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Acceptance(p),
        )
    }

    /// 人类撤销事件。
    pub fn withdrawal_event(
        &mut self,
        summary: impl Into<String>,
        p: WithdrawalPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::WithdrawalEvent,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Withdrawal(p),
        )
    }

    /// 人类激活事件。
    pub fn activation_event(
        &mut self,
        summary: impl Into<String>,
        p: ActivationPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::ActivationEvent,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Activation(p),
        )
    }

    /// 人类对问题的回答（Clarification kind=Answer）。
    pub fn answer(
        &mut self,
        summary: impl Into<String>,
        body: impl Into<String>,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Clarification,
            Provenance::Human,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Clarification(ClarificationPayload {
                kind: ClarificationKind::Answer,
                body: body.into(),
            }),
        )
    }
}

// ============ LLM 路径 ============
//
// 矩阵允许 Llm 的 role：Objective、Constraint、AcceptanceCriterion、Decomposition、
// Milestone、Assessment、FirstSlice、Clarification(Question)、Decision、Pseudocode、
// Code、RawLlm。

impl LlmFactory<'_> {
    pub fn objective(
        &mut self,
        summary: impl Into<String>,
        p: ObjectivePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Objective,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Objective(p),
        )
    }

    pub fn constraint(
        &mut self,
        summary: impl Into<String>,
        p: ConstraintPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Constraint,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Constraint(p),
        )
    }

    pub fn acceptance_criterion(
        &mut self,
        summary: impl Into<String>,
        p: AcceptanceCriterionPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::AcceptanceCriterion,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::AcceptanceCriterion(p),
        )
    }

    pub fn decomposition(
        &mut self,
        summary: impl Into<String>,
        p: DecompositionPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Decomposition,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Decomposition(p),
        )
    }

    pub fn milestone(
        &mut self,
        summary: impl Into<String>,
        p: MilestonePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Milestone,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Milestone(p),
        )
    }

    pub fn assessment(
        &mut self,
        summary: impl Into<String>,
        p: AssessmentPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Assessment,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Assessment(p),
        )
    }

    pub fn first_slice(
        &mut self,
        summary: impl Into<String>,
        p: FirstSlicePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::FirstSlice,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::FirstSlice(p),
        )
    }

    /// LLM 提问（Clarification kind=Question）。
    pub fn question(
        &mut self,
        summary: impl Into<String>,
        body: impl Into<String>,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Clarification,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Clarification(ClarificationPayload {
                kind: ClarificationKind::Question,
                body: body.into(),
            }),
        )
    }

    pub fn question_with_edges(
        &mut self,
        summary: impl Into<String>,
        body: impl Into<String>,
        outgoing: &[(NodeId, EdgeKind)],
    ) -> GraphResult<NodeId> {
        self.store.append_node_with_outgoing_edges(
            NodeRole::Clarification,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Clarification(ClarificationPayload {
                kind: ClarificationKind::Question,
                body: body.into(),
            }),
            outgoing,
        )
    }

    pub fn decision(
        &mut self,
        summary: impl Into<String>,
        p: DecisionPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Decision,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Decision(p),
        )
    }

    pub fn decision_with_edges(
        &mut self,
        summary: impl Into<String>,
        p: DecisionPayload,
        outgoing: &[(NodeId, EdgeKind)],
    ) -> GraphResult<NodeId> {
        self.store.append_node_with_outgoing_edges(
            NodeRole::Decision,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Decision(p),
            outgoing,
        )
    }

    pub fn pseudocode(
        &mut self,
        summary: impl Into<String>,
        p: PseudocodePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Pseudocode,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Pseudocode(p),
        )
    }

    pub fn pseudocode_with_edges(
        &mut self,
        summary: impl Into<String>,
        p: PseudocodePayload,
        outgoing: &[(NodeId, EdgeKind)],
    ) -> GraphResult<NodeId> {
        self.store.append_node_with_outgoing_edges(
            NodeRole::Pseudocode,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Pseudocode(p),
            outgoing,
        )
    }

    pub fn code(&mut self, summary: impl Into<String>, p: CodePayload) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Code,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Code(p),
        )
    }

    pub fn code_with_edges(
        &mut self,
        summary: impl Into<String>,
        p: CodePayload,
        outgoing: &[(NodeId, EdgeKind)],
    ) -> GraphResult<NodeId> {
        self.store.append_node_with_outgoing_edges(
            NodeRole::Code,
            Provenance::Llm,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Code(p),
            outgoing,
        )
    }

    /// LLM 调用失败的兜底节点（强制 creation_status=Failed，I8）。
    pub fn raw_llm(&mut self, summary: impl Into<String>, p: RawLlmPayload) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::RawLlm,
            Provenance::Llm,
            NodeCreationStatus::Failed,
            summary,
            NodePayload::RawLlm(p),
        )
    }
}

// ============ Deterministic 路径 ============
//
// 矩阵允许 Deterministic 的 role：ContextSnapshot、Decision(baseline)、Diagnostic、
// Selection、Materialize。

impl DeterministicFactory<'_> {
    pub fn context_snapshot(
        &mut self,
        summary: impl Into<String>,
        p: ContextSnapshotPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::ContextSnapshot,
            Provenance::Deterministic,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::ContextSnapshot(p),
        )
    }

    /// 确定性 baseline 决策（provenance=Deterministic；与 LLM 决策区分）。
    pub fn baseline_decision(
        &mut self,
        summary: impl Into<String>,
        p: DecisionPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Decision,
            Provenance::Deterministic,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Decision(p),
        )
    }

    pub fn diagnostic(
        &mut self,
        summary: impl Into<String>,
        p: DiagnosticPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Diagnostic,
            Provenance::Deterministic,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Diagnostic(p),
        )
    }

    pub fn selection(
        &mut self,
        summary: impl Into<String>,
        p: SelectionPayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Selection,
            Provenance::Deterministic,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Selection(p),
        )
    }

    pub fn materialize(
        &mut self,
        summary: impl Into<String>,
        p: MaterializePayload,
    ) -> GraphResult<NodeId> {
        self.store.append_node(
            NodeRole::Materialize,
            Provenance::Deterministic,
            NodeCreationStatus::Ok,
            summary,
            NodePayload::Materialize(p),
        )
    }
}
