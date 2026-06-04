//! Constraint audit / regression gate（tools 层）。
//!
//! 见 docs/workflow_graph_spec.md 4.1.2（ConstraintNode + verifier）、4.4.5
//! （Diagnostic kind=ConstraintAudit / RegressionGate）、第七节接入点 4。
//!
//! 职责：对一组约束（含可选 verifier）做审计——`Invariant` / `Forbidden` 约束由可执行
//! verifier 驱动 regression gate，其余 kind 仅作上下文（不 gate）。本 crate 只组装审计判定与
//! 结构化报告；verifier 的**实际执行**（跑 hidden case / audit rule）由确定性管线
//! 注入结果（[`VerifierOutcome`]），不在本层实现——与 `tools/materialize` 消费
//! `GateReport` 同构，保持 tools 层不依赖 workflow 图与具体运行器。

#![forbid(unsafe_code)]

use thiserror::Error;

/// audit 层结果别名。
pub type AuditResult<T> = Result<T, AuditError>;

/// audit 层硬错误。
#[derive(Debug, Error)]
pub enum AuditError {
    /// 约束声明了 verifier，但未提供其执行结果。
    #[error("约束 `{constraint}` 的 verifier `{verifier}` 缺少执行结果")]
    MissingVerifierOutcome {
        constraint: String,
        verifier: String,
    },
}

/// 约束种类（对齐 workflow_graph_spec 4.1.2 ConstraintKind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// 必须保持的旧行为，驱动 regression gate。
    Invariant,
    /// 显式排除范围。
    OutOfScope,
    /// 软约束。
    Preference,
    /// 禁止行为。
    Forbidden,
}

/// verifier 种类（对齐 4.1.2 VerifierKind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifierKind {
    /// hidden case：可由确定性管线执行，驱动 gate。
    HiddenCase,
    /// audit rule：可由确定性管线执行，驱动 gate。
    AuditRule,
    /// 人工：仅作上下文，不 gate。
    Manual,
}

/// 一条待审计的约束（tools 层视图，从 ConstraintNode 投影而来）。
#[derive(Debug, Clone)]
pub struct Constraint {
    /// 约束标识（用于诊断定位，通常是 NodeId 字符串）。
    pub id: String,
    pub kind: ConstraintKind,
    pub statement: String,
    /// 可选 verifier：`(kind, ref)`。
    pub verifier: Option<(VerifierKind, String)>,
}

/// verifier 执行结果（由确定性管线 / hidden-case 运行器注入）。
#[derive(Debug, Clone)]
pub struct VerifierOutcome {
    /// 对应 verifier 的 `ref`。
    pub verifier_ref: String,
    pub passed: bool,
    pub detail: String,
}

/// 单条约束的审计结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintVerdict {
    /// 通过（verifier 执行通过，或无需 gate）。
    Pass,
    /// 失败（verifier 执行未通过）——驱动 regression gate。
    Fail { detail: String },
    /// 跳过（Manual / 无 verifier，仅作上下文）。
    Skipped { reason: String },
}

/// 单条约束的审计项。
#[derive(Debug, Clone)]
pub struct ConstraintAuditItem {
    pub constraint_id: String,
    pub verdict: ConstraintVerdict,
}

/// 一次约束审计的报告。
#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    pub items: Vec<ConstraintAuditItem>,
}

impl AuditReport {
    /// 是否整体通过（无 `Fail`）。
    pub fn ok(&self) -> bool {
        !self
            .items
            .iter()
            .any(|i| matches!(i.verdict, ConstraintVerdict::Fail { .. }))
    }

    /// 失败的约束项。
    pub fn failures(&self) -> impl Iterator<Item = &ConstraintAuditItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.verdict, ConstraintVerdict::Fail { .. }))
    }
}

/// 对一组约束做审计。
///
/// 规则（4.1.2 / 第七节 4）：
/// - `Invariant` / `Forbidden` + 可执行 verifier（HiddenCase / AuditRule）→ 由对应
///   [`VerifierOutcome`] 决定 Pass/Fail（regression gate）；缺结果则报 [`AuditError`]；
/// - 其余约束（非 gate kind，或 Manual / 无 verifier）→ `Skipped`（仅上下文）。
///
/// `outcomes` 按 `verifier_ref` 查找；顺序无关。
pub fn audit_constraints(
    constraints: &[Constraint],
    outcomes: &[VerifierOutcome],
) -> AuditResult<AuditReport> {
    let mut report = AuditReport::default();
    for c in constraints {
        let verdict = audit_one(c, outcomes)?;
        report.items.push(ConstraintAuditItem {
            constraint_id: c.id.clone(),
            verdict,
        });
    }
    Ok(report)
}

fn audit_one(c: &Constraint, outcomes: &[VerifierOutcome]) -> AuditResult<ConstraintVerdict> {
    // 只有 Invariant / Forbidden 可由可执行 verifier 驱动 gate；其余仅作上下文。
    if !drives_gate(c.kind) {
        return Ok(ConstraintVerdict::Skipped {
            reason: format!("{:?} 约束不驱动 gate", c.kind),
        });
    }
    match &c.verifier {
        Some((VerifierKind::HiddenCase, vref)) | Some((VerifierKind::AuditRule, vref)) => {
            let outcome = outcomes
                .iter()
                .find(|o| &o.verifier_ref == vref)
                .ok_or_else(|| AuditError::MissingVerifierOutcome {
                    constraint: c.id.clone(),
                    verifier: vref.clone(),
                })?;
            if outcome.passed {
                Ok(ConstraintVerdict::Pass)
            } else {
                Ok(ConstraintVerdict::Fail {
                    detail: outcome.detail.clone(),
                })
            }
        }
        Some((VerifierKind::Manual, _)) => Ok(ConstraintVerdict::Skipped {
            reason: "Manual verifier 仅作上下文".to_string(),
        }),
        None => Ok(ConstraintVerdict::Skipped {
            reason: "无 verifier，仅作上下文".to_string(),
        }),
    }
}

fn drives_gate(kind: ConstraintKind) -> bool {
    matches!(kind, ConstraintKind::Invariant | ConstraintKind::Forbidden)
}
