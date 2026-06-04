//! 类型层：类型推断与约束求解。
//!
//! 见 docs/language_implementation.md 6.2、7.1、7.2、7.6、第十六节。
//! 推导结果写入按 `ExprId` 索引的 [`TypeTable`]，**不修改 AST 节点**。
//!
//! 覆盖 MVP 检查集中的 Type Check / Intent Type Check：
//! - 字段赋值、return、调用实参类型匹配；
//! - entity 构造全字段覆盖、未知字段、字段类型匹配；
//! - block scope 内变量类型；
//! - 非 Unit callable 全路径 return/raise；
//! - Intent 严格相等与表达式 intent 推导。

use crate::effect::{Effect, EffectSet};
use crate::error::{SemanticDiagnostic, SemanticDiagnosticKind as K};
use crate::model::SemanticModel;
use crate::ty::{IntentKind, Ty};
use sophia_hir::{AsgIndex, Scalar, TypeDesc};
use sophia_syntax::{Ast, BinOp, Block, ElseBranch, Expr, ExprId, Pattern, Stmt};
use std::collections::HashMap;

/// 表达式类型推导结果表，按 `ExprId` 索引。
#[derive(Debug, Default)]
pub struct TypeTable {
    types: HashMap<u32, Ty>,
}

impl TypeTable {
    pub fn new() -> Self {
        TypeTable::default()
    }

    fn set(&mut self, id: ExprId, ty: Ty) {
        self.types.insert(id.0, ty);
    }

    /// 查表达式推导出的类型。
    pub fn get(&self, id: ExprId) -> Option<&Ty> {
        self.types.get(&id.0)
    }
}

/// 局部变量作用域（带类型），与 HIR scope 平行但承载类型。
#[derive(Default)]
struct TypeScope {
    frames: Vec<HashMap<String, Ty>>,
}

impl TypeScope {
    fn new() -> Self {
        TypeScope {
            frames: vec![HashMap::new()],
        }
    }
    fn push(&mut self) {
        self.frames.push(HashMap::new());
    }
    fn pop(&mut self) {
        self.frames.pop();
    }
    fn declare(&mut self, name: impl Into<String>, ty: Ty) {
        self.frames.last_mut().unwrap().insert(name.into(), ty);
    }
    fn lookup(&self, name: &str) -> Option<&Ty> {
        self.frames.iter().rev().find_map(|f| f.get(name))
    }
}

/// body 终止性：每条路径是否都以 return/raise 结束。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// 该块/语句之后控制流可继续（未终止）。
    Fallthrough,
    /// 该块/语句必定 return 或 raise（路径终止）。
    Terminates,
}

/// 把库契约的受限类型描述符（[`TypeDesc`]）转为语义 [`Ty`]。
///
/// intent 名经 [`IntentKind::from_head`] 解析——这是「库只能引用核心固定 intent 集」安全红线的
/// 兑现点：清单里写的 intent 名若不属核心集，这里得到 `Ty::Error`（保守、不放行未知 intent）。
fn typedesc_to_ty(desc: &TypeDesc) -> Ty {
    match desc {
        TypeDesc::Scalar(s) => scalar_to_ty(*s),
        TypeDesc::Intent { intent, inner } => match IntentKind::from_head(intent) {
            Some(kind) => Ty::Intent(kind, Box::new(scalar_to_ty(*inner))),
            // 未知 intent 名（不属核心固定集）：保守恢复为 Error，绝不放行库自定义 intent。
            None => Ty::Error,
        },
    }
}

fn scalar_to_ty(s: Scalar) -> Ty {
    match s {
        Scalar::Int => Ty::Int,
        Scalar::Bool => Ty::Bool,
        Scalar::Text => Ty::Text,
        Scalar::Unit => Ty::Unit,
    }
}

/// 类型层分析器。借用模型与 AST，输出类型表与诊断。
pub struct TypeChecker<'a> {
    model: &'a SemanticModel,
    ast: &'a Ast,
    /// ASG index：携带库 op 契约（`library_op`），用于**表驱动**校验 `Lib.Op(args)`——替代逐库
    /// 命令式 match，使核心不硬编码具体库（见 docs/stdlib_design.md）。
    index: &'a AsgIndex,
    table: TypeTable,
    diags: Vec<SemanticDiagnostic>,
}

/// 一次 callable 类型检查的产物。
pub struct TypeCheckOutput {
    pub table: TypeTable,
    pub diags: Vec<SemanticDiagnostic>,
    /// body 使用到的 effect（供 effect 层复用，避免重复遍历）。
    pub used_effects: EffectSet,
}

impl<'a> TypeChecker<'a> {
    pub fn new(model: &'a SemanticModel, ast: &'a Ast, index: &'a AsgIndex) -> Self {
        TypeChecker {
            model,
            ast,
            index,
            table: TypeTable::new(),
            diags: Vec::new(),
        }
    }

