//! Sophia-Core 语法层。
//!
//! 职责（见 docs/engineering_architecture.md 第三节、docs/language_implementation.md 第三节）：
//! - 绑定 Tree-sitter 生成的 Sophia parser（单一路线，ABI 15）；
//! - 把源码解析为 CST（Concrete Syntax Tree）；
//! - 提供稳定的 `parse_str` API 与 span / 结构化语法错误。
//!
//! 边界纪律（见 docs/engineering_notes.md “代码结构边界”）：
//! - 本 crate 不做 IO、不做 CLI、不做临时诊断；I/O 与呈现由上层（CLI）承担；
//! - 不引入第二条解析路径或特性开关。
//!
//! CST → AST 转换（typed-arena + ID 引用）见 `ast` 与 `lower` 模块。

mod ast;
mod error;
mod lower;
mod span;
mod tree;

pub use ast::{
    AssistField, AssistKey, Ast, BinOp, Block, Callable, CallableKind, Capability, Domain, Effect,
    EffectArg, EffectDef, EffectOperation, EffectParam, ElseBranch, Entity, ErrorDef, ErrorVariant,
    Evolution, Expr, ExprId, FieldDecl, FieldInit, Ident, IncludeDecl, IncludeKind, Invariant,
    Item, MatchArm, Param, Pattern, SemanticIdentity, StateDef, StateValue, Stmt, StrLit, Task,
    TypeRef,
};
pub use error::{SyntaxDiagnostic, SyntaxDiagnosticKind, SyntaxError, SyntaxResult};
pub use span::{Point, Span};
pub use tree::SyntaxTree;

use tree_sitter::Language;

extern "C" {
    fn tree_sitter_sophia() -> Language;
}

/// 返回 Sophia-Core 的 Tree-sitter `Language`。
///
/// 由 `build.rs` 编译的 `src/parser.c` 提供符号 `tree_sitter_sophia`。
/// crate 内部用（`tree.rs` 初始化 parser）——对外入口是 `parse_str` / `parse_ast`。
pub(crate) fn language() -> Language {
    unsafe { tree_sitter_sophia() }
}

/// 把源码解析为 CST。
///
/// 这是语法层对外的唯一稳定入口。返回的 [`SyntaxTree`] 持有源码副本与
/// Tree-sitter 解析树，可进一步查询根节点、span 与语法错误。
///
/// 解析本身不会失败（Tree-sitter 是容错解析器）；语法错误以 ERROR / MISSING
/// 节点的形式保留在树中，由 [`SyntaxTree::errors`] 提取。仅当语言绑定本身
/// 无法设置时返回 [`SyntaxError::LanguageInit`]。
pub fn parse_str(source: impl Into<String>) -> SyntaxResult<SyntaxTree> {
    SyntaxTree::parse(source.into())
}

/// 解析源码并转换为 AST。
///
/// 便捷入口：先 [`parse_str`] 得到 CST，再 lowering 为 [`Ast`]。
/// 即便源码含语法错误也会尽力产出 AST（容错），语法诊断仍可经
/// [`SyntaxTree::errors`] 获取；如需在 AST 前 gate 错误，请改用 [`parse_str`]
/// 自行检查 [`SyntaxTree::errors`]。
pub fn parse_ast(source: impl Into<String>) -> SyntaxResult<Ast> {
    let tree = parse_str(source)?;
    Ok(tree.to_ast())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODO_DOMAIN: &str = include_str!("../examples/TodoDomain.sophia");
    const COMPLETE_TODO: &str = include_str!("../examples/CompleteTodo.sophia");
    const CONTROL: &str = include_str!("../examples/Control.sophia");

    #[test]
    fn language_loads() {
        // ABI 应与 tree-sitter crate 兼容；不兼容会在 set_language 时报错。
        let tree = parse_str("domain D {}").expect("parse");
        assert_eq!(tree.root().kind(), "source_file");
    }

    #[test]
    fn documented_examples_parse_without_errors() {
        for (name, src) in [
            ("TodoDomain", TODO_DOMAIN),
            ("CompleteTodo", COMPLETE_TODO),
            ("Control", CONTROL),
        ] {
            let tree = parse_str(src).expect("parse");
            let errors = tree.errors();
            assert!(
                errors.is_empty(),
                "示例 {name} 不应有语法错误，但得到：{errors:?}"
            );
        }
    }

    #[test]
    fn syntax_error_is_reported_with_span() {
        // 缺少右花括号，制造 ERROR / MISSING 节点。
        let tree = parse_str("entity Broken {").expect("parse");
        let errors = tree.errors();
        assert!(!errors.is_empty(), "残缺源码应产生语法错误");
        // span 必须落在源码范围内。
        for e in &errors {
            assert!(e.span.start.byte <= e.span.end.byte);
        }
    }

    #[test]
    fn top_level_node_kinds_present() {
        let tree = parse_str(TODO_DOMAIN).expect("parse");
        let root = tree.root();
        let mut cursor = root.walk();
        let kinds: Vec<&str> = root.children(&mut cursor).map(|c| c.kind()).collect();
        for expected in [
            "domain_def",
            "entity_def",
            "state_def",
            "transition_def",
            "error_def",
            "capability_def",
            "task_def",
        ] {
            assert!(kinds.contains(&expected), "缺少顶层节点类型 {expected}");
        }
    }

    #[test]
    fn dotted_access_in_expr_is_field_access_not_qualified_name() {
        // 表达式中的点号访问应为 field_access；qualified_name 仅用于 match pattern。
        // 见 docs/language_design.md 第七节、第六节状态值访问语义。
        let tree = parse_str("action A { body { return a.b.c } }").expect("parse");
        let sexp = tree.to_sexp();
        assert!(
            sexp.contains("field_access"),
            "点号访问应解析为 field_access：{sexp}"
        );
        assert!(
            !sexp.contains("qualified_name"),
            "表达式不应出现 qualified_name：{sexp}"
        );
    }

    #[test]
    fn match_pattern_uses_qualified_name() {
        // match pattern 中的状态值（如 TodoStatus.Done）应为 qualified_name。
        let src = "action A { body { match s { TodoStatus.Done => return s } } }";
        let tree = parse_str(src).expect("parse");
        assert!(
            tree.to_sexp().contains("qualified_name"),
            "状态值 pattern 应解析为 qualified_name"
        );
    }

    #[test]
    fn cst_snapshot_is_stable() {
        // CST 作为 snapshot 目标（见 docs/engineering_architecture.md 13.1），
        // 守护 grammar 变更不静默改变解析结构。
        let tree = parse_str(COMPLETE_TODO).expect("parse");
        insta::assert_snapshot!("complete_todo_cst", tree.to_sexp());
    }
}
