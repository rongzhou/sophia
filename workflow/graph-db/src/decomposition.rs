//! 目标拆解的图构造（workflow_graph_spec 4.1.4、5.3）。
//!
//! 与 `assessment::decompose_assessment`（变更评估拆解）不同，本模块处理**目标树拆解**：
//! 当一个 `Objective` 过大时，LLM 决定 `decompose`（design 10.8 动作 6），把它拆成若干
//! 子目标。本确定性 helper 把「LLM 给出的拆解结构」落为图节点与边：
//!
//! - 1 个 `Decomposition` 节点（承载 rationale + proposed_count）；`consumed→ ContextSnapshot`；
//! - `parent_objective decomposes→ Decomposition`（父目标提出此次拆解，6.1 表）；
//! - 每个子目标建一个 `Objective`（LLM provenance），`child member_of→ Decomposition`（成员关系）。
//!
//! **provenance 与 I6**：`Decomposition` 是 decompose 动作的**执行产物节点**（与 design 的
//! `Pseudocode`、implement 的 `Code`、评估的 `Assessment` 同属 LLM 执行输出），故它本身必须
//! `consumed→ ContextSnapshot`（I6）——拆解**结构**（rationale + children）是 LLM 生成内容，
//! 其可复现性锚定在「产出这次拆解的那一次 LLM 调用」的 snapshot 上，而非触发它的 DecisionNode
//! （后者是另一次"该不该拆"的调用，§10.8 动作选择与执行分离）。子 `Objective` 是**结构性
//! 派生节点**（类比 assessment 协议里的 FirstSlice / Constraint），经 `member_of→ Decomposition`
//! 间接锚定，**不**单独携带 `consumed→` 边（边目录也不允许 Objective consumed→）。
//!
//! **binding 不在此处伪造**：LLM 派生的子目标默认**未绑定**（`is_bound` 仅对 human
//! provenance 隐式接受，或链上有 `AcceptanceEvent`）。本 helper 不擅自创建人类授权事件来
//! 强制 binding（撤销 / 接受是人类权威，N4）。遍历层可在结构上推进未绑定子目标（spine 的
//! `ensure_focus` 只校验 role，不要求 bound）；binding 继承（5.3）在人类接受该 Decomposition
//! 后由 `derive_active_context` 自动沿 `member_of` 传播。

use crate::edge::EdgeKind;
use crate::error::{GraphError, GraphResult};
use crate::ids::{NodeId, NodeRole};
use crate::payload::{DecompositionPayload, ObjectivePayload};
use crate::store::GraphStore;

/// 一次目标拆解新建的全部节点。
#[derive(Debug, Clone)]
pub struct DecompositionNodes {
    /// 新建的 Decomposition 节点。
    pub decomposition: NodeId,
    /// 子目标 Objective 节点（与入参 `children` 顺序一致）。
    pub children: Vec<NodeId>,
}

/// 一个待建子目标：标题 + 描述。
#[derive(Debug, Clone)]
pub struct ChildGoal {
    pub title: String,
    pub description: String,
}

/// 把一次目标拆解落为图节点与边（确定性，给定 LLM 拆解结构）。
///
/// 参数：
/// - `parent`：被拆解的 `Objective`（`decomposes→ Decomposition`）；非 Objective 即报错；
/// - `snapshot`：产出本次拆解的 LLM 调用所建的 `ContextSnapshot`（`Decomposition consumed→`
///   它，满足 I6）；非 ContextSnapshot 即报错；
/// - `rationale`：拆解理由（写入 `DecompositionPayload.rationale`，非空）；
/// - `children`：子目标列表（至少 2 个——拆成 1 个无意义）。
///
/// 返回新建的 Decomposition 与子目标节点 ID。
pub fn build_decomposition(
    store: &mut GraphStore,
    parent: NodeId,
    snapshot: NodeId,
    rationale: impl Into<String>,
    children: &[ChildGoal],
) -> GraphResult<DecompositionNodes> {
    let rationale = rationale.into();
    prevalidate_decomposition(store, parent, snapshot, &rationale, children)?;

    let decomposition = store.as_llm().decomposition(
        "decomposition",
        DecompositionPayload {
            rationale,
            proposed_count: children.len() as u32,
        },
    )?;
    // Decomposition 是 decompose 的执行产物，必须 consumed→ 本次调用的 snapshot（I6）。
    store.append_edge(decomposition, snapshot, EdgeKind::Consumed)?;
    // parent_objective decomposes→ Decomposition（6.1：from=Objective, to=Decomposition）。
    store.append_edge(parent, decomposition, EdgeKind::Decomposes)?;

    let mut child_ids = Vec::with_capacity(children.len());
    for child in children {
        let id = store.as_llm().objective(
            format!("subgoal:{}", child.title),
            ObjectivePayload {
                title: child.title.clone(),
                description: child.description.clone(),
            },
        )?;
        // child member_of→ Decomposition（成员关系；binding 沿此边继承，5.3）。
        store.append_edge(id, decomposition, EdgeKind::MemberOf)?;
        child_ids.push(id);
    }

    Ok(DecompositionNodes {
        decomposition,
        children: child_ids,
    })
}

fn prevalidate_decomposition(
    store: &GraphStore,
    parent: NodeId,
    snapshot: NodeId,
    rationale: &str,
    children: &[ChildGoal],
) -> GraphResult<()> {
    if store.role_of(parent) != Some(NodeRole::Objective) {
        return Err(GraphError::InvalidPayload(format!(
            "{parent} 不是 Objective，无法作为 decomposes→ 的父目标"
        )));
    }
    if store.role_of(snapshot) != Some(NodeRole::ContextSnapshot) {
        return Err(GraphError::InvalidPayload(format!(
            "{snapshot} 不是 ContextSnapshot，无法作为 Decomposition 的 consumed→ 目标（I6）"
        )));
    }
    if children.len() < 2 {
        return Err(GraphError::InvalidPayload(format!(
            "目标拆解至少需 2 个子目标，得到 {}",
            children.len()
        )));
    }
    nonempty(rationale, "rationale")?;
    for (i, child) in children.iter().enumerate() {
        nonempty(&child.title, &format!("children[{i}].title"))?;
        nonempty(&child.description, &format!("children[{i}].description"))?;
    }
    Ok(())
}

fn nonempty(value: &str, field: &str) -> GraphResult<()> {
    if value.trim().is_empty() {
        Err(GraphError::InvalidPayload(format!("{field} 不能为空")))
    } else {
        Ok(())
    }
}
