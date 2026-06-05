//! Sophia-Core AST 与 arena。
//!
//! AST 仅表达表层语法结构（见 docs/language_implementation.md 第四节）：
//! 声明、字面量、表达式、span。**不含**类型信息、语义绑定、执行语义。
//!
//! 内存模型（4.2 节）：从 AST 层起使用 **Arena + ID 引用**，不用 `Box<Node>`
//! 或 `Rc<RefCell<Node>>`。表达式节点存放在 [`Ast::exprs`] 中，用 [`ExprId`]
//! 交叉引用；顶层声明与各类子结构以拥有式 `Vec` 承载（它们天然是树形、无共享）。
//!
//! Semantic Assist 字段（meaning/not/purpose/...）单独建模为 [`AssistField`]，
//! 与 Formal Core 解耦，便于 strip-assist 等价门禁在上层移除后比对 IR。

use crate::span::Span;

/// 表达式在 [`Ast::exprs`] 中的稳定索引。
///
/// 采用 `u32` 索引（对齐 docs/language_implementation.md 4.2 节示例）。
/// 索引只增不删，避免悬空引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(u32);

impl ExprId {
    /// 表达式 arena 中的 0 基索引。
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// 一个解析后的源文件 AST。
///
/// 顶层只允许一个 formal node（见 docs/engineering_architecture.md 5.1），
/// 但语法层不强制该约束（留给 HIR/索引层），因此这里用 `Vec` 容纳，
/// 以便对错误输入仍能产出可诊断的 AST。
#[derive(Debug, Default)]
pub struct Ast {
    /// 顶层声明，按源码顺序。
    pub items: Vec<Item>,
    /// 表达式 arena，由 [`ExprId`] 索引。
    exprs: Vec<Expr>,
}

impl Ast {
    /// 新建空 AST。
    pub(crate) fn new() -> Self {
        Ast::default()
    }

    /// 向 arena 追加一个表达式，返回其稳定 ID。
    pub(crate) fn alloc_expr(&mut self, expr: Expr) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(expr);
        id
    }

    /// 按 ID 读取表达式。
    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id.index()]
    }

    /// 移除全部 Semantic Assist 字段（strip-assist 等价门禁用，见 docs/language_design.md 5.1）。
    ///
    /// 清除不决定语义的辅助字段：各节点的 `meaning`/`not`/... 列表，以及 entity 的
    /// `semantic_identity` / `evolution`（§9，仅工具链检查，不参与运行时语义）。
    /// Formal Core（字段类型、签名、effect、error、body 等）与表达式 arena 不变——
    /// 因此移除前后的 Semantic IR 必须完全一致；该不变量由 `tools/check` 比对验证。
    pub fn strip_assists(&mut self) {
        for item in &mut self.items {
            match item {
                Item::Domain(d) => d.assists.clear(),
                Item::Entity(e) => {
                    e.assists.clear();
                    e.semantic_identity = None;
                    e.evolution = None;
                }
                Item::State(s) => {
                    for v in &mut s.values {
                        v.assists.clear();
                    }
                }
                Item::Transition(c) | Item::Action(c) => c.assists.clear(),
                Item::Effect(e) => e.assists.clear(),
                // error / capability / task 无 assist 字段。
                Item::Error(_) | Item::Capability(_) | Item::Task(_) => {}
            }
        }
    }
}

/// 顶层声明（formal node）。
#[derive(Debug)]
pub enum Item {
    Domain(Domain),
    Entity(Entity),
    State(StateDef),
    Transition(Callable),
    Error(ErrorDef),
    Capability(Capability),
    Action(Callable),
    Task(Task),
    /// effect 族声明（内置/领域 effect，见 docs/language_design.md 第十三节）。
    Effect(EffectDef),
}

impl Item {
    /// 声明的源码 span。
    pub fn span(&self) -> Span {
        match self {
            Item::Domain(d) => d.span,
            Item::Entity(e) => e.span,
            Item::State(s) => s.span,
            Item::Transition(c) | Item::Action(c) => c.span,
            Item::Error(e) => e.span,
            Item::Capability(c) => c.span,
            Item::Task(t) => t.span,
            Item::Effect(e) => e.span,
        }
    }

    /// 声明名。
    pub fn name(&self) -> &Ident {
        match self {
            Item::Domain(d) => &d.name,
            Item::Entity(e) => &e.name,
            Item::State(s) => &s.name,
            Item::Transition(c) | Item::Action(c) => &c.name,
            Item::Error(e) => &e.name,
            Item::Capability(c) => &c.name,
            Item::Task(t) => &t.name,
            Item::Effect(e) => &e.name,
        }
    }
}

