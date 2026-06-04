//! 文档语义分析缓存（协议无关）。
//!
//! 见 docs/engineering_architecture.md 10.3：LSP **基于 semantic data 工作，
//! 而不是直接遍历 AST**。本模块把一组 `.sophia` 文档解析为 AST，构建 ASG index
//! 与符号表（module/symbol cache，10.3 起步实现），并在其上提供 hover / goto /
//! diagnostics 查询。增量分析后续引入；当前每次变更全量重算（接口保持 query 风格）。
//!
//! 本模块不依赖 tower-lsp，便于单元测试；协议投影见 `convert` 与 `server`。

use sophia_hir::{resolve_item, resolve_program, NodeKind, ProgramInput};
use sophia_semantic::{analyze_one_callable, SemanticModel};
use sophia_syntax::{parse_str, Ast, Item, Point, Span, SyntaxDiagnostic, SyntaxTree};
use std::collections::BTreeMap;

/// 一个文档的 URI（不透明字符串键）。
pub type DocUri = String;

/// 诊断来源层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSource {
    Syntax,
    Hir,
    Semantic,
}

/// 统一诊断（跨层），携带 span 与可读信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub source: DiagnosticSource,
    pub span: Span,
    pub code: String,
    pub message: String,
}

/// 一个顶层符号（节点）的定义位置与元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDef {
    pub domain: String,
    pub name: String,
    pub kind: NodeKind,
    pub uri: DocUri,
    /// 定义名标识符的 span（用于 goto 精确定位）。
    pub name_span: Span,
    /// 整个节点的 span。
    pub full_span: Span,
}

/// 单个文档的解析产物。
struct Document {
    uri: DocUri,
    domain: String,
    source: String,
    tree: SyntaxTree,
    ast: Ast,
    syntax_diags: Vec<SyntaxDiagnostic>,
}

