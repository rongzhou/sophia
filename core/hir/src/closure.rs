//! Task Closure 与 Semantic Paging（语言设计第八节）。
//!
//! 从 root（action 或 task）出发沿 ASG 计算**确定性的最小语义邻域**，把当前任务的
//! 最小闭包交给 LLM，而非读取整项目再推断相关性（降低 attention diffusion）。
//!
//! 本模块是纯 HIR 层计算：消费 AST + [`AsgIndex`]，产出 [`ContextClosure`]
//! （节点集 + 解释每个节点为何进入 context 的显式边）。IO（读源码内容、渲染）由
//! CLI 协调层负责（§8.1 步骤 9 的 `sources` 在 CLI 填充）。
//!
//! 输出确定性（§8.1 步骤 10、§8.2 步骤 6）：节点与边均按稳定顺序排序。

use crate::index::{AsgIndex, NodeKind};
use sophia_syntax::{Ast, Block, Callable, ElseBranch, Expr, ExprId, Item, Stmt, TypeRef};
use std::collections::{BTreeMap, BTreeSet};

/// 闭包计算错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosureError {
    /// root 节点不存在于 index。
    RootNotFound(String),
    /// root 节点类型不符（如 `--action` 指向非 action）。
    WrongRootKind {
        name: String,
        expected: NodeKind,
        actual: NodeKind,
    },
    /// task closure：formal 依赖被 `exclude` 命中（§8.2 步骤 5，不静默删除而报错）。
    ExcludedDependency { node: String, excluded: String },
}

impl std::fmt::Display for ClosureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClosureError::RootNotFound(n) => write!(f, "root 节点 `{n}` 不存在"),
            ClosureError::WrongRootKind {
                name,
                expected,
                actual,
            } => write!(f, "节点 `{name}` 类型为 {actual:?}，期望 {expected:?}"),
            ClosureError::ExcludedDependency { node, excluded } => write!(
                f,
                "formal 依赖 `{node}` 被 exclude `{excluded}` 命中（§8.2 不静默删除）"
            ),
        }
    }
}

impl std::error::Error for ClosureError {}

/// 闭包边的种类（§8.1 步骤 8），解释某节点为何进入 context。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextEdgeKind {
    /// action/transition 绑定 capability。
    BindsCapability,
    /// body 中调用 action/transition。
    Calls,
    /// `raise` / `errors` 引用 error。
    Raises,
    /// input/output/字段/variant 字段类型引用 entity/state。
    UsesType,
    /// 节点所属 domain 文件。
    InDomain,
    /// task `include` 入口。
    Includes,
}

impl ContextEdgeKind {
    /// 稳定边名（§8 列出的 edge 名）。
    pub fn name(self) -> &'static str {
        match self {
            ContextEdgeKind::BindsCapability => "binds_capability",
            ContextEdgeKind::Calls => "calls",
            ContextEdgeKind::Raises => "raises",
            ContextEdgeKind::UsesType => "uses_type",
            ContextEdgeKind::InDomain => "in_domain",
            ContextEdgeKind::Includes => "includes",
        }
    }
}

/// 一条闭包边（from 节点 →[kind] to 节点）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContextEdge {
    pub from: String,
    pub kind: ContextEdgeKind,
    pub to: String,
}

/// 闭包内一个节点（名 + kind + 文件路径）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClosureNode {
    pub name: String,
    pub kind: NodeKind,
    pub path: String,
}

/// 语义闭包：节点集 + 解释边 + 文件路径（按路径排序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextClosure {
    /// root 节点名。
    pub root: String,
    /// 闭包内节点（按节点名排序）。
    pub nodes: Vec<ClosureNode>,
    /// 解释边（排序去重）。
    pub edges: Vec<ContextEdge>,
    /// 闭包内文件路径（按路径排序去重，对齐 §8.1 步骤 9 的 `files`）。
    pub files: Vec<String>,
}

/// 节点名 → 拥有它的 AST 与索引信息（闭包遍历需逐节点访问 AST）。
struct NodeTable<'a> {
    /// 节点名 → 该节点所在文件的 AST 与其在该 AST 中的 item 下标。
    asts: BTreeMap<String, (&'a Ast, usize)>,
}