/// 标识符（携带 span）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub text: String,
    pub span: Span,
}

// ============ Semantic Assist ============

/// Semantic Assist 字段的键（不决定语义；见 docs/language_design.md 5.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistKey {
    Meaning,
    Purpose,
    Because,
    Not,
    Examples,
    AntiPatterns,
    Plan,
    RepairNotes,
}

/// 一条 Semantic Assist 字段。值为一个或多个字符串字面量（已去引号前的原文）。
#[derive(Debug, Clone)]
pub struct AssistField {
    pub key: AssistKey,
    pub values: Vec<StrLit>,
    pub span: Span,
}

/// 字符串字面量：保留原始带引号文本与去引号后的内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrLit {
    /// 去掉首尾引号、解转义后的内容。
    pub value: String,
    pub span: Span,
}

// ============ 类型 ============

/// 类型引用（表层语法，不做解析）。
/// 类型引用（表层语法，不做解析）。见 docs/type_system.md：
/// `<>` 专属 intent（`Intent`）；结构类型用 `of` 关键字族（`ListOf`/`OneOf`/`SchemaOf`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// 具名类型，如 `Text`、`Todo`、`TodoStatus`、`Null`。
    Named { name: Ident, span: Span },
    /// Intent 包装类型，如 `Sanitized<Text>`、`Raw<Text>`（`<>` 仅用于 intent）。
    Intent {
        head: Ident,
        arg: Box<TypeRef>,
        span: Span,
    },
    /// `list of T`。
    ListOf { elem: Box<TypeRef>, span: Span },
    /// `one of { M, ... }`：互斥成员的联合。
    OneOf { members: Vec<TypeRef>, span: Span },
    /// `schema of T`（渐进类型）。
    SchemaOf { arg: Box<TypeRef>, span: Span },
}

impl TypeRef {
    pub fn span(&self) -> Span {
        match self {
            TypeRef::Named { span, .. }
            | TypeRef::Intent { span, .. }
            | TypeRef::ListOf { span, .. }
            | TypeRef::OneOf { span, .. }
            | TypeRef::SchemaOf { span, .. } => *span,
        }
    }
}

// ============ domain ============

#[derive(Debug)]
pub struct Domain {
    pub name: Ident,
    pub assists: Vec<AssistField>,
    pub span: Span,
}

// ============ entity ============

#[derive(Debug)]
pub struct Entity {
    pub name: Ident,
    pub assists: Vec<AssistField>,
    pub fields: Vec<FieldDecl>,
    pub invariants: Vec<Invariant>,
    pub semantic_identity: Option<SemanticIdentity>,
    pub evolution: Option<Evolution>,
    pub span: Span,
}

