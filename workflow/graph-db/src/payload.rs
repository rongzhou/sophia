//! 节点 Payload Schema。
//!
//! 见 docs/workflow_graph_spec.md 第四节。每个 payload 用 `#[serde(deny_unknown_fields)]`
//! 强制 strict 模式（多余字段拒绝，对应 LLM 输出 `additionalProperties: false`）。
//!
//! [`NodePayload`] 是按 role 标签的判别联合，统一承载各类 payload，便于存储层
//! 以单一类型处理节点并校验 `(role, payload)` 一致性。

use serde::{Deserialize, Serialize};

// ============ 目标簇 ============

/// ObjectiveNode（4.1.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectivePayload {
    pub title: String,
    pub description: String,
}

/// 约束种类（4.1.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    Invariant,
    OutOfScope,
    Preference,
    Forbidden,
}

/// verifier 种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifierKind {
    HiddenCase,
    AuditRule,
    Manual,
}

/// verifier 引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verifier {
    pub kind: VerifierKind,
    pub r#ref: String,
}

/// ConstraintNode（4.1.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstraintPayload {
    pub kind: ConstraintKind,
    pub statement: String,
    #[serde(default)]
    pub verifier: Option<Verifier>,
}

/// AcceptanceCriterionNode（4.1.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceCriterionPayload {
    pub statement: String,
    #[serde(default)]
    pub verifier: Option<Verifier>,
}

/// DecompositionNode（4.1.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecompositionPayload {
    pub rationale: String,
    pub proposed_count: u32,
}

/// MilestoneNode（4.1.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MilestonePayload {
    pub name: String,
    pub summary: String,
}

// ============ 变更簇 ============

/// 变更请求种类（4.2.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeRequestKind {
    NewRequirement,
    Correction,
    Preference,
    Rejection,
    ConstraintChange,
}

/// 变更优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangePriority {
    Must,
    Should,
    Could,
}

/// ChangeRequestNode（4.2.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRequestPayload {
    pub kind: ChangeRequestKind,
    pub request: String,
    pub priority: ChangePriority,
}

/// 风险等级（4.2.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
}

/// 影响半径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlastRadius {
    Local,
    Module,
    Subsystem,
    CrossSystem,
    ProductScale,
}

/// 推荐策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedStrategy {
    DirectChange,
    VerticalSlice,
    StagedRollout,
    Spike,
    RejectAsTooLarge,
}

/// AssessmentNode（4.2.2）。仅承载评估头部信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentPayload {
    pub risk: Risk,
    pub blast_radius: BlastRadius,
    pub recommended_strategy: RecommendedStrategy,
    #[serde(default)]
    pub affected_systems: Vec<String>,
    #[serde(default)]
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

/// FirstSliceNode（4.2.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstSlicePayload {
    pub purpose: String,
}

/// Assessment 的 LLM 输出契约（4.2.2）。
///
/// **仅作为 prompt response 形态，不直接构造图边**：由确定性 helper
/// （`crate::assessment::decompose_assessment`）拆为 Assessment + FirstSlice +
/// Constraint(Invariant) + Decision 多节点与 `assesses` / `proposes` 边。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentLlmOutput {
    #[serde(flatten)]
    pub head: AssessmentPayload,
    #[serde(default)]
    pub proposed_first_slice: Option<FirstSlicePayload>,
    #[serde(default)]
    pub proposed_invariants: Vec<ConstraintPayload>,
    pub proposed_recommended_action: DecisionAction,
    pub self_check: AssessmentSelfCheck,
}

/// Assessment 自检字段（4.2.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentSelfCheck {
    pub affects_only_visible_targets: bool,
    pub no_hidden_answers: bool,
    pub no_pseudocode_or_code: bool,
}

// ============ 事件簇 ============

/// 接受决定（4.3.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceDecision {
    Accepted,
    AcceptedWithChanges,
    Satisfied,
}

/// AcceptanceEventNode（4.3.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptancePayload {
    pub decision: AcceptanceDecision,
    #[serde(default)]
    pub notes: String,
}

/// WithdrawalEventNode（4.3.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawalPayload {
    pub reason: String,
}

/// ActivationEventNode（4.3.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPayload {
    #[serde(default)]
    pub reason: String,
}

/// 澄清种类（4.3.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationKind {
    Question,
    Answer,
}

/// ClarificationNode（4.3.4）。provenance 由 kind 决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClarificationPayload {
    pub kind: ClarificationKind,
    pub body: String,
}

// ============ 推理与执行簇 ============

/// ContextSnapshotNode（4.4.1）。`snapshot` 用 JSON Value 承载 ActiveContext，
/// 由 active context 推导模块（第五节）填充；存储层会校验 digest 与 snapshot 内容一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSnapshotPayload {
    pub schema_version: u32,
    pub snapshot: serde_json::Value,
    /// 64 位 lower-case hex（SHA-256）。
    pub digest: String,
}

/// 决策动作（4.4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    DesignSolution,
    ImplementDesign,
    RepairCode,
    ReviseDesign,
    Decompose,
    Backtrack,
    Select,
    Materialize,
    NeedsClarification,
}

