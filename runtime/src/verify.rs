//! Hidden-case verifier 执行器（constraint audit 的 regression gate 执行侧）。
//!
//! 见 docs/workflow_graph_spec.md 4.1.2（ConstraintNode + verifier kind=HiddenCase）、
//! 第七节接入点 4，以及 docs/language_design.md 4.1.2。`tools/audit` 是**纯判定层**：它消费
//! 注入的 `VerifierOutcome`（pass/fail + detail），不执行任何代码。本模块是**执行侧**：把一个
//! hidden case（入口 action + 实参 + 期望结局）在 v0 解释器上真正跑一遍，得到 pass/fail。
//!
//! 分层（architecture §3.2 / §3.3）：执行属 `runtime` 职责（解释器是唯一执行后端）；`runtime`
//! **不依赖 `tools/audit`**（不反向依赖 tools 判定层）。本模块产出 runtime 原生的
//! [`VerificationResult`]，由编排层（CLI / engine）映射为 `sophia_audit::VerifierOutcome` 注入
//! 审计——与「tools 消费注入报告、执行侧不感知判定图」的注入模式一致。
//!
//! **诚实性**：hidden case 的期望**不**喂给被验证程序（防答案泄漏）——它只在执行**之后**与
//! 实际结局比对。runner 真正执行、真正比对，绝不伪造通过。

use crate::value::{RaisedError, Value};
use crate::{run_action, Outcome};
use serde::{Deserialize, Serialize};
use sophia_semantic::SemanticModel;
use sophia_syntax::Ast;

/// 一个 hidden case 的期望结局（与实际执行结果比对）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ExpectedOutcome {
    /// 期望正常返回某值。
    Returns(Value),
    /// 期望 raise 某 variant（只比对 variant tag，字段不强制——足以验证错误代数路径）。
    Raises(String),
}

/// 一个 hidden case：入口 action + 实参 + 期望结局。
///
/// `verifier_ref` 对应 `ConstraintPayload.verifier.ref`（hidden case 的引用名），用于把执行
/// 结果与具体约束的 verifier 配对（审计层按 `verifier_ref` 查找 outcome）。
///
/// 可序列化（serde）：作为隐藏存储 `sophia-runs/verifiers/hidden.json` 的条目格式
/// （见 workflow_graph_spec.md 五A 节 `HiddenVerifierSpec`）。**绝不进 Development Graph、
/// 绝不进 active context**——只有确定性 gate 在 materialize 时按 ref 加载取用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiddenCase {
    /// verifier 引用名（= ConstraintNode 的 verifier.ref）。
    #[serde(rename = "ref")]
    pub verifier_ref: String,
    /// 入口 action / transition 名。
    pub entry_action: String,
    /// 实参（按 input 顺序）。
    pub args: Vec<Value>,
    /// 期望结局。
    pub expected: ExpectedOutcome,
}

/// 一个 hidden case 的执行结果（runtime 原生，不依赖 audit 判定层）。
///
/// 字段形状与 `sophia_audit::VerifierOutcome` 对齐，便于编排层零损耗映射注入审计。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationResult {
    /// 对应的 verifier 引用名。
    pub verifier_ref: String,
    /// 是否通过（实际结局与期望一致）。
    pub passed: bool,
    /// 说明（通过 / 不匹配 / 执行硬错误的详情）。
    pub detail: String,
}

/// 在 v0 解释器上执行一个 hidden case，返回 pass/fail 结果。
///
/// 流程：`run_action`（解释执行）→ 实际结局与 `expected` 比对。
/// - 执行成功且结局匹配 → `passed=true`；
/// - 执行成功但结局不匹配 → `passed=false`（detail 记实际 vs 期望）；
/// - 执行本身硬错误（validation 等）→ `passed=false`（detail 记错误；不把硬错误当通过）。
///
/// **绝不伪造**：任何无法确认匹配的情形都判 `passed=false`，detail 如实记录原因。
pub fn run_hidden_case(
    model: &SemanticModel,
    asts: &[&Ast],
    case: &HiddenCase,
) -> VerificationResult {
    let result = run_action(model, asts, &case.entry_action, case.args.clone());
    let (passed, detail) = match result {
        Ok(execution) => match_outcome(&case.expected, &execution.outcome),
        Err(e) => (false, format!("执行 `{}` 硬错误：{e}", case.entry_action)),
    };
    VerificationResult {
        verifier_ref: case.verifier_ref.clone(),
        passed,
        detail,
    }
}

/// 批量执行 hidden cases（顺序无关，结果按入参顺序）。
pub fn run_hidden_cases(
    model: &SemanticModel,
    asts: &[&Ast],
    cases: &[HiddenCase],
) -> Vec<VerificationResult> {
    cases
        .iter()
        .map(|c| run_hidden_case(model, asts, c))
        .collect()
}

/// 比对期望与实际结局，返回 `(passed, detail)`。
fn match_outcome(expected: &ExpectedOutcome, actual: &Outcome) -> (bool, String) {
    match (expected, actual) {
        (ExpectedOutcome::Returns(want), Outcome::Returned(got)) => {
            if want == got {
                (true, format!("返回值匹配：{got}"))
            } else {
                (false, format!("返回值不匹配：实际 {got}，期望 {want}"))
            }
        }
        (ExpectedOutcome::Raises(want_variant), Outcome::Raised(RaisedError { variant, .. })) => {
            if want_variant == variant {
                (true, format!("raise variant 匹配：{variant}"))
            } else {
                (
                    false,
                    format!("raise variant 不匹配：实际 {variant}，期望 {want_variant}"),
                )
            }
        }
        (ExpectedOutcome::Returns(want), Outcome::Raised(e)) => {
            (false, format!("期望返回 {want}，实际 raise {}", e.variant))
        }
        (ExpectedOutcome::Raises(want), Outcome::Returned(got)) => {
            (false, format!("期望 raise {want}，实际返回 {got}"))
        }
    }
}