/// entity 字段声明。
#[derive(Debug)]
pub struct FieldDecl {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

/// entity 不变量。`when` 为可选守卫，`require` 为约束表达式。
#[derive(Debug)]
pub struct Invariant {
    pub name: Ident,
    pub when: Option<ExprId>,
    pub require: Option<ExprId>,
    pub span: Span,
}

/// 语义身份（Semantic Assist 类，演化检查用，不参与运行时语义）。
#[derive(Debug)]
pub struct SemanticIdentity {
    pub core_capability: Vec<StrLit>,
    pub forbidden_drift: Vec<StrLit>,
    /// `drift_tolerance` 原文（保留为字符串，避免在语法层引入浮点语义）。
    pub drift_tolerance: Option<String>,
    pub span: Span,
}

/// 演化边界（前瞻性约束）。
#[derive(Debug)]
pub struct Evolution {
    pub allowed: Vec<StrLit>,
    pub forbidden: Vec<StrLit>,
    pub requires_gate: Vec<StrLit>,
    pub span: Span,
}

// ============ state ============

#[derive(Debug)]
pub struct StateDef {
    pub name: Ident,
    pub values: Vec<StateValue>,
    pub span: Span,
}

#[derive(Debug)]
pub struct StateValue {
    pub name: Ident,
    pub assists: Vec<AssistField>,
    pub span: Span,
}

// ============ error ============

#[derive(Debug)]
pub struct ErrorDef {
    pub name: Ident,
    pub variants: Vec<ErrorVariant>,
    pub span: Span,
}

#[derive(Debug)]
pub struct ErrorVariant {
    pub name: Ident,
    pub fields: Vec<VariantField>,
    pub span: Span,
}

#[derive(Debug)]
pub struct VariantField {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

// ============ capability ============

#[derive(Debug)]
pub struct Capability {
    pub name: Ident,
    pub allow: Vec<Effect>,
    pub deny: Vec<Effect>,
    pub span: Span,
}

/// effect 引用（`Family.Op` / `Family.Op(args)`，或保留字 `Pure`）。
///
/// 取代原先硬编码的 4 类 effect：表层语法不再区分 Console/DB，统一为
/// `(family, op, args)` 三元组（见 docs/language_design.md 第十三节）。`Pure`
/// 表示空 effect 集（与任何具体 effect 互斥），用 `family`/`op` 均为空表达。
///
/// 允许 `Pure`（零大小）与 `Op`（带 family/op/args/span）的体积差异：二者语义判别清晰，
/// effect 不进入大规模集合，装箱反而增加全链路模式匹配负担。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `Pure`：无副作用。
    Pure,
    /// `Family.Op(args)`：一个 effect 操作引用。
    Op {
        family: Ident,
        op: Ident,
        args: Vec<EffectArg>,
        span: Span,
    },
}

/// effect 引用的实参（字面量或绑定名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectArg {
    Str(StrLit),
    Int {
        text: String,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    /// 当前作用域内的绑定名（如来自 input 的形参）。
    Ident(Ident),
}

// ============ effect 声明 ============

/// effect 族声明（`effect Family { operation Op { param... } }`）。
#[derive(Debug)]
pub struct EffectDef {
    pub name: Ident,
    pub assists: Vec<AssistField>,
    pub operations: Vec<EffectOperation>,
    pub span: Span,
}

/// effect 操作声明（`operation Op { param name: Type ... }`）。
#[derive(Debug)]
pub struct EffectOperation {
    pub name: Ident,
    pub params: Vec<EffectParam>,
    pub span: Span,
}

/// effect 操作的参数声明（`param name: Type`）。
#[derive(Debug)]
pub struct EffectParam {
    pub name: Ident,
    pub ty: TypeRef,
    pub span: Span,
}

// ============ storage ============（已移除，见 docs/stdlib_design.md：I/O 改由标准库提供）

// ============ transition / action（共享 Callable） ============

/// transition 与 action 的共享形状（见 docs/language_design.md 5.1）。
///
/// 语法层不区分二者的语义约束（transition 必须 Pure 等由 checker 负责）；
/// `kind` 仅记录来源关键字。
#[derive(Debug)]
pub struct Callable {
    pub kind: CallableKind,
    pub name: Ident,
    pub assists: Vec<AssistField>,
    pub capability: Option<Ident>,
    /// `intent_conversion: true` 标记。
    pub intent_conversion: bool,
    pub inputs: Vec<Param>,
    pub outputs: Vec<Param>,
    pub effects: Vec<Effect>,
    /// `errors { ... }` 中引用的 error variant 名。
    pub errors: Vec<Ident>,
    pub requires: Vec<ExprId>,
    pub ensures: Vec<ExprId>,
    pub body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    Transition,
    Action,
}

/// input/output 参数，带可选 `where` 谓词。
#[derive(Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeRef,
    pub predicate: Option<ExprId>,
    pub span: Span,
}

// ============ task ============

#[derive(Debug)]
pub struct Task {
    pub name: Ident,
    /// `goal:` 文本。
    pub goal: Option<StrLit>,
    pub includes: Vec<IncludeDecl>,
    pub excludes: Vec<Effect>,
    pub span: Span,
}

