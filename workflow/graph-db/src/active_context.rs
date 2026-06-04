//! Active Context 推导。
//!
//! 见 docs/workflow_graph_spec.md 第五节。Active context 是确定性管线根据图当前
//! 状态计算出的视图，喂给 `ContextSnapshotNode` 与下游 LLM 调用。它**不存任何字段，
//! 每次重新计算**（不变量 I10）。
//!
//! 关键性质：
//! - binding 谓词（5.2）：链头 + （human 隐式接受 ∨ 链上有 AcceptanceEvent），且无更晚的
//!   WithdrawalEvent；
//! - binding 沿 `member_of` / `groups` / `requires` 单向继承（5.3）；
//! - 序列化稳定（集合按 NodeId 排序、字段固定顺序、RFC 3339 UTC），digest 为 SHA-256
//!   lower-case hex。
//!
//! `*View` 只暴露字段子集，不暴露 NodeMeta 全量，避免向 LLM 注入无关 metadata。

use crate::edge::EdgeKind;
use crate::ids::{NodeId, NodeRole, Provenance};
use crate::payload::{ClarificationKind, NodePayload};
use crate::store::GraphStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// 目标视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveView {
    pub id: NodeId,
    pub title: String,
    pub description: String,
}

/// 约束视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintView {
    pub id: NodeId,
    pub kind: crate::payload::ConstraintKind,
    pub statement: String,
}

/// 验收条件视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCriterionView {
    pub id: NodeId,
    pub statement: String,
}

/// milestone 视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneView {
    pub id: NodeId,
    pub name: String,
    pub summary: String,
}

/// 变更请求视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRequestView {
    pub id: NodeId,
    pub request: String,
}

/// 澄清（问题）视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationView {
    pub id: NodeId,
    pub body: String,
}

/// Active Context：确定性管线根据图当前状态计算出的视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveContext {
    pub bound_objectives: Vec<ObjectiveView>,
    pub active_milestone: Option<MilestoneView>,
    pub bound_constraints: Vec<ConstraintView>,
    pub bound_acceptance_criteria: Vec<AcceptanceCriterionView>,
    pub open_change_requests: Vec<ChangeRequestView>,
    pub outstanding_questions: Vec<ClarificationView>,
    /// 64 位 lower-case hex（SHA-256）。
    pub digest: String,
}

/// 推导 active context（不变量 I10：仅依赖图当前状态）。
pub fn derive_active_context(store: &GraphStore) -> ActiveContext {
    let heads = version_chain_heads(store);
    let mut bound = bound_heads(store, &heads);
    inherit_binding(store, &mut bound);

    let active_ms = active_milestone(store, &bound);
    let bound_objectives = collect_objectives(store, &bound);
    let bound_constraints = collect_constraints(store, &bound, active_ms);
    let bound_acceptance_criteria = collect_acceptance_criteria(store, &bound, active_ms);
    let open_change_requests = collect_open_change_requests(store, &heads);
    let outstanding_questions = collect_outstanding_questions(store, &heads);
    let active_milestone = active_ms.map(|id| milestone_view(store, id));

    // 稳定序列化（不含 digest 自身）后计算 SHA-256。
    let body = SnapshotBody {
        bound_objectives: &bound_objectives,
        active_milestone: &active_milestone,
        bound_constraints: &bound_constraints,
        bound_acceptance_criteria: &bound_acceptance_criteria,
        open_change_requests: &open_change_requests,
        outstanding_questions: &outstanding_questions,
    };
    let digest = sha256_hex(&serde_json::to_string(&body).expect("snapshot 序列化"));

    ActiveContext {
        bound_objectives,
        active_milestone,
        bound_constraints,
        bound_acceptance_criteria,
        open_change_requests,
        outstanding_questions,
        digest,
    }
}

/// 用于 digest 计算的稳定序列化体（字段顺序固定，集合已按 NodeId 排序）。
#[derive(Serialize)]
struct SnapshotBody<'a> {
    bound_objectives: &'a [ObjectiveView],
    active_milestone: &'a Option<MilestoneView>,
    bound_constraints: &'a [ConstraintView],
    bound_acceptance_criteria: &'a [AcceptanceCriterionView],
    open_change_requests: &'a [ChangeRequestView],
    outstanding_questions: &'a [ClarificationView],
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// ---- 步骤 1：链头集合 ----