    /// 对某个 callable（按名）做类型检查与 effect 收集。
    pub fn check_callable(mut self, name: &str) -> TypeCheckOutput {
        let mut used_effects = EffectSet::new();
        if let Some(decl) = self.model.callables.get(name) {
            let mut scope = TypeScope::new();
            for (pname, pty) in &decl.inputs {
                scope.declare(pname.clone(), pty.clone());
            }

            // 找到对应 AST callable 以遍历 body / where / ensures。
            if let Some(callable) = self.find_callable_ast(name) {
                // input where 谓词：在含 input 的作用域内解析。
                for p in &callable.inputs {
                    if let Some(pred) = p.predicate {
                        let t = self.infer(pred, &mut scope, &mut used_effects);
                        self.expect_bool(pred, &t, "where 谓词");
                    }
                }
                // output where 谓词：output 参数自身在该谓词作用域内可见
                //（如 `todo: Todo where todo.status == ...`）。
                for (i, p) in callable.outputs.iter().enumerate() {
                    if let Some(pred) = p.predicate {
                        let mut out_scope = TypeScope::new();
                        if let Some((pname, pty)) = decl.outputs.get(i) {
                            out_scope.declare(pname.clone(), pty.clone());
                        }
                        let mut sink = EffectSet::new();
                        let t = self.infer(pred, &mut out_scope, &mut sink);
                        self.expect_bool(pred, &t, "where 谓词");
                    }
                }
                if let Some(body) = &callable.body {
                    let flow = self.check_block(body, &mut scope, &mut used_effects, decl);
                    // 非 Unit action 全路径必须 return/raise。
                    if let Some(out_ty) = decl.sole_output_ty() {
                        if !matches!(out_ty, Ty::Unit) && flow == Flow::Fallthrough {
                            self.diags.push(SemanticDiagnostic::new(
                                K::MissingReturn,
                                callable.span,
                                format!(
                                    "`{}` 的输出类型为 {out_ty}，但存在未 return/raise 的路径",
                                    name
                                ),
                            ));
                        }
                    }
                }
                // ensures 谓词：`output` 是以各 output 参数为字段的记录
                //（如 `output.todo.status`，见设计第五节示例）。
                let mut pred_scope = TypeScope::new();
                pred_scope.declare("output", output_root_ty(decl));
                for &id in &callable.ensures {
                    let mut sink = EffectSet::new();
                    let t = self.infer(id, &mut pred_scope, &mut sink);
                    self.expect_bool(id, &t, "ensures");
                }

                // intent_conversion 结构约束（设计 7.2）：一入一出、同 inner、不同 intent、
                // 无 effect、body 直接 return 输入值。
                if decl.intent_conversion {
                    self.check_intent_conversion(name, decl, callable, &used_effects);
                }
            }
        }

        TypeCheckOutput {
            table: self.table,
            diags: self.diags,
            used_effects,
        }
    }

    /// 校验 `intent_conversion: true` action 的结构约束（设计 7.2）。
    ///
    /// 必须满足：恰一入一出、input/output 的 inner 类型相同、intent 种类不同、无 effect、
    /// body 仅 `return <input 名>`。任一不满足报 [`K::InvalidIntentConversion`]。
    fn check_intent_conversion(
        &mut self,
        name: &str,
        decl: &crate::model::CallableDecl,
        callable: &sophia_syntax::Callable,
        used_effects: &EffectSet,
    ) {
        let fail = |diags: &mut Vec<SemanticDiagnostic>, reason: &str| {
            diags.push(SemanticDiagnostic::new(
                K::InvalidIntentConversion,
                callable.span,
                format!("`{name}` 标记 intent_conversion 但{reason}"),
            ));
        };

        // 一入一出。
        if decl.inputs.len() != 1 || decl.outputs.len() != 1 {
            fail(&mut self.diags, "不是恰一入一出");
            return;
        }
        let in_ty = &decl.inputs[0].1;
        let out_ty = &decl.outputs[0].1;

        // 两侧都必须带 intent，且 inner 相同、intent 种类不同。
        match (in_ty, out_ty) {
            (Ty::Intent(k_in, inner_in), Ty::Intent(k_out, inner_out)) => {
                if inner_in != inner_out {
                    fail(&mut self.diags, "输入输出的 inner 类型不同");
                } else if k_in == k_out {
                    fail(&mut self.diags, "输入输出的 intent 种类相同（未发生转换）");
                }
            }
            _ => fail(&mut self.diags, "输入或输出未携带 intent"),
        }

        // 无 effect。
        if !used_effects.is_pure() {
            fail(&mut self.diags, "产生了 effect");
        }

        // body 仅 `return <input 名>`。
        let in_name = &decl.inputs[0].0;
        let ok_body = callable.body.as_ref().is_some_and(|b| {
            b.stmts.len() == 1
                && matches!(
                    &b.stmts[0],
                    Stmt::Return { value, .. }
                        if matches!(self.ast.expr(*value), Expr::Ident(id) if &id.text == in_name)
                )
        });
        if !ok_body {
            fail(&mut self.diags, "body 不是直接 return 输入值");
        }
    }

