//! Selection / Materialize 节点编排（构建顺序 step 14 配套）。
//!
//! 见 docs/workflow_graph_spec.md 4.4.6 / 4.4.7 / 第六节、docs/language_design.md 10.10。
//!
//! `tools/materialize` 的类型状态链（`CodeCandidate<Selected>`）已在**编译期**保证
//! 候选必经全部 gate（check → audit → artifact_diff + runtime validation → select）。
//! 本模块是 workflow 层编排：把「已通过 gate 的候选」落为图上的两个确定性节点 + 边：
//!
//! - `SelectionNode`（provenance=Deterministic）`selects→ Code`：选中候选；
//! - `MaterializeNode`（provenance=Deterministic）`materializes→ Selection`：物化意图/事件锚点。
//!
//! 分层纪律：`tools/materialize` 不依赖 workflow 图；MaterializeNode 由本编排层在
//! gate 通过（即拿到 `CodeCandidate<Selected>`）后单独创建。编排层先写图上的物化锚点，再执行
//! 文件写入，避免文件系统已经改变但图上完全无记录。物化的原子写入仍由 materialize crate 负责。

use sophia_graph_db::{
    EdgeKind, GraphError, GraphStore, MaterializePayload, NodeId, NodeRole, SelectionPayload,
};
use sophia_materialize::{
    rank_candidates, CodeCandidate, MaterializeError, MaterializeOutcome, Score, ScoreInputs,
    ScoreWeights, Selected,
};
use std::path::Path;
use thiserror::Error;

