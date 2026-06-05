//! 契约层：capability 满足与 error 传播。
//!
//! 见 docs/language_implementation.md 7.4、7.5。检查：
//! - capability：action effect 必须被 capability allow 且未命中 deny（deny 优先）；
//!   产生 effect 却无 capability 绑定 → 缺 capability；
//! - error：`raise` 的 variant 必须在 callable `errors` 中声明；被调用 action 的
//!   errors 必须由调用方继续声明。
//!
//! 与类型层一致采用容错收集。

use crate::effect::{Effect, EffectSet};
use crate::error::{SemanticDiagnostic, SemanticDiagnosticKind as K};
use crate::model::{CallableDecl, SemanticModel};
use sophia_syntax::{Ast, Callable, Expr, ExprId, Span, Stmt};
use std::collections::BTreeSet;

/// 对一个 callable 做契约层检查（capability + error）。
pub fn check_contracts(
    callable: &Callable,
    decl: &CallableDecl,
    used: &EffectSet,
    model: &SemanticModel,
    ast: &Ast,
    diags: &mut Vec<SemanticDiagnostic>,
) {
    check_capability(decl, used, model, callable.span, diags);
    check_errors(callable, decl, model, ast, diags);
}

/// capability 检查：deny 优先于 allow。
fn check_capability(
    decl: &CallableDecl,
    used: &EffectSet,
    model: &SemanticModel,
    decl_span: Span,
    diags: &mut Vec<SemanticDiagnostic>,
) {
    check_capability_satisfied(
        &decl.name,
        decl.capability.as_deref(),
        used,
        model,
        decl_span,
        diags,
    );
}

/// capability 满足检查的通用核心：
/// 有 effect 必须绑 capability；每个 effect 必须被 allow 且未命中 deny（deny 优先）。
pub(crate) fn check_capability_satisfied(
    owner: &str,
    capability: Option<&str>,
    used: &EffectSet,
    model: &SemanticModel,
    decl_span: Span,
    diags: &mut Vec<SemanticDiagnostic>,
) {
    // 纯（无 effect）不要求 capability。
    if used.is_pure() {
        return;
    }
    let Some(cap_name) = capability else {
        diags.push(SemanticDiagnostic::new(
            K::MissingCapability,
            decl_span,
            format!("`{owner}` 产生 effect 但未绑定 capability"),
        ));
        return;
    };
    let Some(cap) = model.capabilities.get(cap_name) else {
        // capability 名未解析（HIR 已报）；不重复。
        return;
    };

    for e in used.iter() {
        // 每个 used effect 都须被 capability 显式 allow（且不被 deny）才算授权——
        // `Console.Write` / `File.Read` / `Http.Get` 等一视同仁。
        if effect_denied(cap, e) {
            diags.push(SemanticDiagnostic::new(
                K::CapabilityDenied,
                decl_span,
                format!("effect {e} 命中 capability `{cap_name}` 的 deny",),
            ));
        } else if !effect_allowed(cap, e) {
            diags.push(SemanticDiagnostic::new(
                K::CapabilityDenied,
                decl_span,
                format!("effect {e} 未被 capability `{cap_name}` allow"),
            ));
        }
    }
}

/// deny 命中判定（deny 优先）。
fn effect_denied(cap: &crate::model::CapabilityDecl, e: &Effect) -> bool {
    cap.deny.iter().any(|d| effect_matches(d, e))
}

/// allow 命中判定。
fn effect_allowed(cap: &crate::model::CapabilityDecl, e: &Effect) -> bool {
    cap.allow.iter().any(|a| effect_matches(a, e))
}

/// effect 是否匹配 capability 条目：family/op 相同，实参字面量需相等、绑定名通配
/// （见 [`Effect::covered_by`]）。
fn effect_matches(entry: &Effect, used: &Effect) -> bool {
    used.covered_by(entry)
}

