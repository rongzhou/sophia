//! Sophia-Core Execution Graph IR。
//!
//! 显式描述运行时执行结构（见 docs/language_implementation.md 第八节）：
//! execution DAG、task dependencies、awaits、retries、cancellation、scheduling、
//! checkpoints、concurrency boundaries。是 Semantic IR 与 Runtime 之间的桥梁。
//!
//! 边的类型是一等概念：`Data` / `Stream` / `Control` / `Conditional` / `Fallback`
//! （见 [`EdgeKind`]）。`schema of T` 类型不匹配触发 `Fallback`，而非 runtime panic。
//!
//! 起步子集（§16）无并发 / await / retry，执行图退化为「每 callable 一节点」；
//! 更丰富的调度与边语义随后续阶段在此扩展。

#![forbid(unsafe_code)]

mod edge;
mod graph;

pub use edge::EdgeKind;
pub use graph::{ExecEdge, ExecEdgeId, ExecGraph, ExecNode, ExecNodeId, ExecNodeKind};
