//! 事件溯源模型。
//!
//! 见 docs/engineering_architecture.md 6.2。Development Graph 的 append-only、
//! 节点不可变语义天然对应事件溯源：每个事件 append-only 写入 SQLite 的
//! `graph_events` 表；当前图状态由事件流 replay 得出。
//!
//! 与设计示例相比，本实现把「节点创建」与「边新增」作为两类核心事件；状态变更
//! （N3）通过新增 successor 节点 + `supersedes` 边表达，因此不需要独立的状态变更
//! 事件——`StatusChanged` 等高层语义由查询层在节点 / 边之上推导。

use crate::edge::Edge;
use crate::meta::NodeMeta;
use crate::payload::NodePayload;
use serde::{Deserialize, Serialize};

/// append-only 图事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GraphEvent {
    /// 创建节点（meta + payload）。
    ///
    /// payload 装箱以平衡枚举变体大小（meta + payload 远大于一条边）。
    NodeCreated {
        meta: Box<NodeMeta>,
        payload: Box<NodePayload>,
    },
    /// 新增边。
    EdgeAdded { edge: Edge },
}