/// error 检查：raise 的 variant 必须在 errors 声明；被调用方 errors 必须传播。
fn check_errors(
    callable: &Callable,
    decl: &CallableDecl,
    model: &SemanticModel,
    ast: &Ast,
    diags: &mut Vec<SemanticDiagnostic>,
) {
    let declared: BTreeSet<&str> = decl.declared_errors.iter().map(|s| s.as_str()).collect();

    // 收集 body 中 raise 的 variant 与调用的 callee。
    let mut raised: Vec<(String, Span)> = Vec::new();
    let mut called: Vec<String> = Vec::new();
    if let Some(body) = &callable.body {
        collect_body_errors(body, ast, &mut raised, &mut called);
    }

    // 1) raise 的 variant 必须声明。
    for (vname, span) in &raised {
        if !declared.contains(vname.as_str()) {
            diags.push(SemanticDiagnostic::new(
                K::UndeclaredError,
                *span,
                format!("raise 的 variant `{vname}` 未在 errors 中声明"),
            ));
        }
    }

    // 2) 被调用 action 的 errors 必须由调用方继续声明。
    for callee_name in &called {
        if let Some(callee) = model.callables.get(callee_name) {
            for e in &callee.declared_errors {
                if !declared.contains(e.as_str()) {
                    diags.push(SemanticDiagnostic::new(
                        K::ErrorNotPropagated,
                        callable.span,
                        format!(
                            "调用 `{callee_name}` 可能抛出 `{e}`，但 `{}` 未声明",
                            decl.name
                        ),
                    ));
                }
            }
        }
    }
}

/// 遍历 body 收集 raise 的 variant 名与被调用 callable 名。
fn collect_body_errors(
    block: &sophia_syntax::Block,
    ast: &Ast,
    raised: &mut Vec<(String, Span)>,
    called: &mut Vec<String>,
) {
    for stmt in &block.stmts {
        collect_stmt_errors(stmt, ast, raised, called);
    }
}

fn collect_stmt_errors(
    stmt: &Stmt,
    ast: &Ast,
    raised: &mut Vec<(String, Span)>,
    called: &mut Vec<String>,
) {
    match stmt {
        Stmt::Raise { value, .. } => {
            // raise 的值是 Construct，其 name 即 variant。
            if let Expr::Construct { name, .. } = ast.expr(*value) {
                raised.push((name.text.clone(), name.span));
            }
            collect_expr_calls(*value, ast, called);
        }
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => collect_expr_calls(*value, ast, called),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            collect_expr_calls(*condition, ast, called);
            collect_body_errors(consequence, ast, raised, called);
            match alternative {
                Some(sophia_syntax::ElseBranch::Block(b)) => {
                    collect_body_errors(b, ast, raised, called)
                }
                Some(sophia_syntax::ElseBranch::If(s)) => {
                    collect_stmt_errors(s, ast, raised, called)
                }
                None => {}
            }
        }
        Stmt::Match { subject, arms, .. } => {
            collect_expr_calls(*subject, ast, called);
            for arm in arms {
                collect_body_errors(&arm.body, ast, raised, called);
            }
        }
        Stmt::Repeat { count, body, .. } => {
            collect_expr_calls(*count, ast, called);
            collect_body_errors(body, ast, raised, called);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr_calls(*condition, ast, called);
            collect_body_errors(body, ast, raised, called);
        }
    }
}

/// 收集表达式中调用的 callable 名（call_expr 的 callee）。
fn collect_expr_calls(id: ExprId, ast: &Ast, called: &mut Vec<String>) {
    match ast.expr(id) {
        Expr::Call { callee, args, .. } => {
            called.push(callee.text.clone());
            for &a in args {
                collect_expr_calls(a, ast, called);
            }
        }
        Expr::MethodCall { base, args, .. } => {
            collect_expr_calls(*base, ast, called);
            for &a in args {
                collect_expr_calls(a, ast, called);
            }
        }
        Expr::Construct { fields, .. } => {
            for fi in fields {
                collect_expr_calls(fi.value, ast, called);
            }
        }
        Expr::List { items, .. } => {
            for &it in items {
                collect_expr_calls(it, ast, called);
            }
        }
        Expr::Field { base, .. } => collect_expr_calls(*base, ast, called),
        Expr::Not { operand, .. } => collect_expr_calls(*operand, ast, called),
        Expr::Neg { operand, .. } => collect_expr_calls(*operand, ast, called),
        Expr::Binary { left, right, .. } => {
            collect_expr_calls(*left, ast, called);
            collect_expr_calls(*right, ast, called);
        }
        Expr::Str(_)
        | Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Null { .. }
        | Expr::Ident(_) => {}
    }
}