/// Selection / Materialize 编排错误。
#[derive(Debug, Error)]
pub enum SelectMaterializeError {
    /// 写图失败（建 Selection / Materialize 节点或边）。
    #[error("写图失败：{0}")]
    Graph(#[from] GraphError),

    /// 原子物化写入失败。
    #[error("物化失败：{0}")]
    Materialize(#[from] MaterializeError),

    /// `selects→` 的目标节点不是 `Code`（编排前置校验）。
    #[error("selects 目标 {0} 不是 Code 节点")]
    NotCodeNode(NodeId),

    /// `materializes→` 的目标节点不是 `Selection`（编排前置校验）。
    #[error("materializes 目标 {0} 不是 Selection 节点")]
    NotSelectionNode(NodeId),

    /// 多候选选择时未提供任何候选。
    #[error("多候选选择：候选集合为空")]
    NoCandidates,
}

/// 编排产物：新建的 Selection / Materialize 节点与物化结果。
#[derive(Debug, Clone)]
pub struct SelectMaterializeOutcome {
    /// 新建的 SelectionNode。
    pub selection: NodeId,
    /// 新建的 MaterializeNode。
    pub materialize: NodeId,
    /// 原子写入的结果（写入根目录与文件列表）。
    pub written: MaterializeOutcome,
}

/// 创建 SelectionNode 并连 `selects→ Code`。
///
/// 入参 `candidate: &CodeCandidate<Selected>` 是**类型层 gate 通过证明**——只有经
/// check → audit → artifact_diff + runtime validation → select 全部门禁的候选才能
/// 构造出 `Selected` 状态。本函数据此创建确定性 `SelectionNode`（4.4.6）。
///
/// 该证明无法跨进程持久化：CLI 的 `select` / `materialize` 分属不同进程，各自用注入的
/// `GateReport` 重新构造 `Selected` 候选（重跑门禁，对不可逆写盘是更稳妥的姿态）。
pub fn run_selection(
    store: &mut GraphStore,
    candidate: &CodeCandidate<Selected>,
    code_node: NodeId,
    rationale: impl Into<String>,
) -> Result<NodeId, SelectMaterializeError> {
    if store.role_of(code_node) != Some(NodeRole::Code) {
        return Err(SelectMaterializeError::NotCodeNode(code_node));
    }
    // 触碰 candidate 以体现「证明被消费」（file_paths 不改变状态）。
    let _ = candidate.file_paths();

    let selection = store.as_deterministic().selection(
        "selection",
        SelectionPayload {
            rationale: rationale.into(),
        },
    )?;
    store.append_edge(selection, code_node, EdgeKind::Selects)?;
    Ok(selection)
}

/// 一个参与多候选排名的候选：Code 节点 + 通过 gate 的候选证明 + 评分信号。
///
/// `compile_pass` / `tests_pass` / `constraints_pass` 是来自各 gate 报告的**真实信号**；
/// `files` 是候选源码（用于结构性维度）；`pseudocode_clarity` 关乎伪代码，无信号则 `None`。
pub struct RankedCandidate {
    /// 该候选的 Code 节点（胜出后 `selects→` 指向它）。
    pub code_node: NodeId,
    /// 类型层 gate 通过证明（只有它能被选中 / 物化）。
    pub candidate: CodeCandidate<Selected>,
    /// 候选源码（path → content），用于评分的结构性维度。
    pub files: Vec<(String, String)>,
    pub compile_pass: bool,
    pub tests_pass: bool,
    pub constraints_pass: bool,
    pub pseudocode_clarity: Option<f64>,
}

/// 多候选选择产物：胜出候选 + 其 SelectionNode + 全部候选的评分（按排名）。
#[derive(Debug)]
pub struct RankedSelection {
    /// 胜出候选在入参中的原始下标。
    pub winner_index: usize,
    /// 为胜出候选建立的 SelectionNode。
    pub selection: NodeId,
    /// 胜出候选的 gate 证明（交回调用方做 materialize）。
    pub candidate: CodeCandidate<Selected>,
    /// 全部候选的排名：`(原始下标, Score)`，高分在前（确定性平局打破）。
    pub ranking: Vec<(usize, Score)>,
}

/// 在多个**已通过全部 gate** 的候选间按评分（design 10.9）选出最优者并建 SelectionNode。
///
/// 评分是确定性**内存启发式**，不入图（spec 无 `Score` role）：只用排名选出 winner，
/// 再为 winner 建一个 `SelectionNode { rationale }`（rationale 记录评分摘要，可审计）。
/// 单候选时退化为直接选中（与 [`run_selection`] 等价的结果，但带评分理由）。
///
/// `weights` 为 `None` 时用默认权重（`ScoreWeights::default`）。
pub fn run_ranked_selection(
    store: &mut GraphStore,
    candidates: Vec<RankedCandidate>,
    weights: Option<ScoreWeights>,
) -> Result<RankedSelection, SelectMaterializeError> {
    if candidates.is_empty() {
        return Err(SelectMaterializeError::NoCandidates);
    }
    // 每个候选的 code_node 必须是 Code 节点。
    for c in &candidates {
        if store.role_of(c.code_node) != Some(NodeRole::Code) {
            return Err(SelectMaterializeError::NotCodeNode(c.code_node));
        }
    }

    let weights = weights.unwrap_or_default();
    // 构造评分输入（借用各候选的 files）。
    let inputs: Vec<ScoreInputs> = candidates
        .iter()
        .map(|c| ScoreInputs {
            compile_pass: c.compile_pass,
            tests_pass: c.tests_pass,
            constraints_pass: c.constraints_pass,
            files: &c.files,
            pseudocode_clarity: c.pseudocode_clarity,
        })
        .collect();
    let ranking = rank_candidates(&inputs, &weights);
    // rank_candidates 对非空输入必返回非空且首项为最优。
    let (winner_index, winner_score) = ranking[0];

    // 取出 winner 候选（move 出 vec）。
    let mut candidates = candidates;
    let winner = candidates.swap_remove(winner_index);
    let rationale = format!(
        "多候选评分选中：overall={:.3}（compile={:.0}, tests={:.0}, constraints={:.0}, \
         simplicity={:.2}, locality={:.2}, cap_min={:.2}, pseudo_clarity={:.2}），共 {} 个候选",
        winner_score.overall,
        winner_score.compile,
        winner_score.tests,
        winner_score.constraints,
        winner_score.simplicity,
        winner_score.locality,
        winner_score.capability_minimality,
        winner_score.pseudocode_clarity,
        ranking.len()
    );
    let selection = run_selection(store, &winner.candidate, winner.code_node, rationale)?;
    Ok(RankedSelection {
        winner_index,
        selection,
        candidate: winner.candidate,
        ranking,
    })
}

/// 物化一个已选中的候选：原子写盘 + 建 `MaterializeNode`（`materializes→ Selection`）。
///
/// 入参 `candidate: CodeCandidate<Selected>` 同样是 gate 通过证明。`selection` 必须是
/// 一个 `selects→ Code` 的 SelectionNode（前置校验）。
///
/// 流程：原子物化 → 建 MaterializeNode（payload 记 `target_root_label` 与写入的相对
/// 文件列表）→ 连 `materializes→ Selection`。
pub fn run_materialization(
    store: &mut GraphStore,
    candidate: CodeCandidate<Selected>,
    selection: NodeId,
    write_root: &Path,
    target_root_label: impl Into<String>,
) -> Result<(NodeId, MaterializeOutcome), SelectMaterializeError> {
    if store.role_of(selection) != Some(NodeRole::Selection) {
        return Err(SelectMaterializeError::NotSelectionNode(selection));
    }

    let target_root = target_root_label.into();
    let files = candidate.file_paths();
    let materialize = store
        .as_deterministic()
        .materialize("materialize", MaterializePayload { target_root, files })?;
    store.append_edge(materialize, selection, EdgeKind::Materializes)?;

    // 图上已有物化锚点后再写文件，避免不可逆文件写入缺少审计记录。
    let written = candidate.materialize(write_root)?;
    Ok((materialize, written))
}

/// 把一份已通过全部 gate 的候选选中并物化，落为图节点与边。
///
/// 流程（确定性，单一路径）：
/// 1. 建 `SelectionNode`，连 `selects→ code_node`（[`run_selection`]）；
/// 2. 原子物化 + 建 `MaterializeNode`，连 `materializes→ Selection`（[`run_materialization`]）。
///
/// 参数：
/// - `code_node`：被选中的 `Code` 节点（`selects→` 指向它）；
/// - `write_root`：物化写入的文件系统根目录；
/// - `target_root_label`：MaterializeNode payload 记录的逻辑根（如 `"domains"`，
///   不写机器相关的绝对路径，保持产物确定性）；
/// - `selection_rationale`：选择理由（SelectionNode payload，非空）。
///
/// 候选已是 `CodeCandidate<Selected>`——类型层已保证它通过 check / audit /
/// artifact_diff / runtime validation 全部 gate，本函数不重复检查。
pub fn run_selection_materialize(
    store: &mut GraphStore,
    candidate: CodeCandidate<Selected>,
    code_node: NodeId,
    write_root: &Path,
    target_root_label: impl Into<String>,
    selection_rationale: impl Into<String>,
) -> Result<SelectMaterializeOutcome, SelectMaterializeError> {
    let target_root_label = target_root_label.into();
    prevalidate_materialize_payload(&candidate, &target_root_label)?;
    let selection = run_selection(store, &candidate, code_node, selection_rationale)?;
    let (materialize, written) =
        run_materialization(store, candidate, selection, write_root, target_root_label)?;
    Ok(SelectMaterializeOutcome {
        selection,
        materialize,
        written,
    })
}

fn prevalidate_materialize_payload(
    candidate: &CodeCandidate<Selected>,
    target_root: &str,
) -> Result<(), SelectMaterializeError> {
    if target_root.trim().is_empty() {
        return Err(GraphError::InvalidPayload("target_root 不能为空".into()).into());
    }
    if candidate.file_paths().is_empty() {
        return Err(GraphError::InvalidPayload("Materialize.files 不能为空".into()).into());
    }
    Ok(())
}
