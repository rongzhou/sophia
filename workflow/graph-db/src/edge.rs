//! 边目录与硬约束。
//!
//! 见 docs/workflow_graph_spec.md 第六节。每种边只允许特定 `(from.role, to.role)`
//! 组合，`append_edge` 写入前校验（不变量 I3）。不在表中的组合一律拒绝。

use crate::ids::NodeRole;
use serde::{Deserialize, Serialize};

/// 边的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Supersedes,
    Decomposes,
    MemberOf,
    Groups,
    ConstrainedBy,
    Requires,
    Excludes,
    ValidatedBy,
    Targets,
    Assesses,
    Affects,
    Proposes,
    Accepts,
    Withdraws,
    Activates,
    Answers,
    AsksAbout,
    Consumed,
    Considers,
    Addresses,
    Revises,
    Implements,
    Repairs,
    Checks,
    Selects,
    Materializes,
    Attempted,
}

impl EdgeKind {
    /// 边类型名（用于诊断）。
    pub fn name(self) -> &'static str {
        use EdgeKind as E;
        match self {
            E::Supersedes => "supersedes",
            E::Decomposes => "decomposes",
            E::MemberOf => "member_of",
            E::Groups => "groups",
            E::ConstrainedBy => "constrained_by",
            E::Requires => "requires",
            E::Excludes => "excludes",
            E::ValidatedBy => "validated_by",
            E::Targets => "targets",
            E::Assesses => "assesses",
            E::Affects => "affects",
            E::Proposes => "proposes",
            E::Accepts => "accepts",
            E::Withdraws => "withdraws",
            E::Activates => "activates",
            E::Answers => "answers",
            E::AsksAbout => "asks_about",
            E::Consumed => "consumed",
            E::Considers => "considers",
            E::Addresses => "addresses",
            E::Revises => "revises",
            E::Implements => "implements",
            E::Repairs => "repairs",
            E::Checks => "checks",
            E::Selects => "selects",
            E::Materializes => "materializes",
            E::Attempted => "attempted",
        }
    }

    /// 校验 `(from_role, to_role, self)` 是否在允许集合中（第六节表）。
    ///
    /// `supersedes` 的「两端同 role」与不成环约束在存储层另行校验（见 store）。
    pub fn allows(self, from: NodeRole, to: NodeRole) -> bool {
        use EdgeKind as E;
        use NodeRole as R;
        match self {
            // supersedes：两端 role 相同（此处只查相等，链不成环在 store 层）。
            E::Supersedes => from == to,
            E::Decomposes => from == R::Objective && to == R::Decomposition,
            E::MemberOf => {
                matches!(from, R::Objective | R::Milestone)
                    && matches!(to, R::Decomposition | R::Milestone | R::FirstSlice)
            }
            E::Groups => matches!(from, R::Milestone | R::FirstSlice) && to == R::Objective,
            E::ConstrainedBy => {
                matches!(from, R::Objective | R::Milestone | R::FirstSlice) && to == R::Constraint
            }
            E::Requires => matches!(from, R::Milestone | R::FirstSlice) && to == R::Constraint,
            E::Excludes => matches!(from, R::Milestone | R::FirstSlice) && to == R::Constraint,
            E::ValidatedBy => {
                matches!(from, R::Objective | R::Milestone | R::FirstSlice)
                    && to == R::AcceptanceCriterion
            }
            E::Targets => {
                from == R::ChangeRequest
                    && matches!(to, R::Objective | R::Milestone | R::Constraint)
            }
            E::Assesses => from == R::Assessment && matches!(to, R::ChangeRequest | R::Objective),
            E::Affects => {
                from == R::Assessment && matches!(to, R::Objective | R::Milestone | R::Code)
            }
            E::Proposes => {
                from == R::Assessment && matches!(to, R::FirstSlice | R::Constraint | R::Decision)
            }
            E::Accepts => {
                from == R::AcceptanceEvent
                    && matches!(
                        to,
                        R::Objective
                            | R::Constraint
                            | R::Milestone
                            | R::FirstSlice
                            | R::Decomposition
                            | R::ChangeRequest
                            | R::AcceptanceCriterion
                    )
            }
            E::Withdraws => {
                from == R::WithdrawalEvent
                    && matches!(
                        to,
                        R::Objective
                            | R::Constraint
                            | R::Milestone
                            | R::FirstSlice
                            | R::Decomposition
                            | R::ChangeRequest
                            | R::AcceptanceCriterion
                            | R::Code
                    )
            }
            E::Activates => from == R::ActivationEvent && to == R::Milestone,
            E::Answers => from == R::Clarification && to == R::Clarification,
            E::AsksAbout => {
                from == R::Clarification
                    && matches!(to, R::Objective | R::Milestone | R::ChangeRequest)
            }
            E::Consumed => {
                matches!(
                    from,
                    R::Decision | R::Pseudocode | R::Code | R::Assessment | R::Decomposition
                ) && to == R::ContextSnapshot
            }
            E::Considers => {
                from == R::Decision
                    && matches!(to, R::Objective | R::Code | R::ChangeRequest | R::Milestone)
            }
            E::Addresses => {
                matches!(from, R::Pseudocode | R::Code)
                    && matches!(to, R::Objective | R::Milestone | R::FirstSlice)
            }
            E::Revises => from == R::Pseudocode && to == R::Pseudocode,
            E::Implements => from == R::Code && to == R::Pseudocode,
            E::Repairs => from == R::Code && to == R::Code,
            E::Checks => {
                from == R::Diagnostic
                    && matches!(to, R::Pseudocode | R::Code | R::Milestone | R::Constraint)
            }
            E::Selects => from == R::Selection && to == R::Code,
            E::Materializes => from == R::Materialize && to == R::Selection,
            // attempted：to 可为任意被尝试构造的目标节点。
            E::Attempted => from == R::RawLlm,
        }
    }
}

/// 一条边（from / to / kind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: crate::ids::NodeId,
    pub to: crate::ids::NodeId,
    pub kind: EdgeKind,
}
