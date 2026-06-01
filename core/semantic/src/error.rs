//! Semantic IR 层错误与诊断类型。
//!
//! 编译器诊断携带源码 span（见 docs/language_implementation.md 14.2），
//! 与工作流诊断（携带节点 ID）严格分离。
//!
//! 与 HIR 一致采用容错收集：检查不在首个错误中断，便于一次反馈多个问题。
//! 语义分析**不产生硬错误**（全部以诊断收集），故本层无 `Result`/Error 类型。

use sophia_syntax::Span;

/// 语义诊断的种类，对应 docs/language_implementation.md 7.6 最小检查集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticDiagnosticKind {
    /// 类型不匹配（字段赋值、return、调用实参、容器元素等）。
    TypeMismatch,
    /// intent 严格相等被违反（弱 intent 写入强 intent 等）。
    IntentMismatch,
    /// 非 Unit callable 存在未 return/raise 的路径。
    MissingReturn,
    /// entity 构造缺字段。
    MissingField,
    /// entity 构造含未知字段。
    UnknownField,
    /// 字段访问的字段在该类型上不存在。
    NoSuchField,
    /// body 使用了未在 effects 中声明的 effect（含被调用方 effect 未传播——
    /// 类型层把被调用方 effect 并入 used，故未声明的传播 effect 也由此报出）。
    UndeclaredEffect,
    /// `Pure` 与其他 effect 并存。
    PureConflict,
    /// effect 未被 capability allow，或命中 deny。
    CapabilityDenied,
    /// action 缺少 capability 绑定但产生了 effect。
    MissingCapability,
    /// `raise` 的 variant 未在 errors 中声明。
    UndeclaredError,
    /// 被调用方 errors 未被调用方继续声明。
    ErrorNotPropagated,
    /// match 分支不穷尽。
    NonExhaustiveMatch,
    /// match 在不可匹配的类型上（非 Bool / state / `one of`）。
    InvalidMatchSubject,
    /// `one of` 的成员两两不可按 match tag 区分（设计 §2.2）。
    IndistinguishableUnion,
    /// `Console.Write` 输出了非字面量 / 非 Sanitized / 非 Redacted 的值。
    ConsoleOutputIntent,
    /// intent conversion action 不满足结构约束。
    InvalidIntentConversion,
}

impl SemanticDiagnosticKind {
    /// 稳定诊断码（面向 LLM 修复循环，见 14.3）。
    pub fn code(self) -> &'static str {
        use SemanticDiagnosticKind as K;
        match self {
            K::TypeMismatch => "CHECK-TYPE-001",
            K::IntentMismatch => "CHECK-INTENT-001",
            K::MissingReturn => "CHECK-TYPE-002",
            K::MissingField => "CHECK-TYPE-003",
            K::UnknownField => "CHECK-TYPE-004",
            K::NoSuchField => "CHECK-TYPE-005",
            K::UndeclaredEffect => "CHECK-EFFECT-001",
            K::PureConflict => "CHECK-EFFECT-003",
            K::CapabilityDenied => "CHECK-CAP-001",
            K::MissingCapability => "CHECK-CAP-002",
            K::UndeclaredError => "CHECK-ERROR-001",
            K::ErrorNotPropagated => "CHECK-ERROR-002",
            K::NonExhaustiveMatch => "CHECK-ERROR-003",
            K::InvalidMatchSubject => "CHECK-ERROR-004",
            K::IndistinguishableUnion => "CHECK-TYPE-006",
            K::ConsoleOutputIntent => "CHECK-INTENT-002",
            K::InvalidIntentConversion => "CHECK-INTENT-003",
        }
    }
}

/// 一条语义诊断（携带 span）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    pub kind: SemanticDiagnosticKind,
    pub span: Span,
    pub message: String,
}

impl SemanticDiagnostic {
    pub(crate) fn new(
        kind: SemanticDiagnosticKind,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        SemanticDiagnostic {
            kind,
            span,
            message: message.into(),
        }
    }

    /// 稳定诊断码。
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}
