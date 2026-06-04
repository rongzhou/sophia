//! 源码位置（span）。
//!
//! span 信息从 CST 一直保留到 AST / HIR / Semantic IR，用于诊断定位
//! （见 docs/language_implementation.md 第四节、第十四节）。

use tree_sitter::Node;

/// 源码中的一个点：字节偏移 + 行列。
///
/// 行列均为 0 基（与 Tree-sitter 一致）；CLI 呈现时可按需 +1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub byte: usize,
    pub row: usize,
    pub column: usize,
}

/// 源码区间 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Point,
    pub end: Point,
}

impl Span {
    /// 从 Tree-sitter 节点提取 span。
    pub(crate) fn from_node(node: &Node) -> Self {
        let s = node.start_position();
        let e = node.end_position();
        Span {
            start: Point {
                byte: node.start_byte(),
                row: s.row,
                column: s.column,
            },
            end: Point {
                byte: node.end_byte(),
                row: e.row,
                column: e.column,
            },
        }
    }
}