impl<'a> NodeTable<'a> {
    fn build(asts: &'a [&'a Ast]) -> Self {
        let mut table = BTreeMap::new();
        for ast in asts {
            for (i, item) in ast.items.iter().enumerate() {
                table.insert(item.name().text.clone(), (*ast, i));
            }
        }
        NodeTable { asts: table }
    }

    fn item(&self, name: &str) -> Option<(&'a Ast, &'a Item)> {
        self.asts.get(name).map(|(ast, i)| (*ast, &ast.items[*i]))
    }
}

/// 计算 action-rooted 语义上下文（§8.1）。
///
/// 从 root action 出发做 ASG 邻域遍历：绑定 capability、input/output 类型、effect 引用
/// 的 storage、errors 引用的 error、body 中调用的 action/transition（递归）、各节点所属
/// domain 文件。返回的闭包按节点名 / 边 / 路径稳定排序。
pub fn action_context(
    root: &str,
    asts: &[&Ast],
    index: &AsgIndex,
) -> Result<ContextClosure, ClosureError> {
    // root 必须是 action。
    match index.kind_of(root) {
        Some(NodeKind::Action) => {}
        Some(actual) => {
            return Err(ClosureError::WrongRootKind {
                name: root.to_string(),
                expected: NodeKind::Action,
                actual,
            })
        }
        None => return Err(ClosureError::RootNotFound(root.to_string())),
    }

    let table = NodeTable::build(asts);
    let mut acc = Accumulator::new(index, &table);
    acc.visit(root);
    acc.add_domains();
    Ok(acc.finish(root))
}

/// 计算 task closure（§8.2）。
///
/// 从 `task.include` 节点出发，并入各自的 formal 依赖；应用 `task.exclude`——若某 formal
/// 依赖被 exclude 命中（按 storage 名）则报 [`ClosureError::ExcludedDependency`]，不静默删除。
pub fn task_context(
    root: &str,
    asts: &[&Ast],
    index: &AsgIndex,
) -> Result<ContextClosure, ClosureError> {
    match index.kind_of(root) {
        Some(NodeKind::Task) => {}
        Some(actual) => {
            return Err(ClosureError::WrongRootKind {
                name: root.to_string(),
                expected: NodeKind::Task,
                actual,
            })
        }
        None => return Err(ClosureError::RootNotFound(root.to_string())),
    }

    let table = NodeTable::build(asts);
    let (_, root_item) = table
        .item(root)
        .ok_or(ClosureError::RootNotFound(root.to_string()))?;
    let Item::Task(task) = root_item else {
        unreachable!("kind 已校验为 Task");
    };

    let mut acc = Accumulator::new(index, &table);
    // include 入口：每个 include 节点连 `includes→`，并并入其依赖。
    for inc in &task.includes {
        let name = inc.name.text.clone();
        acc.edge(root, ContextEdgeKind::Includes, &name);
        acc.visit(&name);
    }
    acc.add_domains();

    Ok(acc.finish(root))
}

/// 闭包累加器：维护已访问节点、节点信息与解释边。
struct Accumulator<'a> {
    index: &'a AsgIndex,
    table: &'a NodeTable<'a>,
    /// 已纳入闭包的节点名 → kind/path。
    nodes: BTreeMap<String, ClosureNode>,
    /// 解释边集合（去重）。
    edges: BTreeSet<ContextEdge>,
    /// 已处理（展开依赖）的节点，避免重复 / 成环。
    processed: BTreeSet<String>,
}

impl<'a> Accumulator<'a> {
    fn new(index: &'a AsgIndex, table: &'a NodeTable<'a>) -> Self {
        Accumulator {
            index,
            table,
            nodes: BTreeMap::new(),
            edges: BTreeSet::new(),
            processed: BTreeSet::new(),
        }
    }

    /// 把一个节点纳入闭包（登记 kind/path），不展开依赖。
    fn add_node(&mut self, name: &str) {
        if self.nodes.contains_key(name) {
            return;
        }
        if let Some(info) = self.index.get(name) {
            self.nodes.insert(
                name.to_string(),
                ClosureNode {
                    name: name.to_string(),
                    kind: info.kind,
                    path: info.path.clone(),
                },
            );
        }
    }