/// 版本链头：没有任何 supersedes 边指向它的节点。
///
/// 注：`X supersedes→ Y` 表示 X 是新版本、接管 Y 的语义，故 Y 被覆盖。链头是
/// 没有更新版本覆盖它的节点，即没有 supersedes 边以它为 `to`。
fn version_chain_heads(store: &GraphStore) -> BTreeSet<NodeId> {
    let superseded: BTreeSet<NodeId> = store
        .edges_of_kind(EdgeKind::Supersedes)
        .map(|e| e.to)
        .collect();
    store
        .nodes()
        .map(|n| n.meta.id)
        .filter(|id| !superseded.contains(id))
        .collect()
}

/// 节点的版本链：自身 + 它通过 supersedes 间接覆盖的所有旧节点。
fn chain_of(store: &GraphStore, head: NodeId) -> Vec<NodeId> {
    let mut chain = vec![head];
    let mut cur = head;
    let mut guard = 0;
    while let Some(next) = store
        .edges_of_kind(EdgeKind::Supersedes)
        .find(|e| e.from == cur)
        .map(|e| e.to)
    {
        chain.push(next);
        cur = next;
        guard += 1;
        if guard > store.nodes().count() + 1 {
            break;
        }
    }
    chain
}

/// 取某节点所属版本链的链头（沿 supersedes 入边回溯）。
fn head_of_chain(store: &GraphStore, node: NodeId) -> NodeId {
    let mut cur = node;
    let mut guard = 0;
    while let Some(prev) = store
        .edges_of_kind(EdgeKind::Supersedes)
        .find(|e| e.to == cur)
        .map(|e| e.from)
    {
        cur = prev;
        guard += 1;
        if guard > store.nodes().count() + 1 {
            break;
        }
    }
    cur
}

// ---- 步骤 2–3：接受 / 撤销查询 ----

fn bound_heads(store: &GraphStore, heads: &BTreeSet<NodeId>) -> BTreeSet<NodeId> {
    let mut bound = BTreeSet::new();
    for &h in heads {
        if is_bound(store, h) {
            bound.insert(h);
        }
    }
    bound
}

/// binding 谓词（5.2）。
fn is_bound(store: &GraphStore, head: NodeId) -> bool {
    let chain = chain_of(store, head);

    // human provenance 隐式视为已接受。
    let accepted = store.provenance_of(head) == Some(Provenance::Human)
        || latest_acceptance_ts(store, &chain).is_some();
    if !accepted {
        return false;
    }

    // 撤销：若存在更晚（或在无接受时存在任意）的 WithdrawalEvent，则解绑。
    let latest_acc = latest_acceptance_ts(store, &chain);
    let latest_wd = latest_withdrawal_ts(store, &chain);
    match (latest_wd, latest_acc) {
        (Some(wd), Some(acc)) => wd <= acc, // 撤销不晚于接受 → 仍 bound
        (Some(_), None) => false,           // 仅撤销、无接受 → 解绑
        (None, _) => true,                  // 无撤销 → bound
    }
}

/// 链上最新 AcceptanceEvent 的时间戳。
fn latest_acceptance_ts(
    store: &GraphStore,
    chain: &[NodeId],
) -> Option<chrono::DateTime<chrono::Utc>> {
    event_ts_over_chain(store, chain, EdgeKind::Accepts, NodeRole::AcceptanceEvent)
}

/// 链上最新 WithdrawalEvent 的时间戳。
fn latest_withdrawal_ts(
    store: &GraphStore,
    chain: &[NodeId],
) -> Option<chrono::DateTime<chrono::Utc>> {
    event_ts_over_chain(store, chain, EdgeKind::Withdraws, NodeRole::WithdrawalEvent)
}

/// 链上「某事件 role 经某 edge 指向链内任一节点」的最新事件时间戳。
fn event_ts_over_chain(
    store: &GraphStore,
    chain: &[NodeId],
    edge: EdgeKind,
    event_role: NodeRole,
) -> Option<chrono::DateTime<chrono::Utc>> {
    store
        .edges_of_kind(edge)
        .filter(|e| chain.contains(&e.to) && store.role_of(e.from) == Some(event_role))
        .filter_map(|e| store.created_at(e.from))
        .max()
}

// ---- 步骤 4：继承传播 ----

