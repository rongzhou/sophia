//! graph-db 集成测试共用工具。
//!
//! 各测试二进制只用到本模块的一个子集，故允许「未使用」（共享测试模块惯例）。
#![allow(dead_code)]

use sophia_graph_db::{ContextSnapshotPayload, GraphStore, NodeId};

/// 建一个最小合法 ContextSnapshot（deterministic），供需要 `consumed→` 锚点的测试复用。
pub fn snapshot(store: &mut GraphStore) -> NodeId {
    store
        .as_deterministic()
        .context_snapshot(
            "snap",
            ContextSnapshotPayload {
                schema_version: 1,
                snapshot: serde_json::json!({}),
                digest: "a".repeat(64),
            },
        )
        .unwrap()
}
