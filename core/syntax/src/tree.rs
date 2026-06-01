//! CST 包装：持有源码与 Tree-sitter 解析树，提供根节点与语法诊断查询。

use crate::error::{SyntaxDiagnostic, SyntaxDiagnosticKind, SyntaxError, SyntaxResult};
use crate::span::Span;
use tree_sitter::{Node, Parser, Tree};

/// 一棵 Sophia-Core 的 CST，连同其源码。
///
/// 源码以 `String` 内嵌，保证 [`Node`] 的字节区间始终可被解释；
/// 这让语法树自包含，便于上层在不持有原始 buffer 的情况下读取片段。
pub struct SyntaxTree {
    source: String,
    tree: Tree,
}

impl SyntaxTree {
    /// 解析源码为 CST。失败仅发生在语言绑定无法设置时。
    pub(crate) fn parse(source: String) -> SyntaxResult<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&crate::language())
            .map_err(|e| SyntaxError::LanguageInit(e.to_string()))?;
        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| SyntaxError::LanguageInit("parser 返回空树".to_string()))?;
        Ok(SyntaxTree { source, tree })
    }

    /// CST 根节点（`source_file`）。
    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// 取某节点对应的源码片段。
    ///
    /// 节点字节区间来自对本 `source` 的解析，因此 UTF-8 边界恒定有效。
    pub fn text(&self, node: &Node) -> &str {
        node.utf8_text(self.source.as_bytes())
            .expect("节点字节区间源自本源码，必为有效 UTF-8")
    }

    /// CST 的 S 表达式形式，用于快照测试与调试。
    pub fn to_sexp(&self) -> String {
        self.root().to_sexp()
    }

    /// 把 CST 转换为 AST（丢弃 trivia，保留 span）。
    ///
    /// 见 docs/language_implementation.md 第四节。lowering 容错：对部分错误
    /// 输入仍尽力产出 AST，调用方如需 gate 语法错误应先查 [`Self::errors`]。
    pub fn to_ast(&self) -> crate::Ast {
        crate::lower::lower(self.root(), &self.source)
    }

    /// 收集全部语法诊断，按出现顺序（前序遍历）返回。
    ///
    /// 这是容错解析的产物：每个 ERROR / MISSING 节点对应一条诊断，
    /// 由上层（CLI / LSP）渲染为面向 LLM 的结构化错误。遍历顺序确定，
    /// 满足 docs/engineering_notes.md “输出确定性” 要求。
    pub fn errors(&self) -> Vec<SyntaxDiagnostic> {
        let mut out = Vec::new();
        collect_errors(self.root(), &mut out);
        out
    }
}

/// 前序遍历收集 ERROR / MISSING 节点。
///
/// 每个节点用 `node.walk()` 派生与自身同生命周期的游标，避免在递归间
/// 传递可变游标引起的生命周期不变性问题。
fn collect_errors(node: Node, out: &mut Vec<SyntaxDiagnostic>) {
    if node.is_error() {
        out.push(SyntaxDiagnostic {
            kind: SyntaxDiagnosticKind::Error,
            span: Span::from_node(&node),
            node_kind: node.kind().to_string(),
        });
    } else if node.is_missing() {
        out.push(SyntaxDiagnostic {
            kind: SyntaxDiagnosticKind::Missing,
            span: Span::from_node(&node),
            node_kind: node.kind().to_string(),
        });
    }

    // 即便当前节点是 ERROR，也继续深入：错误子树里仍可能含更具体的 MISSING。
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_errors(child, out);
    }
}
