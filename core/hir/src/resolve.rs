//! 名称解析与作用域分析。
//!
//! 见 docs/language_implementation.md 5.2 名称解析规则：
//! - 所有引用必须可由 ASG index 解析；
//! - 禁止隐式 import；
//! - 禁止同名 shadowing（包括 body 局部变量）；
//! - 跨 domain 引用必须通过 boundary 或 task include 显式声明。
//!
//! 解析以**单个顶层节点**为单位（一个文件一个节点）。诊断容错收集，
//! 不在首个错误处中断，便于一次反馈多个问题。

use crate::builtins;
use crate::error::{HirDiagnostic, HirDiagnosticKind};
use crate::index::{AsgIndex, NodeInfo, NodeKind};
use crate::scope::ScopeStack;
use sophia_syntax::{
    Ast, Block, Callable, Capability, Effect, EffectDef, ElseBranch, Entity, ErrorDef, Expr,
    ExprId, Ident, Item, Pattern, Stmt, Task, TypeRef,
};

/// 解析单个节点所需的上下文。
struct Resolver<'a> {
    index: &'a AsgIndex,
    ast: &'a Ast,
    /// 当前节点所属 domain，用于跨 domain 检查。
    domain: &'a str,
    /// 容错收集的诊断；解析结束后整体返回。
    diags: Vec<HirDiagnostic>,
}

/// 对单个顶层节点做名称解析与 scope 分析。
///
/// `domain` 是该节点所属 domain；`index` 是全项目 ASG index（含库注册表注入的特殊根 family，
/// 见 [`AsgIndex::is_library_family`]——核心据此放行 `Lib.Op(args)` 入口，不硬编码具体库名）。
/// 返回收集到的诊断（空表示通过）。
pub fn resolve_item(item: &Item, ast: &Ast, index: &AsgIndex, domain: &str) -> Vec<HirDiagnostic> {
    let mut r = Resolver {
        index,
        ast,
        domain,
        diags: Vec::new(),
    };
    r.resolve_item(item);
    r.diags
}

impl<'a> Resolver<'a> {
    fn resolve_item(&mut self, item: &Item) {
        match item {
            Item::Domain(_) => {}
            Item::Entity(e) => self.resolve_entity(e),
            Item::State(_) => {}
            Item::Error(e) => self.resolve_error(e),
            Item::Capability(c) => self.resolve_capability(c),
            Item::Transition(c) | Item::Action(c) => self.resolve_callable(c),
            Item::Task(t) => self.resolve_task(t),
            Item::Effect(e) => self.resolve_effect_def(e),
        }
    }

    // ---- 类型引用解析 ----

