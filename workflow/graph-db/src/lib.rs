//! Development Graph 持久化层。
//!
//! 采用 SQLite + 事件溯源（见 docs/engineering_architecture.md 第六节、
//! docs/workflow_graph_spec.md）。[`GraphStore`] 强制 append-only 不变量：
//! - `update_node` 不暴露（本类型无任何节点 / 边修改 API，对应 N1 / N2 / I9）；
//! - `append_edge` 写入前校验 `(from.role, to.role, type)`（I3）；
//! - 节点创建走 provenance 分组工厂（[`HumanFactory`] / [`LlmFactory`] /
//!   [`DeterministicFactory`]），底层原语 `append_node` 为 crate 私有：provenance 由
//!   创建路径强制（N6），并校验 `(role, provenance)`（I2）与 `creation_status`（I8）；
//! - `supersedes` 校验链不成环、两端 role 相同（I4）；
//! - 悬空引用拒绝（I5）；
//! - I6（LLM-provenance 节点需 consumed→ ContextSnapshot 边）由 [`GraphStore::validate_i6`]
//!   作为整体不变量检查。
//!
//! 本 crate 属 workflow 层，异步边界在更上层（CLI 协调）；存储 API 本身同步。

#![forbid(unsafe_code)]

mod active_context;
mod assessment;
mod decomposition;
mod edge;
mod error;
mod event;
mod factory;
mod ids;
mod meta;
mod payload;
mod store;

pub use active_context::{
    derive_active_context, snapshot_payload, AcceptanceCriterionView, ActiveContext,
    ChangeRequestView, ClarificationView, ConstraintView, MilestoneView, ObjectiveView,
};
pub use assessment::{decompose_assessment, AssessmentNodes};
pub use decomposition::{build_decomposition, ChildGoal, DecompositionNodes};
pub use edge::{Edge, EdgeKind};
pub use error::{GraphError, GraphResult};
pub use factory::{DeterministicFactory, HumanFactory, LlmFactory};
pub use ids::{NodeCreationStatus, NodeId, NodeRole, Provenance};
pub use meta::NodeMeta;
pub use payload::*;
pub use store::{GraphStore, StoredNode};
