//! NodeMeta：所有节点共享的维度信息。
//!
//! 见 docs/workflow_graph_spec.md 1.2。`meta` 承载 provenance / role / versioning
//! 维度；`payload` 由 role 决定（见 [`crate::payload`]）。

use crate::ids::{NodeCreationStatus, NodeId, NodeRole, Provenance};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 节点元信息。所有字段对所有节点一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMeta {
    pub id: NodeId,
    pub role: NodeRole,
    pub provenance: Provenance,
    pub creation_status: NodeCreationStatus,
    pub created_at: DateTime<Utc>,
    /// 非空摘要。
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// provenance == Llm 时可选。
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_artifact: Option<String>,
    #[serde(default)]
    pub response_artifact: Option<String>,
}