    fn find_callable_ast(&self, name: &str) -> Option<&'a sophia_syntax::Callable> {
        self.ast.items.iter().find_map(|it| match it {
            sophia_syntax::Item::Action(c) | sophia_syntax::Item::Transition(c)
                if c.name.text == name =>
            {
                Some(c)
            }
            _ => None,
        })
    }

    // ---- 语句 / 块 ----

    fn check_block(
        &mut self,
        block: &Block,
        scope: &mut TypeScope,
        effects: &mut EffectSet,
        decl: &crate::model::CallableDecl,
    ) -> Flow {
        scope.push();
        let mut flow = Flow::Fallthrough;
        for stmt in &block.stmts {
            // 已终止后仍有语句：保持 Terminates（不可达语句的精确诊断留待后续）。
            let f = self.check_stmt(stmt, scope, effects, decl);
            if f == Flow::Terminates {
                flow = Flow::Terminates;
            }
        }
        scope.pop();
        flow
    }

    fn check_stmt(
        &mut self,
        stmt: &Stmt,
        scope: &mut TypeScope,
        effects: &mut EffectSet,
        decl: &crate::model::CallableDecl,
    ) -> Flow {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let t = self.infer(*value, scope, effects);
                scope.declare(name.text.clone(), t);
                Flow::Fallthrough
            }
            Stmt::Set { name, value, .. } => {
                let t = self.infer(*value, scope, effects);
                // set 的值须与变量声明类型相容（HIR 已校验目标存在且 mutable）。
                if let Some(declared) = scope.lookup(&name.text) {
                    let declared = declared.clone();
                    self.check_assignable(*value, &t, &declared, "set 值");
                }
                Flow::Fallthrough
            }
            Stmt::Return { value, .. } => {
                let t = self.infer(*value, scope, effects);
                // return 类型须与 sole output 相容。intent_conversion action 是 intent
                // 转换的唯一合法处（设计 7.2），其 `return input` 必然跨 intent，故豁免此处
                // 严格 intent 检查——结构合法性由 `check_intent_conversion` 单独校验。
                if let Some(out_ty) = decl.sole_output_ty() {
                    if !decl.intent_conversion {
                        self.check_assignable(*value, &t, out_ty, "return 值");
                    }
                }
                Flow::Terminates
            }
            Stmt::Raise { value, .. } => {
                self.infer(*value, scope, effects);
                Flow::Terminates
            }
            Stmt::Print { value, .. } => {
                let t = self.infer(*value, scope, effects);
                effects.insert(Effect::new("Console", "Write", vec![]));
                self.check_console_output(*value, &t);
                Flow::Fallthrough
            }
            Stmt::Expr { value, .. } => {
                self.infer(*value, scope, effects);
                Flow::Fallthrough
            }
            Stmt::If {
                condition,
                consequence,
                alternative,
                ..
            } => {
                let ct = self.infer(*condition, scope, effects);
                self.expect_bool(*condition, &ct, "if 条件");
                let then_flow = self.check_block(consequence, scope, effects, decl);
                let else_flow = match alternative {
                    Some(ElseBranch::Block(b)) => self.check_block(b, scope, effects, decl),
                    Some(ElseBranch::If(s)) => self.check_stmt(s, scope, effects, decl),
                    None => Flow::Fallthrough,
                };
                // 仅当两支都终止才终止。
                if then_flow == Flow::Terminates && else_flow == Flow::Terminates {
                    Flow::Terminates
                } else {
                    Flow::Fallthrough
                }
            }
            Stmt::Match {
                subject,
                arms,
                span,
                ..
            } => {
                let st = self.infer(*subject, scope, effects);
                self.check_match_exhaustive(&st, arms, *span);
                let mut all_terminate = !arms.is_empty();
                for arm in arms {
                    scope.push();
                    self.bind_pattern(&arm.pattern, &st, scope);
                    let f = self.check_block(&arm.body, scope, effects, decl);
                    if f != Flow::Terminates {
                        all_terminate = false;
                    }
                    scope.pop();
                }
                if all_terminate {
                    Flow::Terminates
                } else {
                    Flow::Fallthrough
                }
            }
            Stmt::Repeat { count, body, .. } => {
                let t = self.infer(*count, scope, effects);
                self.expect_int(*count, &t, "repeat 次数");
                // 循环体不保证执行，故视为可继续。
                self.check_block(body, scope, effects, decl);
                Flow::Fallthrough
            }
        }
    }

    /// 把 match pattern 绑定的变量加入作用域。
    fn bind_pattern(&mut self, pattern: &Pattern, _subject: &Ty, scope: &mut TypeScope) {
        match pattern {
            // 类型 pattern `Int x` / `Todo t`：binding 类型 = 该类型名解析出的 Ty。
            Pattern::Type { ty, binding, .. } => {
                let bind_ty = self.resolve_type_name_to_ty(&ty.text);
                scope.declare(binding.text.clone(), bind_ty);
            }
            // error variant pattern `V { f, ... }`：字段名绑定为该 variant 声明的字段类型。
            Pattern::Variant {
                variant, fields, ..
            } => {
                let vdecl = self.model.variants.get(&variant.text).cloned();
                for f in fields {
                    let fty = vdecl
                        .as_ref()
                        .and_then(|d| d.field_ty(&f.text).cloned())
                        .unwrap_or(Ty::Error);
                    scope.declare(f.text.clone(), fty);
                }
            }
            _ => {}
        }
    }

    /// 把一个类型名（用于类型 pattern）解析为 `Ty`：标量 / Null / entity / state。
    fn resolve_type_name_to_ty(&self, name: &str) -> Ty {
        match name {
            "Unit" => Ty::Unit,
            "Bool" => Ty::Bool,
            "Int" => Ty::Int,
            "Text" => Ty::Text,
            "Uuid" => Ty::Uuid,
            "Time" => Ty::Time,
            "Null" => Ty::Null,
            _ if self.model.entities.contains_key(name) => Ty::Entity(name.to_string()),
            _ if self.model.states.contains_key(name) => Ty::State(name.to_string()),
            _ => Ty::Error,
        }
    }

    // ---- 表达式推导 ----

    fn infer(&mut self, id: ExprId, scope: &mut TypeScope, effects: &mut EffectSet) -> Ty {
        let ty = self.infer_inner(id, scope, effects);
        self.table.set(id, ty.clone());
        ty
    }

    fn infer_inner(&mut self, id: ExprId, scope: &mut TypeScope, effects: &mut EffectSet) -> Ty {
        let expr = self.ast.expr(id);
        match expr {
            Expr::Str(_) => Ty::Text,
            Expr::Int { .. } => Ty::Int,
            Expr::Bool { .. } => Ty::Bool,
            // `Null` 字面：其类型即 `Null`（作为 one of 成员经 assignability upcast）。
            Expr::Null { .. } => Ty::Null,
            Expr::Ident(name) => scope.lookup(&name.text).cloned().unwrap_or(Ty::Error),
            Expr::List { items, .. } => {
                let mut elem = Ty::Unknown;
                for &it in items {
                    let t = self.infer(it, scope, effects);
                    if elem == Ty::Unknown {
                        elem = t;
                    }
                }
                Ty::List(Box::new(elem))
            }
            Expr::Field { base, field, span } => {
                let bt = self.infer(*base, scope, effects);
                self.field_ty(&bt, &field.text, *span)
            }
            Expr::MethodCall {
                base, method, args, ..
            } => {
                // body 级库 I/O 调用：`Lib.Op(args)`（特殊根 method_call，base 形如 `Ident("File")`
                // / `Ident("Http")` / 三方库 family）。经库注册表（`index.library_op`）表驱动特判：
                // 推导实参、并入对应 effect、给出返回类型——核心不硬编码具体库。见 docs/stdlib_design.md。
                if let Some(ty) = self.infer_effect_op(*base, &method.text, args, scope, effects) {
                    return ty;
                }
                let bt = self.infer(*base, scope, effects);
                for &a in args {
                    self.infer(a, scope, effects);
                }
                self.method_ty(&bt, &method.text)
            }
            Expr::Call { callee, args, span } => {
                self.infer_call(callee, args, *span, scope, effects)
            }
            Expr::Construct { name, fields, span } => {
                // 推导字段值类型。
                let mut field_tys = Vec::new();
                for fi in fields {
                    let t = self.infer(fi.value, scope, effects);
                    field_tys.push((fi.name.text.clone(), t, fi.span));
                }
                self.check_construct(&name.text, &field_tys, *span)
            }
            Expr::Not { operand, span } => {
                let t = self.infer(*operand, scope, effects);
                self.expect_bool(*operand, &t, "not 操作数");
                let _ = span;
                Ty::Bool
            }
            Expr::Neg { operand, span } => {
                let t = self.infer(*operand, scope, effects);
                self.expect_int(*operand, &t, "取负操作数");
                let _ = span;
                Ty::Int
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => self.infer_binary(*op, *left, *right, *span, scope, effects),
        }
    }

    fn infer_call(
        &mut self,
        callee: &sophia_syntax::Ident,
        args: &[ExprId],
        span: sophia_syntax::Span,
        scope: &mut TypeScope,
        effects: &mut EffectSet,
    ) -> Ty {
        // 内置 to_text(Int) -> Text。
        if callee.text == "to_text" {
            self.check_call_arity("to_text", 1, args.len(), span);
            for &a in args {
                let t = self.infer(a, scope, effects);
                self.expect_int(a, &t, "to_text 参数");
            }
            return Ty::Text;
        }
        // 调用 action / transition：传播其 effects、检查实参、返回其 sole output。
        if let Some(decl) = self.model.callables.get(&callee.text) {
            // 被调用方 effects 并入当前 effects（子集校验在 effect 层）。
            for e in decl.declared_effects.iter() {
                effects.insert(e.clone());
            }
            self.check_call_arity(&callee.text, decl.inputs.len(), args.len(), span);
            // 实参逐个推导并与 input 类型比对（按顺序）。
            for (i, &a) in args.iter().enumerate() {
                let at = self.infer(a, scope, effects);
                if let Some((_, pty)) = decl.inputs.get(i) {
                    self.check_assignable(a, &at, pty, "调用实参");
                }
            }
            return decl.sole_output_ty().cloned().unwrap_or(Ty::Unknown);
        }
        // 未知 callee（HIR 已报未解析）：恢复占位。
        for &a in args {
            self.infer(a, scope, effects);
        }
        let _ = span;
        Ty::Error
    }

    fn check_call_arity(
        &mut self,
        callee: &str,
        expected: usize,
        actual: usize,
        span: sophia_syntax::Span,
    ) {
        if expected != actual {
            self.diags.push(SemanticDiagnostic::new(
                K::TypeMismatch,
                span,
                format!("`{callee}` 期望 {expected} 个参数，但收到 {actual} 个"),
            ));
        }
    }

    fn infer_binary(
        &mut self,
        op: BinOp,
        left: ExprId,
        right: ExprId,
        span: sophia_syntax::Span,
        scope: &mut TypeScope,
        effects: &mut EffectSet,
    ) -> Ty {
        let lt = self.infer(left, scope, effects);
        let rt = self.infer(right, scope, effects);
        match op {
            BinOp::Or | BinOp::And => {
                self.expect_bool(left, &lt, "布尔运算左操作数");
                self.expect_bool(right, &rt, "布尔运算右操作数");
                Ty::Bool
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Ty::Bool,
            BinOp::Add => {
                // Int+Int -> Int；Text+Text -> Text（保留 intent）；List + [item] -> List。
                self.infer_add(&lt, &rt, span)
            }
            BinOp::Sub | BinOp::Mul => {
                self.expect_int(left, &lt, "算术左操作数");
                self.expect_int(right, &rt, "算术右操作数");
                Ty::Int
            }
        }
    }

    /// `+` 的类型推导：Int / Text（保留 intent）/ List 追加。
    fn infer_add(&mut self, lt: &Ty, rt: &Ty, span: sophia_syntax::Span) -> Ty {
        if lt.is_gradual() || rt.is_gradual() {
            return Ty::Unknown;
        }
        // 保留 intent 的文本拼接：基于 inner 判定。
        let (l_inner, l_intent) = split_intent(lt);
        let (r_inner, _r_intent) = split_intent(rt);
        match (l_inner, r_inner) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Text, Ty::Text) => {
                // 表达式推导保留左侧 intent（docs 7.2 示例：Raw<Text>+Text -> Raw<Text>）。
                match l_intent {
                    Some(k) => Ty::Intent(k, Box::new(Ty::Text)),
                    None => Ty::Text,
                }
            }
            (Ty::List(le), _) => {
                // list + [item]：右侧应为同元素 List。
                Ty::List(le.clone())
            }
            _ => {
                self.diags.push(SemanticDiagnostic::new(
                    K::TypeMismatch,
                    span,
                    format!("`+` 不支持操作数类型 {lt} 与 {rt}"),
                ));
                Ty::Error
            }
        }
    }

    /// 字段访问的结果类型。
    fn field_ty(&mut self, base: &Ty, field: &str, span: sophia_syntax::Span) -> Ty {
        // 内置伪字段（起步子集示例使用）：Text.length（"无"用 `match ... { Null => }` 取代旧 .exists）。
        match base.strip_intent() {
            Ty::Text if field == "length" => return Ty::Int,
            Ty::Record(fields) => {
                // `output.<param>` 访问：返回对应 output 参数类型。
                if let Some((_, t)) = fields.iter().find(|(n, _)| n == field) {
                    return t.clone();
                }
                self.diags.push(SemanticDiagnostic::new(
                    K::NoSuchField,
                    span,
                    format!("output 无字段 `{field}`"),
                ));
                return Ty::Error;
            }
            Ty::Entity(name) => {
                if let Some(ent) = self.model.entities.get(name) {
                    if let Some(t) = ent.field_ty(field) {
                        return t.clone();
                    }
                    self.diags.push(SemanticDiagnostic::new(
                        K::NoSuchField,
                        span,
                        format!("entity `{name}` 无字段 `{field}`"),
                    ));
                    return Ty::Error;
                }
            }
            _ => {}
        }
        // base 为 Error/Unknown 或特殊根（File / Http 等）：恢复。
        Ty::Unknown
    }

    /// 方法调用结果类型（起步子集仅 `list.append(item) -> List`）。
    ///
    /// body 级标准库 I/O 操作（`File.Read` / `Http.Get`）由 [`Self::infer_effect_op`] 单独处理；
    /// 其它方法返回 `Unknown` 渐进恢复。
    fn method_ty(&mut self, base: &Ty, method: &str) -> Ty {
        match base.strip_intent() {
            Ty::List(elem) if method == "append" => Ty::List(elem.clone()),
            _ => Ty::Unknown,
        }
    }

    /// body 级标准库 I/O（特殊根 method_call）：`File.Read(path)` / `File.Write(path, content)` /
    /// `Http.Get(url)`。
    ///
    /// 仅当 `base` 是**库特殊根** family（如 `File` / `Http` / 三方库）时返回 `Some(结果类型)`，
    /// 否则 `None`（交回常规 method）。识别后据库注册表的 op 契约（`index.library_op`）**表驱动**
    /// 校验：推导实参、校验形参类型（含 intent 边界）、并入 effect（身份不带资源 arg）、给出返回
    /// 类型。见 docs/stdlib_design.md——核心不硬编码任何具体库的签名，全部从清单契约派生。
    ///
    /// 未知操作 → 报诊断并以 `Ty::Error` 恢复（仍算 effect-op 形状，不退回常规 method）。
    fn infer_effect_op(
        &mut self,
        base: ExprId,
        method: &str,
        args: &[ExprId],
        scope: &mut TypeScope,
        effects: &mut EffectSet,
    ) -> Option<Ty> {
        // base 必须是库特殊根标识符（family），由库注册表注入 index。
        let Expr::Ident(root) = self.ast.expr(base) else {
            return None;
        };
        let family = root.text.clone();
        if !self.index.is_library_family(&family) {
            return None;
        }
        let span = self.ast.expr(base).span();

        // 推导实参类型。
        let arg_tys: Vec<Ty> = args
            .iter()
            .map(|&a| self.infer(a, scope, effects))
            .collect();

        // 查 op 契约（清单驱动）。未知 op → 诊断 + Error 恢复。
        let Some(contract) = self.index.library_op(&family, method).cloned() else {
            self.diags.push(SemanticDiagnostic::new(
                K::NoSuchField,
                span,
                format!("{family} 不支持操作 `{method}`"),
            ));
            return Some(Ty::Error);
        };

        // effect 身份不带资源 arg（声明位 0 参），仅 effectful op 并入 effect 集。
        if contract.effectful {
            effects.insert(Effect::new(&contract.family, &contract.op, vec![]));
        }

        // 表驱动校验形参类型（含 intent 严格相等）。
        for (i, param_desc) in contract.params.iter().enumerate() {
            let want = typedesc_to_ty(param_desc);
            let op_label = format!("{}.{}", contract.family, contract.op);
            self.expect_io_arg(&arg_tys, i, &want, &op_label, i, span);
        }
        // 实参多于声明 → 诊断（少于声明已由 expect_io_arg 的缺参分支覆盖）。
        if arg_tys.len() > contract.params.len() {
            self.diags.push(SemanticDiagnostic::new(
                K::TypeMismatch,
                span,
                format!(
                    "{}.{} 期望 {} 个实参，实际 {} 个",
                    contract.family,
                    contract.op,
                    contract.params.len(),
                    arg_tys.len()
                ),
            ));
        }

        Some(typedesc_to_ty(&contract.returns))
    }

    /// 校验标准库 I/O 操作第 `idx` 个实参类型与期望相容（缺参或类型不符均报诊断）。
    fn expect_io_arg(
        &mut self,
        arg_tys: &[Ty],
        idx: usize,
        expected: &Ty,
        op: &str,
        role: usize,
        span: sophia_syntax::Span,
    ) {
        match arg_tys.get(idx) {
            Some(at) if !at.assignable_to(expected) => {
                let kind = if at.is_intent() || expected.is_intent() {
                    K::IntentMismatch
                } else {
                    K::TypeMismatch
                };
                self.diags.push(SemanticDiagnostic::new(
                    kind,
                    span,
                    format!(
                        "{op} 的第 {} 个实参类型 {at} 与期望 {expected} 不符",
                        role + 1
                    ),
                ));
            }
            None => {
                self.diags.push(SemanticDiagnostic::new(
                    K::TypeMismatch,
                    span,
                    format!("{op} 缺少第 {} 个实参", role + 1),
                ));
            }
            _ => {}
        }
    }

    /// entity 构造检查：全字段覆盖、未知字段、字段类型匹配（含 intent 严格相等）。
    fn check_construct(
        &mut self,
        name: &str,
        provided: &[(String, Ty, sophia_syntax::Span)],
        span: sophia_syntax::Span,
    ) -> Ty {
        if let Some(ent) = self.model.entities.get(name) {
            let fields = ent.fields.clone();
            self.check_record_construct("entity", name, &fields, provided, span);
            return Ty::Entity(name.to_string());
        }

        if let Some(variant) = self.model.variants.get(name) {
            let fields = variant.fields.clone();
            self.check_record_construct("error variant", name, &fields, provided, span);
            return Ty::ErrorVariant(name.to_string());
        }

        if let Some(decl) = self.model.callables.get(name) {
            let inputs = decl.inputs.clone();
            let output = decl.sole_output_ty().cloned().unwrap_or(Ty::Unknown);
            self.check_record_construct("transition", name, &inputs, provided, span);
            return output;
        }

        Ty::Unknown
    }

    fn check_record_construct(
        &mut self,
        kind_label: &str,
        name: &str,
        expected_fields: &[(String, Ty)],
        provided: &[(String, Ty, sophia_syntax::Span)],
        span: sophia_syntax::Span,
    ) {
        // 未知字段 + 类型匹配。
        for (fname, fty, fspan) in provided {
            match expected_fields.iter().find(|(field, _)| field == fname) {
                Some((_, expected)) => {
                    if !fty.assignable_to(expected) {
                        let kind = if involves_intent(fty, expected) {
                            K::IntentMismatch
                        } else {
                            K::TypeMismatch
                        };
                        self.diags.push(SemanticDiagnostic::new(
                            kind,
                            *fspan,
                            format!("字段 `{fname}`：期望 {expected}，实际 {fty}"),
                        ));
                    }
                }
                None => self.diags.push(SemanticDiagnostic::new(
                    K::UnknownField,
                    *fspan,
                    format!("{kind_label} `{name}` 无字段 `{fname}`"),
                )),
            }
        }
        // 缺字段。
        for (fname, _) in expected_fields {
            if !provided.iter().any(|(p, _, _)| p == fname) {
                self.diags.push(SemanticDiagnostic::new(
                    K::MissingField,
                    span,
                    format!("构造 `{name}` 缺少字段 `{fname}`"),
                ));
            }
        }
    }

    // ---- 检查辅助 ----

    fn check_assignable(&mut self, id: ExprId, found: &Ty, expected: &Ty, ctx: &str) {
        let _ = id;
        if !found.assignable_to(expected) {
            let kind = if involves_intent(found, expected) {
                K::IntentMismatch
            } else {
                K::TypeMismatch
            };
            self.diags.push(SemanticDiagnostic::new(
                kind,
                self.ast.expr(id).span(),
                format!("{ctx}：期望 {expected}，实际 {found}"),
            ));
        }
    }

    fn expect_bool(&mut self, id: ExprId, t: &Ty, ctx: &str) {
        if !t.is_gradual() && t.strip_intent() != &Ty::Bool {
            self.diags.push(SemanticDiagnostic::new(
                K::TypeMismatch,
                self.ast.expr(id).span(),
                format!("{ctx} 应为 Bool，实际 {t}"),
            ));
        }
    }

    fn expect_int(&mut self, id: ExprId, t: &Ty, ctx: &str) {
        if !t.is_gradual() && t.strip_intent() != &Ty::Int {
            self.diags.push(SemanticDiagnostic::new(
                K::TypeMismatch,
                self.ast.expr(id).span(),
                format!("{ctx} 应为 Int，实际 {t}"),
            ));
        }
    }

    /// `Console.Write` 只能输出字面量、`Sanitized<T>` 或 `Redacted<T>`（7.2 边界）。
    fn check_console_output(&mut self, id: ExprId, t: &Ty) {
        // 字面量直接允许。
        if matches!(
            self.ast.expr(id),
            Expr::Str(_) | Expr::Int { .. } | Expr::Bool { .. }
        ) {
            return;
        }
        if t.is_gradual() {
            return;
        }
        let ok = matches!(
            t,
            Ty::Intent(IntentKind::Sanitized, _) | Ty::Intent(IntentKind::Redacted, _)
        );
        if !ok {
            self.diags.push(SemanticDiagnostic::new(
                K::ConsoleOutputIntent,
                self.ast.expr(id).span(),
                format!("Console.Write 只能输出字面量 / Sanitized<T> / Redacted<T>，实际 {t}"),
            ));
        }
    }

    /// match 穷尽性（7.5、设计第七节）：Bool / state / `one of { ... }`，永久禁止 `_`。
    fn check_match_exhaustive(
        &mut self,
        subject: &Ty,
        arms: &[sophia_syntax::MatchArm],
        span: sophia_syntax::Span,
    ) {
        if subject.is_gradual() {
            return;
        }
        match subject.strip_intent() {
            Ty::Bool => {
                let mut has_true = false;
                let mut has_false = false;
                for a in arms {
                    if let Pattern::Bool { value, .. } = a.pattern {
                        if value {
                            has_true = true;
                        } else {
                            has_false = true;
                        }
                    }
                }
                if !(has_true && has_false) {
                    self.diags.push(SemanticDiagnostic::new(
                        K::NonExhaustiveMatch,
                        span,
                        "Bool match 必须覆盖 true 与 false",
                    ));
                }
            }
            Ty::OneOf(members) => {
                // one of 穷尽：每个成员都须被一个匹配 tag 的 pattern 覆盖（永久禁止 `_`）。
                for m in members {
                    let covered = arms.iter().any(|a| pattern_covers_member(&a.pattern, m));
                    if !covered {
                        self.diags.push(SemanticDiagnostic::new(
                            K::NonExhaustiveMatch,
                            span,
                            format!("one of match 未覆盖成员 `{m}`"),
                        ));
                    }
                }
            }
            Ty::State(sname) => {
                if let Some(state) = self.model.states.get(sname) {
                    for v in &state.values {
                        let covered = arms.iter().any(|a| match &a.pattern {
                            Pattern::State { value, .. } => &value.text == v,
                            _ => false,
                        });
                        if !covered {
                            self.diags.push(SemanticDiagnostic::new(
                                K::NonExhaustiveMatch,
                                span,
                                format!("state match 未覆盖值 `{}.{}`", sname, v),
                            ));
                        }
                    }
                }
            }
            other => {
                self.diags.push(SemanticDiagnostic::new(
                    K::InvalidMatchSubject,
                    span,
                    format!("match 只支持 Bool / state / one of，实际 {other}"),
                ));
            }
        }
    }
}