/// 目标规模。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalSize {
    Tiny,
    Small,
    Medium,
    Large,
}

/// 压力等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pressure {
    Low,
    Medium,
    High,
}

/// 编译状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompileStatus {
    NotChecked,
    Pass,
    Fail,
}

/// 错误类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    None,
    Local,
    Conceptual,
    Integration,
}

/// 状态评估：按 kind 标签判别（4.4.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum StateAssessment {
    Goal {
        goal_size: GoalSize,
        decomposition_pressure: Pressure,
        active_milestone_present: bool,
        outstanding_clarifications: u32,
    },
    Code {
        has_pseudocode: bool,
        has_code: bool,
        compile_status: CompileStatus,
        error_type: ErrorType,
        repair_attempts: u32,
    },
    Change {
        blast_radius: BlastRadius,
        risk: Risk,
        affects_active_milestone: bool,
    },
}

/// DecisionNode（4.4.2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionPayload {
    pub selected_action: DecisionAction,
    /// [0.0, 1.0]。
    pub confidence: f32,
    pub rationale: String,
    pub state_assessment: StateAssessment,
}

/// PseudocodeNode（4.4.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PseudocodePayload {
    pub purpose: String,
    /// 必须为 "content.pseudo"。
    pub artifact_path: String,
}

/// CodeNode（4.4.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodePayload {
    /// 非空，至少一个候选文件路径。
    pub files: Vec<String>,
}

/// 诊断种类（4.4.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    PseudoCheck,
    CodeCheck,
    ConstraintAudit,
    ArtifactWrite,
    ArtifactDiff,
    RegressionGate,
}

/// 诊断严重度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

/// 单条诊断项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticItem {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub problem: String,
    #[serde(default)]
    pub location: Option<String>,
}

/// DiagnosticNode（4.4.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPayload {
    pub kind: DiagnosticKind,
    pub ok: bool,
    #[serde(default)]
    pub diagnostics: Vec<DiagnosticItem>,
}

/// SelectionNode（4.4.6）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionPayload {
    pub rationale: String,
}

/// MaterializeNode（4.4.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializePayload {
    pub target_root: String,
    pub files: Vec<String>,
}

/// LLM 调用失败种类（4.4.8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawLlmFailureKind {
    ExecutionError,
    ParseError,
    ValidationError,
    SelfCheckFailure,
}

/// RawLlmNode（4.4.8）。强制 creation_status=Failed。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLlmPayload {
    pub failure_kind: RawLlmFailureKind,
    pub operation: String,
    pub error_summary: String,
}

/// 判别联合：按 role 承载具体 payload。
///
/// 序列化为 `{ "role_tag": "...", ...payload }` 形式，便于存储与按 role 分发。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", rename_all = "snake_case")]
pub enum NodePayload {
    Objective(ObjectivePayload),
    Constraint(ConstraintPayload),
    AcceptanceCriterion(AcceptanceCriterionPayload),
    Decomposition(DecompositionPayload),
    Milestone(MilestonePayload),
    ChangeRequest(ChangeRequestPayload),
    Assessment(AssessmentPayload),
    FirstSlice(FirstSlicePayload),
    Acceptance(AcceptancePayload),
    Withdrawal(WithdrawalPayload),
    Activation(ActivationPayload),
    Clarification(ClarificationPayload),
    ContextSnapshot(ContextSnapshotPayload),
    Decision(DecisionPayload),
    Pseudocode(PseudocodePayload),
    Code(CodePayload),
    Diagnostic(DiagnosticPayload),
    Selection(SelectionPayload),
    Materialize(MaterializePayload),
    RawLlm(RawLlmPayload),
}

impl NodePayload {
    /// 该 payload 对应的 role（用于校验 meta.role 与 payload 一致）。
    pub fn role(&self) -> crate::ids::NodeRole {
        use crate::ids::NodeRole as R;
        match self {
            NodePayload::Objective(_) => R::Objective,
            NodePayload::Constraint(_) => R::Constraint,
            NodePayload::AcceptanceCriterion(_) => R::AcceptanceCriterion,
            NodePayload::Decomposition(_) => R::Decomposition,
            NodePayload::Milestone(_) => R::Milestone,
            NodePayload::ChangeRequest(_) => R::ChangeRequest,
            NodePayload::Assessment(_) => R::Assessment,
            NodePayload::FirstSlice(_) => R::FirstSlice,
            NodePayload::Acceptance(_) => R::AcceptanceEvent,
            NodePayload::Withdrawal(_) => R::WithdrawalEvent,
            NodePayload::Activation(_) => R::ActivationEvent,
            NodePayload::Clarification(_) => R::Clarification,
            NodePayload::ContextSnapshot(_) => R::ContextSnapshot,
            NodePayload::Decision(_) => R::Decision,
            NodePayload::Pseudocode(_) => R::Pseudocode,
            NodePayload::Code(_) => R::Code,
            NodePayload::Diagnostic(_) => R::Diagnostic,
            NodePayload::Selection(_) => R::Selection,
            NodePayload::Materialize(_) => R::Materialize,
            NodePayload::RawLlm(_) => R::RawLlm,
        }
    }
}