/// 沿 `member_of` / `groups` / `requires` 单向继承 binding（5.3）。
///
/// 设计算法（5.4 步骤 4）先处理 Decomposition、再处理 Milestone，且后者读取的是
/// **被前者更新过的** bound 集合——因此 `Decomposition → member Milestone → grouped
/// Objective` 这类传递链必须被覆盖。这里用不动点迭代（重复传播直到无新增），
/// 既覆盖传递性，又与处理顺序无关。
fn inherit_binding(store: &GraphStore, bound: &mut BTreeSet<NodeId>) {
    loop {
        let snapshot: Vec<NodeId> = bound.iter().copied().collect();
        let before = bound.len();
        for node in snapshot {
            match store.role_of(node) {
                Some(NodeRole::Decomposition) => {
                    // member_of→ d 的 Objective / Milestone / FirstSlice 继承 binding。
                    for e in store.edges_of_kind(EdgeKind::MemberOf) {
                        if e.to == node {
                            bound.insert(head_of_chain(store, e.from));
                        }
                    }
                }
                Some(NodeRole::Milestone) => {
                    for e in store.edges_of_kind(EdgeKind::Groups) {
                        if e.from == node {
                            bound.insert(head_of_chain(store, e.to));
                        }
                    }
                    for e in store.edges_of_kind(EdgeKind::Requires) {
                        if e.from == node {
                            bound.insert(head_of_chain(store, e.to));
                        }
                    }
                }
                _ => {}
            }
        }
        // 不动点：无新增即停止。
        if bound.len() == before {
            break;
        }
    }
}

// ---- 步骤 5：active milestone ----

fn active_milestone(store: &GraphStore, bound: &BTreeSet<NodeId>) -> Option<NodeId> {
    // 候选：bound 的 Milestone。
    let candidates: BTreeSet<NodeId> = bound
        .iter()
        .copied()
        .filter(|&id| store.role_of(id) == Some(NodeRole::Milestone))
        .collect();
    // 取最新 ActivationEvent 指向的候选。
    store
        .edges_of_kind(EdgeKind::Activates)
        .filter(|e| candidates.contains(&e.to))
        .filter_map(|e| store.created_at(e.from).map(|ts| (ts, e.to)))
        .max_by_key(|(ts, _)| *ts)
        .map(|(_, target)| target)
}

// ---- 步骤 6–8：聚合 ----

fn collect_objectives(store: &GraphStore, bound: &BTreeSet<NodeId>) -> Vec<ObjectiveView> {
    let mut out: Vec<ObjectiveView> = bound
        .iter()
        .filter_map(|&id| match store.node(id).map(|n| &n.payload) {
            Some(NodePayload::Objective(o)) => Some(ObjectiveView {
                id,
                title: o.title.clone(),
                description: o.description.clone(),
            }),
            _ => None,
        })
        .collect();
    out.sort_by_key(|v| v.id);
    out
}

fn collect_constraints(
    store: &GraphStore,
    bound: &BTreeSet<NodeId>,
    active_ms: Option<NodeId>,
) -> Vec<ConstraintView> {
    let mut ids: BTreeSet<NodeId> = BTreeSet::new();

    // active milestone 通过 requires / excludes 指向的 bound 约束。
    if let Some(ms) = active_ms {
        for kind in [EdgeKind::Requires, EdgeKind::Excludes] {
            for e in store.edges_of_kind(kind) {
                if e.from == ms && bound.contains(&e.to) {
                    ids.insert(e.to);
                }
            }
        }
    }
    // bound objective 通过 constrained_by 指向的 bound 约束。
    for e in store.edges_of_kind(EdgeKind::ConstrainedBy) {
        if bound.contains(&e.from)
            && store.role_of(e.from) == Some(NodeRole::Objective)
            && bound.contains(&e.to)
        {
            ids.insert(e.to);
        }
    }

    let mut out: Vec<ConstraintView> = ids
        .into_iter()
        .filter_map(|id| match store.node(id).map(|n| &n.payload) {
            Some(NodePayload::Constraint(c)) => Some(ConstraintView {
                id,
                kind: c.kind,
                statement: c.statement.clone(),
            }),
            _ => None,
        })
        .collect();
    out.sort_by_key(|v| v.id);
    out
}

fn collect_acceptance_criteria(
    store: &GraphStore,
    bound: &BTreeSet<NodeId>,
    active_ms: Option<NodeId>,
) -> Vec<AcceptanceCriterionView> {
    let mut ids: BTreeSet<NodeId> = BTreeSet::new();
    // validated_by：from 是 bound objective 或 active milestone。
    for e in store.edges_of_kind(EdgeKind::ValidatedBy) {
        let from_ok = (bound.contains(&e.from)
            && store.role_of(e.from) == Some(NodeRole::Objective))
            || Some(e.from) == active_ms;
        if from_ok {
            ids.insert(e.to);
        }
    }
    let mut out: Vec<AcceptanceCriterionView> = ids
        .into_iter()
        .filter_map(|id| match store.node(id).map(|n| &n.payload) {
            Some(NodePayload::AcceptanceCriterion(a)) => Some(AcceptanceCriterionView {
                id,
                statement: a.statement.clone(),
            }),
            _ => None,
        })
        .collect();
    out.sort_by_key(|v| v.id);
    out
}