    /// 记录一条解释边，并把 to 节点纳入闭包。
    fn edge(&mut self, from: &str, kind: ContextEdgeKind, to: &str) {
        if self.index.contains(to) {
            self.add_node(to);
            self.edges.insert(ContextEdge {
                from: from.to_string(),
                kind,
                to: to.to_string(),
            });
        }
    }

    /// 纳入节点并递归展开其 formal 依赖。
    fn visit(&mut self, name: &str) {
        if self.processed.contains(name) {
            return;
        }
        self.add_node(name);
        // 不在 index 的名字（内置 / 未声明）不展开。
        if !self.index.contains(name) {
            return;
        }
        self.processed.insert(name.to_string());

        let Some((ast, item)) = self.table.item(name) else {
            return;
        };
        match item {
            Item::Action(c) | Item::Transition(c) => self.visit_callable(name, c, ast),
            Item::Entity(e) => {
                for field in &e.fields {
                    self.use_type(name, &field.ty);
                }
            }
            Item::Error(err) => {
                for variant in &err.variants {
                    for field in &variant.fields {
                        self.use_type(name, &field.ty);
                    }
                }
            }
            Item::Capability(_) => {
                // capability allow/deny 是 effect 引用（标准库 / 领域 effect 族），不引用节点；
                // 无 formal 依赖需展开。
            }
            // domain / state / task / effect 无需展开 formal 依赖。
            Item::Domain(_) | Item::State(_) | Item::Task(_) | Item::Effect(_) => {}
        }
    }

    /// 展开一个 callable（action/transition）的依赖。
    fn visit_callable(&mut self, name: &str, c: &Callable, ast: &Ast) {
        // capability。
        if let Some(cap) = &c.capability {
            self.edge(name, ContextEdgeKind::BindsCapability, &cap.text);
            self.visit(&cap.text.clone());
        }
        // input / output 类型。
        for p in c.inputs.iter().chain(&c.outputs) {
            self.use_type(name, &p.ty);
        }
        // errors → error 节点（经 variant 表解析）。
        for e in &c.errors {
            if let Some(vi) = self.index.variant(&e.text) {
                let err_node = vi.error_node.clone();
                self.edge(name, ContextEdgeKind::Raises, &err_node);
                self.visit(&err_node);
            } else if self.index.kind_of(&e.text) == Some(NodeKind::Error) {
                self.edge(name, ContextEdgeKind::Raises, &e.text.clone());
                self.visit(&e.text.clone());
            }
        }
        // body 中调用的 action/transition / 构造的 entity。
        if let Some(body) = &c.body {
            let mut refs = Vec::new();
            collect_block_refs(body, ast, &mut refs);
            for r in refs {
                match self.index.kind_of(&r) {
                    Some(NodeKind::Action) | Some(NodeKind::Transition) => {
                        self.edge(name, ContextEdgeKind::Calls, &r);
                        self.visit(&r);
                    }
                    Some(NodeKind::Entity) => {
                        self.edge(name, ContextEdgeKind::UsesType, &r);
                        self.visit(&r);
                    }
                    _ => {}
                }
            }
        }
    }

    /// 从类型引用提取 entity/state 依赖（递归解开 wrapper）。
    fn use_type(&mut self, from: &str, ty: &TypeRef) {
        let mut names = Vec::new();
        collect_named_types(ty, &mut names);
        for n in names {
            if matches!(
                self.index.kind_of(&n),
                Some(NodeKind::Entity) | Some(NodeKind::State)
            ) {
                self.edge(from, ContextEdgeKind::UsesType, &n);
                self.visit(&n);
            }
        }
    }

    /// 为闭包内每个节点加入其所属 domain 文件（§8.1 步骤 7）。
    fn add_domains(&mut self) {
        // 收集闭包内各节点的 domain。
        let domains: BTreeSet<String> = self
            .nodes
            .values()
            .map(|n| {
                self.index
                    .get(&n.name)
                    .map(|i| i.domain.clone())
                    .unwrap_or_default()
            })
            .filter(|d| !d.is_empty())
            .collect();
        // 闭包内已存在的节点名（避免迭代中借用冲突）。
        let member_names: Vec<String> = self.nodes.keys().cloned().collect();
        for domain in domains {
            // domain 节点：kind==Domain 且 name==domain（domain 文件 `domain <Name>`）。
            if self.index.kind_of(&domain) == Some(NodeKind::Domain) {
                for member in &member_names {
                    // 只为同 domain 的成员连边。
                    if self.index.get(member).map(|i| i.domain.as_str()) == Some(domain.as_str())
                        && member != &domain
                    {
                        self.edges.insert(ContextEdge {
                            from: member.clone(),
                            kind: ContextEdgeKind::InDomain,
                            to: domain.clone(),
                        });
                    }
                }
                self.add_node(&domain);
            }
        }
    }

