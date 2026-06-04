//! HIR 层错误类型（thiserror）。
//!
//! 错误分两类：
//! - 构建期硬错误（index 构建失败等），用枚举区分；
//! - 名称解析 / scope 诊断（带 span），以 [`HirDiagnostic`] 列表返回，
//!   不阻断后续分析（容错收集，便于一次性反馈多个问题）。

use sophia_syntax::Span;
use thiserror::Error;

/// HIR 层结果别名。
pub type HirResult<T> = Result<T, HirError>;

/// HIR 层硬错误（无法继续）。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HirError {
    /// 一个文件定义了多个顶层 node（违反 5.1 文件布局约束）。
    #[error("文件 `{path}` 定义了 {count} 个顶层 node，但每个文件只能有一个")]
    MultipleTopLevelNodes { path: String, count: usize },

    /// node 文件不含任何顶层 node。
    #[error("文件 `{path}` 不含任何顶层 node")]
    EmptyNodeFile { path: String },

    /// 跨文件出现同名节点（禁止 shadowing）。
    #[error("重复的节点名 `{name}`：`{first_path}` 与 `{second_path}`")]
    DuplicateNode {
        name: String,
        first_path: String,
        second_path: String,
    },

    /// 用户 `effect` 声明与内置 / 库 effect 操作冲突，或同一 effect 内重复声明 operation。
    #[error("effect 操作冲突 `{family}.{op}`：{existing}；冲突声明位于 `{path}`")]
    EffectOpConflict {
        family: String,
        op: String,
        existing: String,
        path: String,
    },

    /// index 序列化失败。
    #[error("ASG index 序列化失败：{0}")]
    Serialization(String),

    /// 库随附 Sophia 源码节点解析失败。
    #[error("库 `{lib}` 源码 `{path}` 解析失败：{reason}")]
    LibrarySourceParse {
        lib: String,
        path: String,
        reason: String,
    },
}

/// HIR 诊断的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirDiagnosticKind {
    /// 引用无法由 ASG index 解析。
    UnresolvedReference,
    /// 引用解析到的节点类型与使用位置不符（如把 entity 当 capability 用）。
    WrongReferenceKind,
    /// 出现被禁止的同名 shadowing（body 局部变量遮蔽可见变量）。
    Shadowing,
    /// 引用了未声明 / 未绑定的局部变量。
    UnresolvedVariable,
    /// 对不可变变量执行 `set`。
    AssignToImmutable,
    /// 跨 domain 引用未通过 task include 显式声明。
    ImplicitCrossDomain,
    /// 引用了未声明的 effect 操作（`Family.Op` 不在内置族或 effect 声明中），
    /// 或实参个数与声明不符。
    UnresolvedEffect,
}

/// 一条 HIR 诊断（携带 span）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirDiagnostic {
    pub kind: HirDiagnosticKind,
    pub span: Span,
    /// 涉及的名字。
    pub name: String,
    /// 面向 LLM / 人的补充说明。
    pub message: String,
}

impl HirDiagnostic {
    pub(crate) fn new(
        kind: HirDiagnosticKind,
        span: Span,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        HirDiagnostic {
            kind,
            span,
            name: name.into(),
            message: message.into(),
        }
    }
}
