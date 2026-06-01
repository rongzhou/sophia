//! Execution Graph IR 的边类型枚举。
//!
//! 见 docs/language_implementation.md 8.2 节。边类型是一等概念，
//! 不是 retry / cancellation 语义的附属物。
//!
//! v0 起步子集只**产出** `Control`（callable 调用边，见 [`super::ExecGraph::from_model`]）；
//! `Data` / `Stream` / `Conditional` / `Fallback` 是设计文档（§8.2）定义的边语义词汇，
//! 随并发 / 流式 / 路由 / 兜底调度落地时启用——保留为完整的 IR 设计词汇表，非投机代码。

/// 执行图边的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// 纯控制流，不携带数据。v0 唯一产出的边类型（callable 调用边）。
    Control,
    /// 携带类型化数据。（设计预留，v0 不产出）
    Data,
    /// 流式传输（token-by-token）。（设计预留，v0 不产出）
    Stream,
    /// 带谓词的路由边。（设计预留，v0 不产出）
    Conditional,
    /// 节点执行失败时触发（含 `schema of T` 不匹配）。（设计预留，v0 不产出）
    Fallback,
}
