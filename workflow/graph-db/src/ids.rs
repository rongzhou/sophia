//! 标识与基础枚举。
//!
//! 见 docs/workflow_graph_spec.md 第一节。`NodeId`、`Provenance`、`NodeRole`、
//! `NodeCreationStatus` 是图 schema 的基础词汇。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 节点 ID。生成事件时分配；序列化为 `N{>=4 位零填充十进制}`（如 `N0001`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    /// 规范字符串形式 `N0001`。
    pub fn as_string(&self) -> String {
        format!("N{:04}", self.0)
    }

    /// 从规范字符串解析。
    pub fn parse(s: &str) -> Option<Self> {
        let digits = s.strip_prefix('N')?;
        if digits.len() < 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let id = NodeId(digits.parse::<u32>().ok()?);
        if id.0 == 0 || id.as_string() != s {
            return None;
        }
        Some(id)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

// NodeId 序列化为规范字符串，保证图产物稳定可读。
impl Serialize for NodeId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        NodeId::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("非法 NodeId `{s}`")))
    }
}

/// 节点内容的产生者（不可伪造，由创建路径强制）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Human,
    Llm,
    Deterministic,
}

/// 节点在本体中的角色（节点类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    Objective,
    Constraint,
    AcceptanceCriterion,
    Decomposition,
    Milestone,
    ChangeRequest,
    Assessment,
    FirstSlice,
    AcceptanceEvent,
    WithdrawalEvent,
    ActivationEvent,
    Clarification,
    ContextSnapshot,
    Decision,
    Pseudocode,
    Code,
    Diagnostic,
    Selection,
    Materialize,
    RawLlm,
}

/// 节点创建状态。`Failed` 仅 `RawLlm` 允许（不变量 I8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeCreationStatus {
    Ok,
    Failed,
}

impl Provenance {
    /// 该 provenance 是否被指定 role 接受（第二节 矩阵）。
    pub fn allowed_for(self, role: NodeRole) -> bool {
        use NodeRole as R;
        use Provenance as P;
        match role {
            R::Objective | R::Constraint | R::AcceptanceCriterion | R::Milestone => {
                matches!(self, P::Human | P::Llm)
            }
            R::Decomposition
            | R::Assessment
            | R::FirstSlice
            | R::Pseudocode
            | R::Code
            | R::RawLlm => self == P::Llm,
            R::ChangeRequest | R::AcceptanceEvent | R::WithdrawalEvent | R::ActivationEvent => {
                self == P::Human
            }
            // Clarification 由 kind 决定（question=Llm / answer=Human）；矩阵层只校验
            // 两者之一合法，精确的 kind↔provenance 由 payload 工厂强制。
            R::Clarification => matches!(self, P::Human | P::Llm),
            R::ContextSnapshot | R::Diagnostic | R::Selection | R::Materialize => {
                self == P::Deterministic
            }
            R::Decision => matches!(self, P::Llm | P::Deterministic),
        }
    }
}
