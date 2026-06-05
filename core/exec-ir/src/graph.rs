//! Execution Graph IR 的图结构。
//!
//! 见 docs/language_implementation.md 第八节。Execution Graph IR 显式描述运行时
//! 执行结构，是 Semantic IR 与 Runtime 之间的桥梁（流水线
//! `Semantic IR → Execution Graph IR → Interpreter`，9.2）。
//!
//! 起步子集（§16）没有并发 / await / retry，因此执行图退化为：每个 callable 一个
//! 执行节点；callable body 中对其他 action/transition 的调用形成 `Control` 调用边。
//! 节点内部的语句级执行（body 子语言）由解释器消费 AST + Semantic 元信息完成，
//! 不在图上展开。并发、流式、fallback 等更丰富的边语义随后续阶段在此扩展。

use crate::edge::EdgeKind;
use sophia_semantic::SemanticModel;
use sophia_syntax::{Ast, Block, ElseBranch, Expr, ExprId, Item, Stmt};

/// 执行节点稳定 ID（`u32` 索引，只增不复用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExecNodeId(u32);

impl ExecNodeId {
    /// 稳定数值表示（用于 trace / 日志展示）。
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// 作为 `nodes()` 切片下标使用的稳定索引。
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// 执行边稳定 ID（`u32` 索引，只增不复用）。
///
/// 见 docs/language_implementation.md 9.4：trace 投影需要稳定引用图中的具体边
/// （`ExecutionSpan.edge_id`）。边按构建顺序分配 ID，与 `edges()` 的下标一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExecEdgeId(u32);

impl ExecEdgeId {
    /// 稳定数值表示（用于 trace / 日志展示）。
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// 作为 `edges()` 切片下标使用的稳定索引。
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// 执行节点的种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecNodeKind {
    /// 一个 action 的执行入口（按名引用 Semantic 模型中的 callable）。
    Action(String),
    /// 一个 transition 的执行入口（纯函数）。
    Transition(String),
}

/// 执行图节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecNode {
    id: ExecNodeId,
    kind: ExecNodeKind,
}

impl ExecNode {
    /// 节点稳定 ID。
    pub fn id(&self) -> ExecNodeId {
        self.id
    }

    /// 节点种类。
    pub fn kind(&self) -> &ExecNodeKind {
        &self.kind
    }

    /// 节点对应的 callable 名。
    pub fn name(&self) -> &str {
        match &self.kind {
            ExecNodeKind::Action(n) | ExecNodeKind::Transition(n) => n,
        }
    }
}

/// 执行图边。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecEdge {
    id: ExecEdgeId,
    from: ExecNodeId,
    to: ExecNodeId,
    kind: EdgeKind,
}

impl ExecEdge {
    /// 边稳定 ID。
    pub fn id(&self) -> ExecEdgeId {
        self.id
    }

    /// 边起点节点 ID。
    pub fn from(&self) -> ExecNodeId {
        self.from
    }

    /// 边终点节点 ID。
    pub fn to(&self) -> ExecNodeId {
        self.to
    }

    /// 边种类。
    pub fn kind(&self) -> EdgeKind {
        self.kind
    }
}

/// Execution Graph IR。
///
/// 节点按 callable 名字典序构建，保证产出确定（输出确定性要求）。
#[derive(Debug, Clone, Default)]
pub struct ExecGraph {
    nodes: Vec<ExecNode>,
    edges: Vec<ExecEdge>,
}

impl ExecGraph {
    pub fn new() -> Self {
        ExecGraph::default()
    }

    /// 从 Semantic 声明模型 + 全程序 AST 构建执行图。
    ///
    /// - 每个 callable（action / transition）→ 一个执行节点（按名字典序）；
    /// - callable body 中对其它 action/transition 的调用 → 一条 `Control` 调用边
    ///   （`Expr::Call` 的 callee，或 transition 的构造式调用 `Name { ... }`）。
    ///
    /// 起步子集无需更丰富的调度边；并发 / 流式 / fallback 等随后续阶段扩展。
    pub fn from_model(model: &SemanticModel, asts: &[&Ast]) -> Self {
        let mut graph = ExecGraph::new();
        // 1) 建 callable 节点（BTreeMap 已按名排序，遍历顺序确定）。
        for (name, decl) in &model.callables {
            let kind = match decl.kind {
                sophia_syntax::CallableKind::Action => ExecNodeKind::Action(name.clone()),
                sophia_syntax::CallableKind::Transition => ExecNodeKind::Transition(name.clone()),
            };
            let id = ExecNodeId(graph.nodes.len() as u32);
            graph.nodes.push(ExecNode { id, kind });
        }
        // 2) 建调用边：扫描每个 callable 的 body，收集对其它 callable 的调用。
        //    按 (caller 名, callee 名) 字典序去重，保证边集确定。
        let mut edges: std::collections::BTreeSet<(String, String)> =
            std::collections::BTreeSet::new();
        for ast in asts {
            for item in &ast.items {
                let (Item::Action(c) | Item::Transition(c)) = item else {
                    continue;
                };
                let Some(body) = &c.body else { continue };
                let mut callees = Vec::new();
                collect_callees(body, ast, &mut callees);
                for callee in callees {
                    // 仅对解析为 callable（action/transition）的调用建边。
                    if model.callables.contains_key(&callee) && callee != c.name.text {
                        edges.insert((c.name.text.clone(), callee));
                    }
                }
            }
        }
        for (caller, callee) in edges {
            if let (Some(from), Some(to)) = (
                graph.node_id_by_name(&caller),
                graph.node_id_by_name(&callee),
            ) {
                graph.add_edge(from, to, EdgeKind::Control);
            }
        }
        graph
    }

