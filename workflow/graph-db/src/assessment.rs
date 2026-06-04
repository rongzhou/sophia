//! 评估拆解协议（workflow_graph_spec 4.2.2）。
//!
//! `AssessmentLlmOutput` 是 LLM 的 prompt response 形态，**不直接构造图边**。
//! 本模块的确定性 helper 把它拆为多节点 + 边：
//! - 1 个 `Assessment` 节点（承载头部信息）；`assesses→` 指向被评估对象；
//! - 可选 `FirstSlice` 节点；`proposes→`；
//! - 0..N 个 `Constraint(Invariant)` 节点（regression 约束，每条独立）；`proposes→`；
//! - 1 个 `Decision` 节点（推荐的下一步决策，change-kind state assessment）；`proposes→`。
//!
//! 这些节点均为 LLM-provenance（除 Decision 也可为 LLM），因此每个都需 `consumed→
//! ContextSnapshot` 边（I6）：调用方传入已建的 snapshot 节点，helper 负责连边。

use crate::edge::EdgeKind;
use crate::error::{GraphError, GraphResult};
use crate::ids::{NodeId, NodeRole};
use crate::payload::{
    AssessmentLlmOutput, ConstraintKind, ConstraintPayload, DecisionPayload, StateAssessment,
    Verifier,
};
use crate::store::GraphStore;

/// 拆解产物：本次评估新建的全部节点。
#[derive(Debug, Clone)]
pub struct AssessmentNodes {
    pub assessment: NodeId,
    pub first_slice: Option<NodeId>,
    pub invariants: Vec<NodeId>,
    pub decision: NodeId,
}

/// 把一个 `AssessmentLlmOutput` 拆解为图节点与边。
///
/// 参数：
/// - `output`：LLM 输出（已通过 schema 验证）；
/// - `assessed`：被评估对象（ChangeRequest | Objective），`assesses→` 指向它；
/// - `snapshot`：本次 LLM 调用前建的 `ContextSnapshot` 节点（用于 I6 `consumed→` 边）。
///
/// self-check 必须全部为真（4.2.2）——否则拒绝拆解（视为无效评估，调用方应改 emit
/// `RawLlmNode`）。`proposed_invariants` 的 kind 必须为 `Invariant`。
pub fn decompose_assessment(
    store: &mut GraphStore,
    output: &AssessmentLlmOutput,
    assessed: NodeId,
    snapshot: NodeId,
) -> GraphResult<AssessmentNodes> {
    prevalidate_assessment(store, output, assessed, snapshot)?;

    // 1) Assessment 节点。
    let assessment = store
        .as_llm()
        .assessment("assessment", output.head.clone())?;
    store.append_edge(assessment, snapshot, EdgeKind::Consumed)?;
    store.append_edge(assessment, assessed, EdgeKind::Assesses)?;

    // 2) 可选 FirstSlice。
    let first_slice = if let Some(fs) = &output.proposed_first_slice {
        let id = store.as_llm().first_slice("first_slice", fs.clone())?;
        store.append_edge(assessment, id, EdgeKind::Proposes)?;
        Some(id)
    } else {
        None
    };

    // 3) regression 约束（每条独立，kind 必须 Invariant）。
    let mut invariants = Vec::new();
    for (i, c) in output.proposed_invariants.iter().enumerate() {
        let id = store
            .as_llm()
            .constraint(format!("invariant_{i}"), c.clone())?;
        store.append_edge(assessment, id, EdgeKind::Proposes)?;
        invariants.push(id);
    }

    // 4) 推荐决策（change-kind state assessment，由评估头部派生）。
    let decision_payload = DecisionPayload {
        selected_action: output.proposed_recommended_action,
        confidence: 1.0,
        rationale: format!(
            "评估推荐：risk={:?}, blast_radius={:?}, strategy={:?}",
            output.head.risk, output.head.blast_radius, output.head.recommended_strategy
        ),
        state_assessment: StateAssessment::Change {
            blast_radius: output.head.blast_radius,
            risk: output.head.risk,
            // 评估阶段尚无 active milestone 影响判定；保守取 false。
            affects_active_milestone: false,
        },
    };
    let decision = store
        .as_llm()
        .decision("recommended_action", decision_payload)?;
    store.append_edge(decision, snapshot, EdgeKind::Consumed)?;
    store.append_edge(assessment, decision, EdgeKind::Proposes)?;

    Ok(AssessmentNodes {
        assessment,
        first_slice,
        invariants,
        decision,
    })
}

fn prevalidate_assessment(
    store: &GraphStore,
    output: &AssessmentLlmOutput,
    assessed: NodeId,
    snapshot: NodeId,
) -> GraphResult<()> {
    if !matches!(
        store.role_of(assessed),
        Some(NodeRole::ChangeRequest | NodeRole::Objective)
    ) {
        return Err(GraphError::InvalidPayload(format!(
            "{assessed} 不是 ChangeRequest/Objective，无法作为 assesses→ 目标"
        )));
    }
    if store.role_of(snapshot) != Some(NodeRole::ContextSnapshot) {
        return Err(GraphError::InvalidPayload(format!(
            "{snapshot} 不是 ContextSnapshot，无法作为 Assessment/Decision 的 consumed→ 目标（I6）"
        )));
    }

    // self-check 全真校验（不通过即拒绝）。
    let sc = &output.self_check;
    if !(sc.affects_only_visible_targets && sc.no_hidden_answers && sc.no_pseudocode_or_code) {
        return Err(GraphError::InvalidPayload(
            "Assessment self-check 未全部通过，拒绝拆解".to_string(),
        ));
    }

    if let Some(fs) = &output.proposed_first_slice {
        nonempty(&fs.purpose, "proposed_first_slice.purpose")?;
    }

    for (i, c) in output.proposed_invariants.iter().enumerate() {
        validate_invariant(i, c)?;
    }
    Ok(())
}

fn validate_invariant(idx: usize, c: &ConstraintPayload) -> GraphResult<()> {
    if c.kind != ConstraintKind::Invariant {
        return Err(GraphError::InvalidPayload(format!(
            "proposed_invariants[{idx}] 的 kind 必须为 Invariant"
        )));
    }
    nonempty(
        &c.statement,
        &format!("proposed_invariants[{idx}].statement"),
    )?;
    validate_verifier(&c.verifier)
}

fn validate_verifier(verifier: &Option<Verifier>) -> GraphResult<()> {
    if let Some(verifier) = verifier {
        nonempty(&verifier.r#ref, "verifier.ref")?;
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