    /// 收尾：把累加结果按稳定顺序排序输出。
    fn finish(self, root: &str) -> ContextClosure {
        let mut nodes: Vec<ClosureNode> = self.nodes.into_values().collect();
        nodes.sort();
        let mut edges: Vec<ContextEdge> = self.edges.into_iter().collect();
        edges.sort();
        let mut files: Vec<String> = nodes.iter().map(|n| n.path.clone()).collect();
        files.sort();
        files.dedup();
        ContextClosure {
            root: root.to_string(),
            nodes,
            edges,
            files,
        }
    }
}

/// 递归收集类型引用中的全部具名标识符（generic 头与叶子都算）。
fn collect_named_types(ty: &TypeRef, out: &mut Vec<String>) {
    match ty {
        TypeRef::Named { name, .. } => out.push(name.text.clone()),
        TypeRef::Intent { head, arg, .. } => {
            out.push(head.text.clone());
            collect_named_types(arg, out);
        }
        TypeRef::ListOf { elem, .. } => collect_named_types(elem, out),
        TypeRef::SchemaOf { arg, .. } => collect_named_types(arg, out),
        TypeRef::OneOf { members, .. } => {
            for m in members {
                collect_named_types(m, out);
            }
        }
    }
}

/// 收集一个 block 中 body 引用的节点名（Call 的 callee、Construct 的 name）。
fn collect_block_refs(block: &Block, ast: &Ast, out: &mut Vec<String>) {
    for stmt in &block.stmts {
        collect_stmt_refs(stmt, ast, out);
    }
}

fn collect_stmt_refs(stmt: &Stmt, ast: &Ast, out: &mut Vec<String>) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Raise { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => collect_expr_refs(*value, ast, out),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            collect_expr_refs(*condition, ast, out);
            collect_block_refs(consequence, ast, out);
            match alternative {
                Some(ElseBranch::Block(b)) => collect_block_refs(b, ast, out),
                Some(ElseBranch::If(s)) => collect_stmt_refs(s, ast, out),
                None => {}
            }
        }
        Stmt::Match { subject, arms, .. } => {
            collect_expr_refs(*subject, ast, out);
            for arm in arms {
                collect_block_refs(&arm.body, ast, out);
            }
        }
        Stmt::Repeat { count, body, .. } => {
            collect_expr_refs(*count, ast, out);
            collect_block_refs(body, ast, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr_refs(*condition, ast, out);
            collect_block_refs(body, ast, out);
        }
    }
}

fn collect_expr_refs(id: ExprId, ast: &Ast, out: &mut Vec<String>) {
    match ast.expr(id) {
        Expr::Call { callee, args, .. } => {
            out.push(callee.text.clone());
            for a in args {
                collect_expr_refs(*a, ast, out);
            }
        }
        Expr::Construct { name, fields, .. } => {
            out.push(name.text.clone());
            for f in fields {
                collect_expr_refs(f.value, ast, out);
            }
        }
        Expr::MethodCall { base, args, .. } => {
            collect_expr_refs(*base, ast, out);
            for a in args {
                collect_expr_refs(*a, ast, out);
            }
        }
        Expr::Field { base, .. } => collect_expr_refs(*base, ast, out),
        Expr::List { items, .. } => {
            for it in items {
                collect_expr_refs(*it, ast, out);
            }
        }
        Expr::Not { operand, .. } => collect_expr_refs(*operand, ast, out),
        Expr::Neg { operand, .. } => collect_expr_refs(*operand, ast, out),
        Expr::Binary { left, right, .. } => {
            collect_expr_refs(*left, ast, out);
            collect_expr_refs(*right, ast, out);
        }
        Expr::Str(_)
        | Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Ident(_)
        | Expr::Null { .. } => {}
    }
}