/// 判断一个 pattern 是否覆盖 `one of` 的某成员类型（按 match tag）。
fn pattern_covers_member(pattern: &Pattern, member: &Ty) -> bool {
    match (pattern, member.strip_intent()) {
        // Null pattern 覆盖 Null 成员。
        (Pattern::Null { .. }, Ty::Null) => true,
        // error variant pattern 覆盖同名 error variant 成员。
        (Pattern::Variant { variant, .. }, Ty::ErrorVariant(name)) => variant.text == *name,
        // 类型 pattern `Int x` 按类型名覆盖标量 / entity / state 成员。
        (Pattern::Type { ty, .. }, m) => type_name_matches(&ty.text, m),
        _ => false,
    }
}

/// 类型 pattern 的类型名是否匹配某成员 `Ty`（标量 / entity / state）。
fn type_name_matches(name: &str, m: &Ty) -> bool {
    match m {
        Ty::Unit => name == "Unit",
        Ty::Bool => name == "Bool",
        Ty::Int => name == "Int",
        Ty::Text => name == "Text",
        Ty::Uuid => name == "Uuid",
        Ty::Time => name == "Time",
        Ty::Null => name == "Null",
        Ty::Entity(n) | Ty::State(n) => name == n,
        _ => false,
    }
}

/// 拆出最外层 intent：返回 (inner 类型, intent 种类)。
fn split_intent(t: &Ty) -> (Ty, Option<IntentKind>) {
    match t {
        Ty::Intent(k, inner) => ((**inner).clone(), Some(*k)),
        other => (other.clone(), None),
    }
}

/// 两侧是否有任一带 intent（用于区分 IntentMismatch 与 TypeMismatch 诊断码）。
fn involves_intent(a: &Ty, b: &Ty) -> bool {
    a.is_intent() || b.is_intent()
}

/// `output` 根类型：以各 output 参数为字段的记录（用于 ensures 谓词作用域）。
///
/// 设计第五节示例使用 `output.<param>.<field>` 访问形式，因此 `output` 是一个
/// 以 output 参数名为字段的记录，而非单个输出值的类型。
fn output_root_ty(decl: &crate::model::CallableDecl) -> Ty {
    Ty::Record(
        decl.outputs
            .iter()
            .map(|(n, t)| (n.clone(), t.clone()))
            .collect(),
    )
}