#[derive(Debug)]
pub struct IncludeDecl {
    pub kind: IncludeKind,
    pub name: Ident,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeKind {
    Entity,
    State,
    Error,
    Capability,
    Transition,
    Action,
}

// ============ body 子语言 ============

/// 语句块。
#[derive(Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// body 语句。
#[derive(Debug)]
pub enum Stmt {
    /// `let [mutable] name = expr`。
    Let {
        mutable: bool,
        name: Ident,
        value: ExprId,
        span: Span,
    },
    /// `set name = expr`。
    Set {
        name: Ident,
        value: ExprId,
        span: Span,
    },
    /// `return expr`。
    Return { value: ExprId, span: Span },
    /// `raise Variant { ... }`。
    Raise { value: ExprId, span: Span },
    /// `print expr`。
    Print { value: ExprId, span: Span },
    /// `if cond { ... } else { ... }`。
    If {
        condition: ExprId,
        consequence: Block,
        alternative: Option<ElseBranch>,
        span: Span,
    },
    /// `match subject { arms }`。
    Match {
        subject: ExprId,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// `repeat N times { ... }`。
    Repeat {
        count: ExprId,
        body: Block,
        span: Span,
    },
    /// `while condition { ... }`。
    While {
        condition: ExprId,
        body: Block,
        span: Span,
    },
    /// 表达式语句。
    Expr { value: ExprId, span: Span },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Set { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Raise { span, .. }
            | Stmt::Print { span, .. }
            | Stmt::If { span, .. }
            | Stmt::Match { span, .. }
            | Stmt::Repeat { span, .. }
            | Stmt::While { span, .. }
            | Stmt::Expr { span, .. } => *span,
        }
    }
}

/// `if` 的 else 分支：要么是 `else { ... }` 块，要么是 `else if ...` 链。
#[derive(Debug)]
pub enum ElseBranch {
    /// `else { ... }`。
    Block(Block),
    /// `else if ...`：持有后续的 If 语句。
    If(Box<Stmt>),
}

/// match 分支。`body` 是一个 block，单语句分支被规范化为单语句 block。
#[derive(Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
    pub span: Span,
}

/// match pattern。永久禁止 `_` catch-all（语法层已无法解析）。
/// 见 docs/type_system.md §三：one of 成员按 tag 分派。
#[derive(Debug)]
pub enum Pattern {
    /// 布尔字面量 `true` / `false`。
    Bool { value: bool, span: Span },
    /// 状态值，如 `TodoStatus.Done`（CST 中的 `qualified_name`）。
    State {
        head: Ident,
        value: Ident,
        span: Span,
    },
    /// `Null` 字面 pattern（匹配 `one of` 的 Null 成员）。
    Null { span: Span },
    /// 类型 pattern `<TypeName> <binding>`：匹配标量 / entity / state 成员并绑定。
    Type {
        ty: Ident,
        binding: Ident,
        span: Span,
    },
    /// error variant pattern `V { f1, f2 }`：匹配 error variant 成员，按字段名绑定。
    Variant {
        variant: Ident,
        fields: Vec<Ident>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Bool { span, .. }
            | Pattern::State { span, .. }
            | Pattern::Null { span }
            | Pattern::Type { span, .. }
            | Pattern::Variant { span, .. } => *span,
        }
    }
}

/// 二元运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
}

/// 表达式（存放于 [`Ast::exprs`] arena，经 [`ExprId`] 引用）。
#[derive(Debug)]
pub enum Expr {
    /// 字符串字面量。
    Str(StrLit),
    /// 整数字面量（保留原文，避免语法层引入溢出语义）。
    Int { text: String, span: Span },
    /// 布尔字面量。
    Bool { value: bool, span: Span },
    /// 变量 / 名称引用。
    Ident(Ident),
    /// `Null` 字面（`one of` 的 Null 成员值）。
    Null { span: Span },
    /// 列表字面量 `[a, b, ...]`。
    List { items: Vec<ExprId>, span: Span },
    /// 字段访问 `base.field`。
    Field {
        base: ExprId,
        field: Ident,
        span: Span,
    },
    /// 方法调用 `base.method(args)`。
    MethodCall {
        base: ExprId,
        method: Ident,
        args: Vec<ExprId>,
        span: Span,
    },
    /// 函数/转换调用 `callee(args)`。
    Call {
        callee: Ident,
        args: Vec<ExprId>,
        span: Span,
    },
    /// entity 构造 `Name { field = expr, ... }`。
    Construct {
        name: Ident,
        fields: Vec<FieldInit>,
        span: Span,
    },
    /// 一元 `not expr`。
    Not { operand: ExprId, span: Span },
    /// 一元算术取负 `-expr`。
    Neg { operand: ExprId, span: Span },
    /// 二元表达式。
    Binary {
        op: BinOp,
        left: ExprId,
        right: ExprId,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Str(s) => s.span,
            Expr::Int { span, .. }
            | Expr::Bool { span, .. }
            | Expr::Null { span }
            | Expr::List { span, .. }
            | Expr::Field { span, .. }
            | Expr::MethodCall { span, .. }
            | Expr::Call { span, .. }
            | Expr::Construct { span, .. }
            | Expr::Not { span, .. }
            | Expr::Neg { span, .. }
            | Expr::Binary { span, .. } => *span,
            Expr::Ident(i) => i.span,
        }
    }
}

/// entity 构造中的字段初始化 `field = expr`。
#[derive(Debug)]
pub struct FieldInit {
    pub name: Ident,
    pub value: ExprId,
    pub span: Span,
}