fn collect_open_change_requests(
    store: &GraphStore,
    heads: &BTreeSet<NodeId>,
) -> Vec<ChangeRequestView> {
    let mut out: Vec<ChangeRequestView> = heads
        .iter()
        .filter(|&&id| store.role_of(id) == Some(NodeRole::ChangeRequest))
        .filter(|&&id| {
            let chain = chain_of(store, id);
            // 无接受、无撤销 → open。
            latest_acceptance_ts(store, &chain).is_none()
                && latest_withdrawal_ts(store, &chain).is_none()
        })
        .filter_map(|&id| match store.node(id).map(|n| &n.payload) {
            Some(NodePayload::ChangeRequest(c)) => Some(ChangeRequestView {
                id,
                request: c.request.clone(),
            }),
            _ => None,
        })
        .collect();
    out.sort_by_key(|v| v.id);
    out
}

fn collect_outstanding_questions(
    store: &GraphStore,
    heads: &BTreeSet<NodeId>,
) -> Vec<ClarificationView> {
    let mut out: Vec<ClarificationView> = heads
        .iter()
        .filter_map(|&id| {
            let payload = store.node(id).map(|n| &n.payload)?;
            let NodePayload::Clarification(c) = payload else {
                return None;
            };
            if c.kind != ClarificationKind::Question {
                return None;
            }
            // 是否被回答：存在 answers→ 该 question 的 Clarification(Answer)。
            let answered = store.edges_of_kind(EdgeKind::Answers).any(|e| e.to == id);
            if answered {
                return None;
            }
            Some(ClarificationView {
                id,
                body: c.body.clone(),
            })
        })
        .collect();
    out.sort_by_key(|v| v.id);
    out
}

fn milestone_view(store: &GraphStore, id: NodeId) -> MilestoneView {
    match store.node(id).map(|n| &n.payload) {
        Some(NodePayload::Milestone(m)) => MilestoneView {
            id,
            name: m.name.clone(),
            summary: m.summary.clone(),
        },
        _ => MilestoneView {
            id,
            name: String::new(),
            summary: String::new(),
        },
    }
}

/// 由 active context 构造 `ContextSnapshotPayload`（4.4.1）。
///
/// 这是「每次 LLM 调用前先建 ContextSnapshot」接入点（第七节）的确定性 helper：
/// 推导 → 序列化 → 装入 payload。digest 与 snapshot 内容来自同一次推导，保证一致。
pub fn snapshot_payload(ctx: &ActiveContext) -> crate::payload::ContextSnapshotPayload {
    crate::payload::ContextSnapshotPayload {
        schema_version: 1,
        snapshot: serde_json::to_value(ctx).expect("ActiveContext 序列化"),
        digest: ctx.digest.clone(),
    }
}

/// 按 active context 的 digest 契约重算 snapshot digest。
///
/// 真实 ActiveContext snapshot 含有固定六个 body 字段和一个 `digest` 字段；digest 只覆盖
/// 六个 body 字段，且字段顺序与 [`SnapshotBody`] 一致。恢复 / 测试用的其它 JSON 值按自身
/// canonical JSON 计算，仍能校验“digest 与 payload 内容一致”。
pub(crate) fn digest_snapshot_value(snapshot: &Value) -> Result<String, String> {
    if let Ok(ctx) = serde_json::from_value::<ActiveContext>(snapshot.clone()) {
        let body = SnapshotBody {
            bound_objectives: &ctx.bound_objectives,
            active_milestone: &ctx.active_milestone,
            bound_constraints: &ctx.bound_constraints,
            bound_acceptance_criteria: &ctx.bound_acceptance_criteria,
            open_change_requests: &ctx.open_change_requests,
            outstanding_questions: &ctx.outstanding_questions,
        };
        return serde_json::to_string(&body)
            .map(|raw| sha256_hex(&raw))
            .map_err(|e| e.to_string());
    }
    serde_json::to_string(snapshot)
        .map(|raw| sha256_hex(&raw))
        .map_err(|e| e.to_string())
}
