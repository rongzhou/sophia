//! 执行 Trace：Execution Graph 执行的**投影**（见 docs/language_implementation.md 9.4）。
//!
//! Trace 不是独立观测层，而是把解释器的实际执行映射回 Execution Graph IR 的节点与边：
//! 每进入一个 callable 记一条 span（携带 `ExecNodeId`）；若该次进入是被另一 callable
//! 调用触发的，则该 span 还携带触发它的调用边 `ExecEdgeId`（顶层入口无入边，为 `None`）。
//!
//! 这使 trace 数据可直接映射回图结构，支持"哪个节点最慢""哪条边触发了 fallback"这类
//! 查询，而非时间线上的字符串 span（9.4）。起步子集执行图退化为「每 callable 一节点 +
//! Control 调用边」，故 span 也退化为「每次 callable 进入一条」；并发 / await / fallback
//! 等更丰富的 span 语义随执行图边语义在后续阶段一并扩展。
//!
//! **确定性优先**：起步阶段 span 不记真实墙钟时长（`Instant`/`Duration` 不确定、破坏
//! 可复现快照），只记图结构投影（node_id / edge_id / 名称 / 结局）与**进入顺序**。真实
//! 计时属性能剖析，待引入时作为可选侧通道，不污染确定性核心。

use sophia_exec_ir::{ExecEdgeId, ExecNodeId};

/// 一次 callable 进入在执行图上的投影（9.4 `ExecutionSpan` 的起步子集形态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSpan {
    /// 进入顺序序号（从 0 起，深度优先调用序；确定性，替代墙钟时间线）。
    pub seq: u32,
    /// 执行图中被进入的节点。
    pub node_id: ExecNodeId,
    /// 触发本次进入的调用边；顶层入口无入边为 `None`（9.4）。
    pub edge_id: Option<ExecEdgeId>,
    /// 节点对应的 callable 名（冗余，便于人读 trace，不依赖图反查）。
    pub callable: String,
    /// 调用深度（顶层入口为 0，每层调用 +1）。
    pub depth: u32,
    /// 本次进入的结局（投影回图：正常返回 / 领域错误 raise）。
    pub outcome: SpanOutcome,
}

/// 一次 callable 进入的结局（trace 投影，不含返回值正文——值不进 trace，避免泄漏 / 膨胀）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanOutcome {
    /// 正常 `return`。
    Returned,
    /// `raise` 的领域错误。
    Raised,
}

/// 一次顶层执行收集到的全部 span（按进入顺序）。
///
/// 由 [`crate::Interpreter`] 在执行过程中追加；执行结束后作为图执行的完整投影返回。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    spans: Vec<ExecutionSpan>,
}

impl Trace {
    /// 空 trace。
    pub fn new() -> Self {
        Trace::default()
    }

    /// 全部 span（按进入顺序）。
    pub fn spans(&self) -> &[ExecutionSpan] {
        &self.spans
    }

    /// span 数（= 执行期间 callable 进入次数）。
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// 在 callable **进入时**开一条 span（pre-order：父先于子），返回其下标。
    ///
    /// 结局未知，先以 `Returned` 占位，待 callable 执行完由 [`Trace::close`] 改写。
    /// `seq` 等于开 span 的下标——因为 open 按进入顺序追加，故 `seq` 即深度优先进入序。
    pub(crate) fn open(
        &mut self,
        node_id: ExecNodeId,
        edge_id: Option<ExecEdgeId>,
        callable: impl Into<String>,
        depth: u32,
    ) -> usize {
        let seq = self.spans.len() as u32;
        self.spans.push(ExecutionSpan {
            seq,
            node_id,
            edge_id,
            callable: callable.into(),
            depth,
            outcome: SpanOutcome::Returned, // 占位，close 时改写为真实结局。
        });
        seq as usize
    }

    /// 在 callable 执行完时写回真实结局（与 [`Trace::open`] 返回的下标配对）。
    pub(crate) fn close(&mut self, idx: usize, outcome: SpanOutcome) {
        if let Some(span) = self.spans.get_mut(idx) {
            span.outcome = outcome;
        }
    }
}