    /// 全部节点。
    pub fn nodes(&self) -> &[ExecNode] {
        &self.nodes
    }

    /// 全部边。
    pub fn edges(&self) -> &[ExecEdge] {
        &self.edges
    }

    /// 按名查节点。
    pub fn node_by_name(&self, name: &str) -> Option<&ExecNode> {
        self.nodes.iter().find(|n| n.name() == name)
    }

    /// 按名查节点 ID。
    pub fn node_id_by_name(&self, name: &str) -> Option<ExecNodeId> {
        self.node_by_name(name).map(|n| n.id)
    }

    /// 是否存在某 callable 节点。
    pub fn has_node(&self, name: &str) -> bool {
        self.node_by_name(name).is_some()
    }

    /// 是否存在从 `caller` 到 `callee` 的调用边（`Control`）。
    pub fn has_call_edge(&self, caller: &str, callee: &str) -> bool {
        let (Some(from), Some(to)) = (self.node_id_by_name(caller), self.node_id_by_name(callee))
        else {
            return false;
        };
        self.edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == EdgeKind::Control)
    }

    /// 查从 `caller` 到 `callee` 的 `Control` 调用边 ID（trace 投影用，9.4）。
    pub fn call_edge_id(&self, caller: &str, callee: &str) -> Option<ExecEdgeId> {
        let (from, to) = (self.node_id_by_name(caller)?, self.node_id_by_name(callee)?);
        self.edges
            .iter()
            .find(|e| e.from == from && e.to == to && e.kind == EdgeKind::Control)
            .map(|e| e.id)
    }

    /// 按 ID 查边。
    pub fn edge(&self, id: ExecEdgeId) -> Option<&ExecEdge> {
        self.edges.iter().find(|e| e.id == id)
    }

    /// 增加一条边（校验两端节点存在），返回其稳定 ID。
    fn add_edge(&mut self, from: ExecNodeId, to: ExecNodeId, kind: EdgeKind) -> ExecEdgeId {
        debug_assert!(
            self.nodes.iter().any(|n| n.id == from) && self.nodes.iter().any(|n| n.id == to),
            "边的两端节点必须存在"
        );
        let id = ExecEdgeId(self.edges.len() as u32);
        self.edges.push(ExecEdge { id, from, to, kind });
        id
    }
}

/// 收集一个 block 中对其它 callable 的调用名（`Call` 的 callee 与 `Construct` 的 name）。
fn collect_callees(block: &Block, ast: &Ast, out: &mut Vec<String>) {
    for stmt in &block.stmts {
        collect_stmt_callees(stmt, ast, out);
    }
}

fn collect_stmt_callees(stmt: &Stmt, ast: &Ast, out: &mut Vec<String>) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Raise { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => collect_expr_callees(*value, ast, out),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            collect_expr_callees(*condition, ast, out);
            collect_callees(consequence, ast, out);
            match alternative {
                Some(ElseBranch::Block(b)) => collect_callees(b, ast, out),
                Some(ElseBranch::If(s)) => collect_stmt_callees(s, ast, out),
                None => {}
            }
        }
        Stmt::Match { subject, arms, .. } => {
            collect_expr_callees(*subject, ast, out);
            for arm in arms {
                collect_callees(&arm.body, ast, out);
            }
        }
        Stmt::Repeat { count, body, .. } => {
            collect_expr_callees(*count, ast, out);
            collect_callees(body, ast, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr_callees(*condition, ast, out);
            collect_callees(body, ast, out);
        }
    }
}

fn collect_expr_callees(id: ExprId, ast: &Ast, out: &mut Vec<String>) {
    match ast.expr(id) {
        Expr::Call { callee, args, .. } => {
            out.push(callee.text.clone());
            for &a in args {
                collect_expr_callees(a, ast, out);
            }
        }
        Expr::Construct { name, fields, .. } => {
            // transition 经构造式语法调用（`Name { ... }`）。
            out.push(name.text.clone());
            for fi in fields {
                collect_expr_callees(fi.value, ast, out);
            }
        }
        Expr::MethodCall { base, args, .. } => {
            collect_expr_callees(*base, ast, out);
            for &a in args {
                collect_expr_callees(a, ast, out);
            }
        }
        Expr::Field { base, .. } => collect_expr_callees(*base, ast, out),
        Expr::List { items, .. } => {
            for &it in items {
                collect_expr_callees(it, ast, out);
            }
        }
        Expr::Not { operand, .. } => collect_expr_callees(*operand, ast, out),
        Expr::Neg { operand, .. } => collect_expr_callees(*operand, ast, out),
        Expr::Binary { left, right, .. } => {
            collect_expr_callees(*left, ast, out);
            collect_expr_callees(*right, ast, out);
        }
        Expr::Str(_)
        | Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Null { .. }
        | Expr::Ident(_) => {}
    }
}
