//! GraphStore 错误类型。
//!
//! 工作流诊断携带节点 ID 与图上下文（见 docs/language_implementation.md 14.2），
//! 与编译器诊断（携带 span）严格分离。

use crate::ids::{NodeId, NodeRole, Provenance};
use thiserror::Error;

/// graph-db 层结果别名。
pub type GraphResult<T> = Result<T, GraphError>;

/// graph-db 层错误。
#[derive(Debug, Error)]
pub enum GraphError {
    /// `(role, provenance)` 不在第二节矩阵中（不变量 I2）。
    #[error("provenance {provenance:?} 不被 role {role:?} 接受")]
    ProvenanceNotAllowed {
        role: NodeRole,
        provenance: Provenance,
    },

    /// `creation_status == Failed` 出现在非 RawLlm 节点上（不变量 I8）。
    #[error("creation_status=Failed 仅 RawLlm 允许，但出现在 {role:?}")]
    InvalidFailedStatus { role: NodeRole },

    /// 边的 `(from.role, to.role, type)` 不在第六节允许集合中（不变量 I3）。
    #[error("非法边 {edge}: {from_role:?} → {to_role:?}")]
    InvalidEdge {
        edge: &'static str,
        from_role: NodeRole,
        to_role: NodeRole,
    },

    /// 指向的节点不存在（不变量 I5，悬空引用）。
    #[error("边引用了不存在的节点 {0}")]
    DanglingReference(NodeId),

    /// `supersedes` 两端 role 不同，或成环（不变量 I4）。
    #[error("非法 supersedes：{0}")]
    InvalidSupersedes(String),

    /// `summary` 为空（NodeMeta 约束）。
    #[error("节点 summary 不能为空")]
    EmptySummary,

    /// payload 字段约束被违反（如必填字段为空）。
    #[error("payload 约束被违反：{0}")]
    InvalidPayload(String),

    /// 序列化 / 反序列化失败。
    #[error("序列化失败：{0}")]
    Serialization(String),

    /// SQLite 后端错误。
    #[error("存储后端错误：{0}")]
    Backend(String),
}

impl From<rusqlite::Error> for GraphError {
    fn from(e: rusqlite::Error) -> Self {
        GraphError::Backend(e.to_string())
    }
}

impl From<serde_json::Error> for GraphError {
    fn from(e: serde_json::Error) -> Self {
        GraphError::Serialization(e.to_string())
    }
}
