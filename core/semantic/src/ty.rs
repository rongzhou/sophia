//! 类型表示。
//!
//! 见 docs/language_design.md 第六节、docs/language_implementation.md 第七节、
//! 第十六节起步子集。`Ty` 是 Semantic IR 内部的规范化类型，区别于语法层的
//! `TypeRef`（后者只是表层引用，未解析 entity/state 归属、未规范化 wrapper）。
//!
//! 设计要点：
//! - Intent 严格相等（7.2）：`Raw<Text>` 不能赋给 `Sanitized<Text>`，也不能降级为 `Text`；
//! - `Unknown` 是渐进类型顶（7.1），与任意类型兼容，运行时退化为动态检查；
//! - `Error` 是类型推导失败的恢复占位，与任意类型兼容，避免级联误报。

use std::fmt;

/// Intent Type 种类（docs/language_design.md 6.2、docs/language_implementation.md 16.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntentKind {
    Raw,
    Parsed,
    Validated,
    Sanitized,
    Verified,
    Authorized,
    Persisted,
    Secret,
    Redacted,
}

impl IntentKind {
    /// 由 wrapper 头名解析 intent 种类。
    pub fn from_head(head: &str) -> Option<Self> {
        Some(match head {
            "Raw" => IntentKind::Raw,
            "Parsed" => IntentKind::Parsed,
            "Validated" => IntentKind::Validated,
            "Sanitized" => IntentKind::Sanitized,
            "Verified" => IntentKind::Verified,
            "Authorized" => IntentKind::Authorized,
            "Persisted" => IntentKind::Persisted,
            "Secret" => IntentKind::Secret,
            "Redacted" => IntentKind::Redacted,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IntentKind::Raw => "Raw",
            IntentKind::Parsed => "Parsed",
            IntentKind::Validated => "Validated",
            IntentKind::Sanitized => "Sanitized",
            IntentKind::Verified => "Verified",
            IntentKind::Authorized => "Authorized",
            IntentKind::Persisted => "Persisted",
            IntentKind::Secret => "Secret",
            IntentKind::Redacted => "Redacted",
        }
    }
}

/// 规范化类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// 标量。
    Unit,
    Bool,
    Int,
    Text,
    /// 不透明内置标量（规范示例使用；起步子集作为原始类型对待）。
    Uuid,
    Time,
    /// `Null`：表达"无"的内置单值类型（典型作 `one of` 成员）。
    Null,
    /// `list of T`。
    List(Box<Ty>),
    /// `one of { M, ... }`：互斥成员的联合（成员两两按 tag 可区分）。
    OneOf(Vec<Ty>),
    /// `schema of T`（渐进类型一等公民，7.1）。
    Schema(Box<Ty>),
    /// `Unknown`（渐进类型顶，与任意类型兼容）。
    Unknown,
    /// 已声明 entity 类型。
    Entity(String),
    /// 已声明 state 类型。
    State(String),
    /// 已声明 error variant（作为 `one of` 成员的"失败结局"类型）。
    ErrorVariant(String),
    /// Intent 包装类型。inner 不应再是 Intent（严格一层）。
    Intent(IntentKind, Box<Ty>),
    /// 输出记录：`ensures` 中 `output` 根的类型，字段为各 output 参数。
    ///
    /// 仅用于谓词作用域内的字段访问（`output.<param>`），不作为可声明类型出现。
    /// 字段按声明顺序保留。
    Record(Vec<(String, Ty)>),
    /// 类型推导失败的恢复占位（与任意类型兼容，错误已单独报告）。
    Error,
}

impl Ty {
    /// 是否为 `Unknown` 或 `Error`（渐进 / 恢复，跳过严格检查）。
    pub fn is_gradual(&self) -> bool {
        matches!(self, Ty::Unknown | Ty::Error)
    }

    /// 是否携带 intent。
    pub fn is_intent(&self) -> bool {
        matches!(self, Ty::Intent(..))
    }

    /// 剥离最外层 intent，返回 inner 类型（无 intent 则返回自身克隆）。
    pub fn strip_intent(&self) -> &Ty {
        match self {
            Ty::Intent(_, inner) => inner,
            other => other,
        }
    }

    /// 赋值相容：`source` 类型的值能否赋给 `target` 类型的位置。
    ///
    /// 规则（docs/language_implementation.md 7.2）：
    /// - 渐进类型（Unknown / Error）任一侧出现即相容；
    /// - Intent **严格相等**：种类与 inner 都必须匹配；intent 与非 intent 不相容；
    /// - `list of` / `schema of` 协变比较 inner；
    /// - `one of`：成员可 upcast 到联合（源是某成员）；联合→联合当目标成员集 ⊇ 源；
    /// - entity / state 按名相等；标量按种类相等。
    pub fn assignable_to(&self, target: &Ty) -> bool {
        if self.is_gradual() || target.is_gradual() {
            return true;
        }
        // `one of` 目标：源是其任一成员（成员 → 联合 upcast），或源也是子集联合。
        if let Ty::OneOf(targets) = target {
            return match self {
                Ty::OneOf(sources) => sources
                    .iter()
                    .all(|s| targets.iter().any(|t| s.assignable_to(t))),
                _ => targets.iter().any(|t| self.assignable_to(t)),
            };
        }
        match (self, target) {
            (Ty::Unit, Ty::Unit)
            | (Ty::Bool, Ty::Bool)
            | (Ty::Int, Ty::Int)
            | (Ty::Text, Ty::Text)
            | (Ty::Uuid, Ty::Uuid)
            | (Ty::Time, Ty::Time)
            | (Ty::Null, Ty::Null) => true,
            (Ty::List(a), Ty::List(b)) | (Ty::Schema(a), Ty::Schema(b)) => a.assignable_to(b),
            (Ty::Entity(a), Ty::Entity(b)) | (Ty::State(a), Ty::State(b)) => a == b,
            (Ty::ErrorVariant(a), Ty::ErrorVariant(b)) => a == b,
            (Ty::Intent(k1, a), Ty::Intent(k2, b)) => k1 == k2 && a.assignable_to(b),
            (Ty::Record(a), Ty::Record(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((n1, t1), (n2, t2))| n1 == n2 && t1.assignable_to(t2))
            }
            // intent 与非 intent 不相容（严格，无隐式降级 / 升级）。
            _ => false,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Unit => write!(f, "Unit"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Int => write!(f, "Int"),
            Ty::Text => write!(f, "Text"),
            Ty::Uuid => write!(f, "Uuid"),
            Ty::Time => write!(f, "Time"),
            Ty::Null => write!(f, "Null"),
            Ty::List(t) => write!(f, "list of {t}"),
            Ty::OneOf(members) => {
                write!(f, "one of {{ ")?;
                for (i, m) in members.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{m}")?;
                }
                write!(f, " }}")
            }
            Ty::Schema(t) => write!(f, "schema of {t}"),
            Ty::Unknown => write!(f, "Unknown"),
            Ty::Entity(n) => write!(f, "{n}"),
            Ty::State(n) => write!(f, "{n}"),
            Ty::ErrorVariant(n) => write!(f, "{n}"),
            Ty::Intent(k, t) => write!(f, "{}<{t}>", k.as_str()),
            Ty::Record(fields) => {
                write!(f, "{{")?;
                for (i, (n, t)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{n}: {t}")?;
                }
                write!(f, "}}")
            }
            Ty::Error => write!(f, "<error>"),
        }
    }
}