/// 工作区：持有全部打开文档，提供跨文件语义查询。
#[derive(Default)]
pub struct Workspace {
    docs: BTreeMap<DocUri, Document>,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace::default()
    }

    /// 打开 / 更新一个文档（全量重解析）。
    ///
    /// `domain` 由 URI 的目录结构推导（CLI / server 层负责）；这里直接接收。
    pub fn upsert(
        &mut self,
        uri: impl Into<DocUri>,
        domain: impl Into<String>,
        source: impl Into<String>,
    ) {
        let uri = uri.into();
        let source = source.into();
        let tree = parse_str(source.clone()).expect("parse 不会硬失败");
        let syntax_diags = tree.errors();
        let ast = tree.to_ast();
        self.docs.insert(
            uri.clone(),
            Document {
                uri,
                domain: domain.into(),
                source,
                tree,
                ast,
                syntax_diags,
            },
        );
    }

    /// 关闭一个文档。
    pub fn remove(&mut self, uri: &str) {
        self.docs.remove(uri);
    }

    /// 文档源码（供位置换算）。
    pub fn source(&self, uri: &str) -> Option<&str> {
        self.docs.get(uri).map(|d| d.source.as_str())
    }

    /// 计算某文档的全部诊断（syntax + hir + semantic），按文档精确归属。
    ///
    /// 语法错误存在时跳过 hir/semantic（它们假定无语法错误，见各层文档）。
    /// HIR 诊断按本文档的每个 item 用 `resolve_item` 单独收集；semantic 诊断按本文档
    /// 的每个 callable 用 `analyze_one_callable` 单独收集——避免跨文档 span 偏移碰撞
    /// （每文档 AST 都是 0 基偏移）。
    pub fn diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        let Some(doc) = self.docs.get(uri) else {
            return Vec::new();
        };

        // 语法诊断优先；存在则不进入语义层。
        let syntax: Vec<Diagnostic> = doc
            .syntax_diags
            .iter()
            .map(|d| Diagnostic {
                source: DiagnosticSource::Syntax,
                span: d.span,
                code: format!("{:?}", d.kind),
                message: format!("语法错误：{}", d.node_kind),
            })
            .collect();
        if !syntax.is_empty() {
            return syntax;
        }

        // 跨文件 index 构建失败（重名 / 一文件多节点）必须作为可见 HIR 诊断返回；
        // 不能退化为空 index，否则会吞掉 CLI check 能看到的 workspace-level 错误。
        let inputs: Vec<ProgramInput> = self
            .docs
            .values()
            .map(|d| ProgramInput {
                domain: &d.domain,
                path: &d.uri,
                ast: &d.ast,
            })
            .collect();
        let registry = sophia_stdlib::standard_registry();
        let index = match resolve_program(&inputs, &registry) {
            Ok((index, _)) => index,
            Err(e) => {
                return vec![Diagnostic {
                    source: DiagnosticSource::Hir,
                    span: workspace_error_span(doc),
                    code: "WorkspaceIndex".to_string(),
                    message: e.to_string(),
                }];
            }
        };

        let mut out = Vec::new();

        // HIR：仅解析本文档的 items（精确归属）。
        for item in &doc.ast.items {
            for d in resolve_item(item, &doc.ast, &index, &doc.domain) {
                out.push(Diagnostic {
                    source: DiagnosticSource::Hir,
                    span: d.span,
                    code: format!("{:?}", d.kind),
                    message: d.message,
                });
            }
        }

        // Semantic：模型从全程序构建（解析跨文件引用），但只取本文档 callable 的诊断。
        let asts: Vec<&Ast> = self.docs.values().map(|d| &d.ast).collect();
        let model = SemanticModel::build(&asts, &index);
        for item in &doc.ast.items {
            let name = match item {
                Item::Action(c) | Item::Transition(c) => &c.name.text,
                _ => continue,
            };
            for d in analyze_one_callable(name, &model, &asts, &index) {
                out.push(Diagnostic {
                    source: DiagnosticSource::Semantic,
                    span: d.span,
                    code: d.code().to_string(),
                    message: d.message,
                });
            }
        }
        out
    }

    /// 工作区全部顶层符号定义（按 `domain::name`）。
    pub fn symbols(&self) -> BTreeMap<String, SymbolDef> {
        let mut map = BTreeMap::new();
        for doc in self.docs.values() {
            for item in &doc.ast.items {
                let name = item.name();
                let key = symbol_key(&doc.domain, &name.text);
                map.insert(
                    key,
                    SymbolDef {
                        domain: doc.domain.clone(),
                        name: name.text.clone(),
                        kind: node_kind_of(item),
                        uri: doc.uri.clone(),
                        name_span: name.span,
                        full_span: item.span(),
                    },
                );
            }
        }
        map
    }

    /// 找到某文档某字节偏移处的标识符文本（用于 hover / goto）。
    pub fn ident_at(&self, uri: &str, byte: usize) -> Option<String> {
        let doc = self.docs.get(uri)?;
        ident_at_byte(&doc.tree, byte)
    }

    /// hover：返回该位置标识符对应符号的说明（基于 semantic data）。
    pub fn hover(&self, uri: &str, byte: usize) -> Option<String> {
        let name = self.ident_at(uri, byte)?;
        let doc = self.docs.get(uri)?;
        let def = self.symbol_in_domain(&doc.domain, &name)?;
        Some(format!(
            "**{}** — {:?}\n\n定义于 `{}`",
            def.name, def.kind, def.uri
        ))
    }

    /// goto definition：返回该位置标识符对应符号的定义位置。
    pub fn goto_definition(&self, uri: &str, byte: usize) -> Option<SymbolDef> {
        let name = self.ident_at(uri, byte)?;
        let doc = self.docs.get(uri)?;
        self.symbol_in_domain(&doc.domain, &name)
    }

    fn symbol_in_domain(&self, domain: &str, name: &str) -> Option<SymbolDef> {
        self.symbols().get(&symbol_key(domain, name)).cloned()
    }
}

/// 取语法树中覆盖给定字节偏移的最内层 `identifier` 节点文本。
fn ident_at_byte(tree: &SyntaxTree, byte: usize) -> Option<String> {
    let root = tree.root();
    let mut node = root.descendant_for_byte_range(byte, byte)?;
    // 向上找到 identifier（或本身即是）。
    loop {
        if node.kind() == "identifier" {
            return Some(tree.text(&node).to_string());
        }
        node = node.parent()?;
    }
}

fn node_kind_of(item: &Item) -> NodeKind {
    NodeKind::of_item(item)
}

fn symbol_key(domain: &str, name: &str) -> String {
    format!("{domain}::{name}")
}

fn workspace_error_span(doc: &Document) -> Span {
    doc.ast.items.first().map(Item::span).unwrap_or(Span {
        start: Point {
            byte: 0,
            row: 0,
            column: 0,
        },
        end: Point {
            byte: 0,
            row: 0,
            column: 0,
        },
    })
}