    /// 解析一个类型引用：wrapper 头必须是已知 wrapper，叶子必须是标量或已声明
    /// entity/state。
    fn resolve_type(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Named { name, .. } => self.resolve_type_name(name),
            // Intent 包装：head 必须是已知 intent wrapper。
            TypeRef::Intent { head, arg, .. } => {
                if !builtins::is_intent_wrapper(&head.text) {
                    self.unresolved_type(head);
                }
                self.resolve_type(arg);
            }
            TypeRef::ListOf { elem, .. } => self.resolve_type(elem),
            TypeRef::SchemaOf { arg, .. } => self.resolve_type(arg),
            // 联合：逐成员解析。成员可以是标量 / Null / entity / state / error variant。
            TypeRef::OneOf { members, .. } => {
                for m in members {
                    self.resolve_one_of_member(m);
                }
            }
        }
    }

    /// 解析 `one of` 的一个成员：标量 / Null / entity / state 走类型解析；
    /// 单独的具名成员若解析到 error variant 也合法（联合可含 error variant）。
    fn resolve_one_of_member(&mut self, ty: &TypeRef) {
        if let TypeRef::Named { name, .. } = ty {
            if builtins::is_scalar_type(&name.text) {
                return;
            }
            // entity / state / error variant 均可作联合成员。
            if matches!(
                self.index.get(&name.text).map(|i| i.kind),
                Some(NodeKind::Entity) | Some(NodeKind::State)
            ) {
                return;
            }
            if let Some(vinfo) = self.index.variant(&name.text) {
                let domain = vinfo.domain.clone();
                self.check_cross_domain_domain(name, &domain);
                return;
            }
            self.diags.push(HirDiagnostic::new(
                HirDiagnosticKind::UnresolvedReference,
                name.span,
                &name.text,
                format!(
                    "`one of` 成员 `{}` 不是已知类型 / state / error variant",
                    name.text
                ),
            ));
            return;
        }
        // 非具名成员（嵌套 list of / one of 等）走常规类型解析。
        self.resolve_type(ty);
    }

    /// 解析 match 类型 pattern 的类型名（`Int x` / `Todo t` 中的 `Int`/`Todo`）：
    /// 须为标量内置类型或已声明 entity / state（与类型位置同规则）。
    fn resolve_pattern_type_name(&mut self, name: &Ident) {
        self.resolve_type_name(name);
    }

    fn resolve_type_name(&mut self, name: &Ident) {
        if builtins::is_scalar_type(&name.text) {
            return;
        }
        match self.index.get(&name.text) {
            Some(info) => {
                // 类型位置只接受 entity / state。
                if !matches!(info.kind, NodeKind::Entity | NodeKind::State) {
                    self.diags.push(HirDiagnostic::new(
                        HirDiagnosticKind::WrongReferenceKind,
                        name.span,
                        &name.text,
                        format!("`{}` 是 {:?}，不能用作类型", name.text, info.kind),
                    ));
                }
                self.check_cross_domain(name, info);
            }
            None => self.unresolved_type(name),
        }
    }

    fn unresolved_type(&mut self, name: &Ident) {
        self.diags.push(HirDiagnostic::new(
            HirDiagnosticKind::UnresolvedReference,
            name.span,
            &name.text,
            format!("未知类型 `{}`", name.text),
        ));
    }

    /// 跨 domain 引用检查：当前节点引用了别的 domain 的节点，
    /// 在起步子集中以诊断提示（task include 才是显式声明的入口）。
    fn check_cross_domain(&mut self, name: &Ident, info: &NodeInfo) {
        self.check_cross_domain_domain(name, &info.domain);
    }

    /// 跨 domain 引用检查（按目标 domain 字符串）。
    fn check_cross_domain_domain(&mut self, name: &Ident, target_domain: &str) {
        // 库 domain 豁免：「用户 → 库节点」的跨 domain 引用合法（库是显式可用的外部能力，类比
        // task include 是显式入口）。用户↔用户跨 domain 仍受检（见 docs/stdlib_design.md §五）。
        if self.index.is_library_domain(target_domain) {
            return;
        }
        if target_domain != self.domain {
            self.diags.push(HirDiagnostic::new(
                HirDiagnosticKind::ImplicitCrossDomain,
                name.span,
                &name.text,
                format!(
                    "跨 domain 引用 `{}`（位于 {}），必须通过 boundary 或 task include 显式声明",
                    name.text, target_domain
                ),
            ));
        }
    }

    /// 解析对某一类节点的引用，校验其存在且类型匹配。
    fn resolve_node_ref(&mut self, name: &Ident, expected: NodeKind) {
        match self.index.get(&name.text) {
            Some(info) => {
                if info.kind != expected {
                    self.diags.push(HirDiagnostic::new(
                        HirDiagnosticKind::WrongReferenceKind,
                        name.span,
                        &name.text,
                        format!(
                            "`{}` 是 {:?}，此处需要 {:?}",
                            name.text, info.kind, expected
                        ),
                    ));
                }
                self.check_cross_domain(name, info);
            }
            None => self.diags.push(HirDiagnostic::new(
                HirDiagnosticKind::UnresolvedReference,
                name.span,
                &name.text,
                format!("未解析的引用 `{}`", name.text),
            )),
        }
    }

    // ---- 各节点解析 ----

    fn resolve_entity(&mut self, e: &Entity) {
        for f in &e.fields {
            self.resolve_type(&f.ty);
        }
        // 不变量表达式：根变量 `self` 可见。
        for inv in &e.invariants {
            let mut scope = ScopeStack::new();
            scope.declare(builtins::INVARIANT_SELF, false);
            if let Some(when) = inv.when {
                self.resolve_expr(when, &mut scope);
            }
            if let Some(require) = inv.require {
                self.resolve_expr(require, &mut scope);
            }
        }
    }

    fn resolve_error(&mut self, e: &ErrorDef) {
        for variant in &e.variants {
            for f in &variant.fields {
                self.resolve_type(&f.ty);
            }
        }
    }

    fn resolve_capability(&mut self, c: &Capability) {
        // 校验 allow/deny 中每个 effect 引用的 Family.Op 已声明、实参数量匹配。
        // effect 的字符串参数（如领域 effect 的资源名）是字面量，不是节点引用。
        for e in c.allow.iter().chain(c.deny.iter()) {
            self.resolve_effect(e);
        }
    }

    /// 校验一个 effect 引用：`Pure` 恒合法；`Family.Op` 必须已声明且实参数量匹配。
    fn resolve_effect(&mut self, e: &Effect) {
        let Effect::Op {
            family, op, args, ..
        } = e
        else {
            return; // Pure
        };
        match self.index.effect_op(&family.text, &op.text) {
            None => {
                self.diags.push(HirDiagnostic::new(
                    HirDiagnosticKind::UnresolvedEffect,
                    op.span,
                    format!("{}.{}", family.text, op.text),
                    format!("未声明的 effect 操作 `{}.{}`", family.text, op.text),
                ));
            }
            Some(info) if info.arity != args.len() => {
                self.diags.push(HirDiagnostic::new(
                    HirDiagnosticKind::UnresolvedEffect,
                    op.span,
                    format!("{}.{}", family.text, op.text),
                    format!(
                        "effect `{}.{}` 期望 {} 个参数，实际 {} 个",
                        family.text,
                        op.text,
                        info.arity,
                        args.len()
                    ),
                ));
            }
            Some(_) => {}
        }
    }

    /// effect 声明：校验各 operation 的 param 类型引用。
    fn resolve_effect_def(&mut self, e: &EffectDef) {
        for op in &e.operations {
            for p in &op.params {
                self.resolve_type(&p.ty);
            }
        }
    }

    fn resolve_callable(&mut self, c: &Callable) {
        // capability 绑定。
        if let Some(cap) = &c.capability {
            self.resolve_node_ref(cap, NodeKind::Capability);
        }
        // input / output 类型。
        for p in c.inputs.iter().chain(c.outputs.iter()) {
            self.resolve_type(&p.ty);
        }
        // effect 引用：Family.Op 必须已声明、实参数量匹配。
        for e in &c.effects {
            self.resolve_effect(e);
        }
        // errors 引用必须是已声明的 error variant（不是 error 节点本身）。
        for err in &c.errors {
            self.resolve_variant_ref(err);
        }

        // body scope：input 参数为根作用域变量。
        let mut scope = ScopeStack::new();
        for p in &c.inputs {
            // input 在起步子集中不可 set（不可变根变量）。
            scope.declare(p.name.text.clone(), false);
        }
        // 参数 where 谓词在含 input 的作用域内解析。
        for p in &c.inputs {
            if let Some(pred) = p.predicate {
                self.resolve_expr(pred, &mut scope);
            }
        }
        for p in &c.outputs {
            if let Some(pred) = p.predicate {
                self.resolve_expr(pred, &mut scope);
            }
        }

        if let Some(body) = &c.body {
            self.resolve_block(body, &mut scope);
        }

        // ensures：根变量 `output` 可见。
        let mut ensures_scope = ScopeStack::new();
        ensures_scope.declare(builtins::ENSURES_OUTPUT, false);
        for &id in &c.ensures {
            self.resolve_expr(id, &mut ensures_scope);
        }
        for &id in &c.requires {
            // requires 在 input 作用域内解析。
            let mut req_scope = ScopeStack::new();
            for p in &c.inputs {
                req_scope.declare(p.name.text.clone(), false);
            }
            self.resolve_expr(id, &mut req_scope);
        }
    }

    fn resolve_task(&mut self, t: &Task) {
        // task include：引用必须存在；这里不强制 kind 一致性以外的约束。
        for inc in &t.includes {
            let expected = include_kind_to_node_kind(inc.kind);
            match self.index.get(&inc.name.text) {
                Some(info) => {
                    if info.kind != expected {
                        self.diags.push(HirDiagnostic::new(
                            HirDiagnosticKind::WrongReferenceKind,
                            inc.name.span,
                            &inc.name.text,
                            format!(
                                "include 声明 `{}` 为 {:?}，但索引中是 {:?}",
                                inc.name.text, expected, info.kind
                            ),
                        ));
                    }
                    // task include 是跨 domain 的显式声明入口，因此不触发
                    // ImplicitCrossDomain 诊断。
                }
                None => self.diags.push(HirDiagnostic::new(
                    HirDiagnosticKind::UnresolvedReference,
                    inc.name.span,
                    &inc.name.text,
                    format!("include 引用了未知节点 `{}`", inc.name.text),
                )),
            }
        }
    }

    // ---- body / 表达式解析 ----

    fn resolve_block(&mut self, block: &Block, scope: &mut ScopeStack) {
        scope.push();
        for stmt in &block.stmts {
            self.resolve_stmt(stmt, scope);
        }
        scope.pop();
    }

    fn resolve_stmt(&mut self, stmt: &Stmt, scope: &mut ScopeStack) {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                value,
                ..
            } => {
                self.resolve_expr(*value, scope);
                self.declare_checked(name, *mutable, scope);
            }
            Stmt::Set { name, value, .. } => {
                self.resolve_expr(*value, scope);
                self.resolve_set_target(name, scope);
            }
            Stmt::Return { value, .. }
            | Stmt::Raise { value, .. }
            | Stmt::Print { value, .. }
            | Stmt::Expr { value, .. } => self.resolve_expr(*value, scope),
            Stmt::If {
                condition,
                consequence,
                alternative,
                ..
            } => {
                self.resolve_expr(*condition, scope);
                self.resolve_block(consequence, scope);
                match alternative {
                    Some(ElseBranch::Block(b)) => self.resolve_block(b, scope),
                    Some(ElseBranch::If(s)) => self.resolve_stmt(s, scope),
                    None => {}
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.resolve_expr(*subject, scope);
                for arm in arms {
                    scope.push();
                    // Type pattern 的 binding、Variant pattern 的字段名都作为 arm 内绑定；
                    // Type pattern 的类型名须解析（标量 / entity / state）；Variant 名解析为 error variant。
                    match &arm.pattern {
                        Pattern::Type { ty, binding, .. } => {
                            self.resolve_pattern_type_name(ty);
                            self.declare_checked(binding, false, scope);
                        }
                        Pattern::Variant {
                            variant, fields, ..
                        } => {
                            self.resolve_variant_ref(variant);
                            for f in fields {
                                self.declare_checked(f, false, scope);
                            }
                        }
                        _ => {}
                    }
                    // arm.body 是 block；这里不再额外 push（resolve_block 自带一层）。
                    self.resolve_block(&arm.body, scope);
                    scope.pop();
                }
            }
            Stmt::Repeat { count, body, .. } => {
                self.resolve_expr(*count, scope);
                self.resolve_block(body, scope);
            }
        }
    }

    /// 声明局部变量，先做 shadowing 检查。
    fn declare_checked(&mut self, name: &Ident, mutable: bool, scope: &mut ScopeStack) {
        if scope.is_visible(&name.text) {
            self.diags.push(HirDiagnostic::new(
                HirDiagnosticKind::Shadowing,
                name.span,
                &name.text,
                format!("禁止 shadow 可见变量 `{}`", name.text),
            ));
            // 仍然声明，避免后续把对它的引用误报为未解析。
        }
        scope.declare(name.text.clone(), mutable);
    }

    /// 解析 `set` 目标：必须已绑定且可变。
    fn resolve_set_target(&mut self, name: &Ident, scope: &ScopeStack) {
        match scope.lookup(&name.text) {
            Some(binding) => {
                if !binding.mutable {
                    self.diags.push(HirDiagnostic::new(
                        HirDiagnosticKind::AssignToImmutable,
                        name.span,
                        &name.text,
                        format!("`{}` 不可变，不能 set；用 `let mutable` 声明", name.text),
                    ));
                }
            }
            None => self.diags.push(HirDiagnostic::new(
                HirDiagnosticKind::UnresolvedVariable,
                name.span,
                &name.text,
                format!("set 了未声明的变量 `{}`", name.text),
            )),
        }
    }

    fn resolve_expr(&mut self, id: ExprId, scope: &mut ScopeStack) {
        let expr = self.ast.expr(id);
        match expr {
            Expr::Str(_) | Expr::Int { .. } | Expr::Bool { .. } | Expr::Null { .. } => {}
            Expr::Ident(ident) => self.resolve_value_ident(ident, scope),
            Expr::List { items, .. } => {
                for &it in items {
                    self.resolve_expr(it, scope);
                }
            }
            Expr::Field { base, .. } => {
                // 字段名本身的合法性由类型检查负责；这里只解析 base。
                self.resolve_expr(*base, scope);
            }
            Expr::MethodCall { base, args, .. } => {
                self.resolve_expr(*base, scope);
                for &a in args {
                    self.resolve_expr(a, scope);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.resolve_callee(callee);
                for &a in args {
                    self.resolve_expr(a, scope);
                }
            }
            Expr::Construct { name, fields, .. } => {
                // 构造式语法 `Name { ... }` 在 grammar 中同时覆盖 entity 构造与
                // transition 调用（见 language_design 5.1 的 transition 调用写法）。
                // HIR 只做名称解析，不强制起步子集限制，因此接受 entity 或 transition。
                self.resolve_construct_name(name);
                for fi in fields {
                    self.resolve_expr(fi.value, scope);
                }
            }
            Expr::Not { operand, .. } => self.resolve_expr(*operand, scope),
            Expr::Neg { operand, .. } => self.resolve_expr(*operand, scope),
            Expr::Binary { left, right, .. } => {
                self.resolve_expr(*left, scope);
                self.resolve_expr(*right, scope);
            }
        }
    }

    /// 解析作为值的标识符：要么是可见局部变量，要么……否则未解析。
    ///
    /// 注意：state 名等通过 `field_access`/`qualified` 出现在更复杂表达式里；
    /// 裸标识符在 body 中只可能是局部变量或特殊根。
    fn resolve_value_ident(&mut self, ident: &Ident, scope: &ScopeStack) {
        if scope.lookup(&ident.text).is_some() {
            return;
        }
        // 允许引用标准库 / 三方库 I/O 的**特殊根**标识符：库 family（如 `File` / `Http`）是
        // body 级 I/O 语法（`File.Read(path)` / `Http.Get(url)`）的入口。判定经 index 的库 family
        // 集（`is_library_family`，由库注册表注入）——核心不再硬编码具体库名。type 层并入对应
        // effect、给出返回类型，解释器经 host 委派。
        if self.index.is_library_family(&ident.text) {
            return;
        }
        // 允许引用已声明节点（如 state 名作为 `TodoStatus.Done` 的 base，
        // 或 transition/action 名作为调用 base）。
        if self.index.contains(&ident.text) {
            return;
        }
        self.diags.push(HirDiagnostic::new(
            HirDiagnosticKind::UnresolvedVariable,
            ident.span,
            &ident.text,
            format!("未声明的变量或名称 `{}`", ident.text),
        ));
    }

    /// 解析构造式名字：可为 entity 构造、transition 调用，或 error variant
    /// （`raise Variant { ... }` 也 lower 为 Construct）。构造式语法在 grammar
    /// 中合并表达，这里按名字在索引 / variant 表中的归属判定。
    fn resolve_construct_name(&mut self, name: &Ident) {
        if let Some(info) = self.index.get(&name.text) {
            if !matches!(info.kind, NodeKind::Entity | NodeKind::Transition) {
                self.diags.push(HirDiagnostic::new(
                    HirDiagnosticKind::WrongReferenceKind,
                    name.span,
                    &name.text,
                    format!("`{}` 是 {:?}，不能用于构造或转换调用", name.text, info.kind),
                ));
            } else {
                self.check_cross_domain(name, info);
            }
            return;
        }
        if let Some(vinfo) = self.index.variant(&name.text) {
            self.check_cross_domain_domain(name, &vinfo.domain);
            return;
        }
        self.diags.push(HirDiagnostic::new(
            HirDiagnosticKind::UnresolvedReference,
            name.span,
            &name.text,
            format!("构造 / raise 了未知节点或 variant `{}`", name.text),
        ));
    }

    /// 解析对 error variant 的引用（`errors { ... }` 列表项）。
    fn resolve_variant_ref(&mut self, name: &Ident) {
        match self.index.variant(&name.text) {
            Some(vinfo) => self.check_cross_domain_domain(name, &vinfo.domain),
            None => {
                // 给出更精确的提示：如果它其实是 error 节点名，提示应引用 variant。
                let msg = if matches!(self.index.kind_of(&name.text), Some(NodeKind::Error)) {
                    format!("`{}` 是 error 节点，errors 应引用其 variant", name.text)
                } else {
                    format!("未知的 error variant `{}`", name.text)
                };
                self.diags.push(HirDiagnostic::new(
                    HirDiagnosticKind::UnresolvedReference,
                    name.span,
                    &name.text,
                    msg,
                ));
            }
        }
    }

    /// 解析调用目标 callee：内置函数、或已声明 transition/action。
    fn resolve_callee(&mut self, callee: &Ident) {
        if builtins::is_builtin_function(&callee.text) {
            return;
        }
        match self.index.get(&callee.text) {
            Some(info) => {
                if !matches!(info.kind, NodeKind::Transition | NodeKind::Action) {
                    self.diags.push(HirDiagnostic::new(
                        HirDiagnosticKind::WrongReferenceKind,
                        callee.span,
                        &callee.text,
                        format!("`{}` 是 {:?}，不可调用", callee.text, info.kind),
                    ));
                } else {
                    self.check_cross_domain(callee, info);
                }
            }
            None => self.diags.push(HirDiagnostic::new(
                HirDiagnosticKind::UnresolvedReference,
                callee.span,
                &callee.text,
                format!("调用了未知的 `{}`", callee.text),
            )),
        }
    }
}

fn include_kind_to_node_kind(k: sophia_syntax::IncludeKind) -> NodeKind {
    use sophia_syntax::IncludeKind as I;
    match k {
        I::Entity => NodeKind::Entity,
        I::State => NodeKind::State,
        I::Error => NodeKind::Error,
        I::Capability => NodeKind::Capability,
        I::Transition => NodeKind::Transition,
        I::Action => NodeKind::Action,
    }
}
