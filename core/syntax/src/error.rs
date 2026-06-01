//! 语法层错误类型。
//!
//! 错误处理基线（见 docs/engineering_notes.md 2026-05-28）：库 crate 使用
//! `thiserror` 定义类型化错误，不向公共 API 暴露 `anyhow::Error`。

use crate::span::Span;
use thiserror::Error;

/// 语法层结果别名。
pub type SyntaxResult<T> = Result<T, SyntaxError>;

/// 语法层错误。
///
/// 注意：源码中的语法错误（ERROR / MISSING 节点）不是 `Err`，而是以
/// [`SyntaxDiagnostic`] 列表的形式由 [`crate::SyntaxTree::errors`] 返回，
/// 因为 Tree-sitter 是容错解析器，单个错误不应阻断后续阶段。
/// 这里的 `SyntaxError` 只表示无法继续的硬失败（如语言绑定初始化失败）。
#[derive(Debug, Error)]
pub enum SyntaxError {
    /// Tree-sitter 语言绑定初始化失败（通常是 ABI 不兼容）。
    #[error("无法初始化 Sophia 语言绑定：{0}")]
    LanguageInit(String),
}

/// 一条语法诊断，对应 CST 中的 ERROR 或 MISSING 节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    pub kind: SyntaxDiagnosticKind,
    pub span: Span,
    /// 出错节点的类型名（ERROR 节点为 "ERROR"；MISSING 节点为缺失的符号名）。
    pub node_kind: String,
}

/// 语法诊断的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxDiagnosticKind {
    /// 出现无法解析的 token 序列（ERROR 节点）。
    Error,
    /// 缺失必需的 token（MISSING 节点）。
    Missing,
}
