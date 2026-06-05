//! CST → AST 转换（lowering）。
//!
//! 见 docs/language_implementation.md 第四节：CST → AST 是独立转换步骤，丢弃
//! trivia（空白、注释），保留 span。本模块只做表层结构搬运，不做名称解析、
//! 类型推导或任何语义判断（那是 HIR / Semantic IR 的职责）。
//!
//! 容错策略：lowering 假定输入 CST 无语法错误（调用方应先用
//! [`crate::SyntaxTree::errors`] 过滤）。遇到结构上意外缺失的子节点时，
//! 跳过该项而不 panic，保证对部分错误输入仍能产出尽量完整的 AST。

use crate::ast::*;
use crate::span::Span;
use tree_sitter::Node;

/// 把一棵无语法错误的 CST 转换为 AST。
pub(crate) fn lower(root: Node, source: &str) -> Ast {
    let mut ast = Ast::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(item) = lower_item(child, source, &mut ast) {
            ast.items.push(item);
        }
    }
    ast
}

/// lowering 上下文中读取节点文本的便捷封装。
fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes())
        .expect("节点字节区间源自本源码，必为有效 UTF-8")
}

fn span(node: Node) -> Span {
    Span::from_node(&node)
}

/// 取节点的首个具名子节点，**跳过 trivia（注释）**。
///
/// 注释在 grammar 中是 `extras`，会作为具名节点出现在任意位置（包括表达式内部，
/// 如 `foo(/* c */ 5)`）。CST → AST 必须丢弃 trivia（见 docs/language_implementation.md
/// 第四节），因此这里过滤 `comment`。使用 `named_child(i)`（返回节点生命周期绑定到
/// 语法树而非临时游标），避免借用周期问题。
fn first_named_child(node: Node) -> Option<Node> {
    let mut i = 0;
    while let Some(child) = node.named_child(i) {
        if child.kind() != "comment" {
            return Some(child);
        }
        i += 1;
    }
    None
}

fn ident(node: Node, source: &str) -> Ident {
    Ident {
        text: text(node, source).to_string(),
        span: span(node),
    }
}

/// 取字段并转 Ident。
fn field_ident(node: Node, field: &str, source: &str) -> Option<Ident> {
    node.child_by_field_name(field).map(|n| ident(n, source))
}

/// 解析字符串字面量：去掉首尾引号并处理常见转义。
fn str_lit(node: Node, source: &str) -> StrLit {
    let raw = text(node, source);
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    StrLit {
        value: unescape(inner),
        span: span(node),
    }
}

/// 处理字符串字面量中的转义序列。
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ============ 顶层声明 ============

fn lower_item(node: Node, source: &str, ast: &mut Ast) -> Option<Item> {
    match node.kind() {
        "domain_def" => lower_domain(node, source).map(Item::Domain),
        "entity_def" => lower_entity(node, source, ast).map(Item::Entity),
        "state_def" => lower_state(node, source).map(Item::State),
        "transition_def" => {
            lower_callable(node, source, ast, CallableKind::Transition).map(Item::Transition)
        }
        "error_def" => lower_error(node, source).map(Item::Error),
        "capability_def" => lower_capability(node, source).map(Item::Capability),
        "action_def" => lower_callable(node, source, ast, CallableKind::Action).map(Item::Action),
        "task_def" => lower_task(node, source).map(Item::Task),
        "effect_def" => lower_effect_def(node, source).map(Item::Effect),
        _ => None,
    }
}

fn lower_domain(node: Node, source: &str) -> Option<Domain> {
    let name = field_ident(node, "name", source)?;
    let body = node.child_by_field_name("body");
    let assists = body.map(|b| collect_assists(b, source)).unwrap_or_default();
    Some(Domain {
        name,
        assists,
        span: span(node),
    })
}

fn lower_entity(node: Node, source: &str, ast: &mut Ast) -> Option<Entity> {
    let name = field_ident(node, "name", source)?;
    let body = node.child_by_field_name("body")?;

    let mut assists = Vec::new();
    let mut fields = Vec::new();
    let mut invariants = Vec::new();
    let mut semantic_identity = None;
    let mut evolution = None;

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "assist_field" => {
                if let Some(a) = lower_assist(member, source) {
                    assists.push(a);
                }
            }
            "fields_block" => fields.extend(lower_fields_block(member, source)),
            "invariants_block" => invariants.extend(lower_invariants_block(member, source, ast)),
            "semantic_identity_block" => {
                semantic_identity = lower_semantic_identity(member, source);
            }
            "evolution_block" => evolution = lower_evolution(member, source),
            _ => {}
        }
    }

    Some(Entity {
        name,
        assists,
        fields,
        invariants,
        semantic_identity,
        evolution,
        span: span(node),
    })
}

fn lower_fields_block(node: Node, source: &str) -> Vec<FieldDecl> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        if decl.kind() != "field_decl" {
            continue;
        }
        let (Some(name), Some(ty)) = (
            field_ident(decl, "name", source),
            decl.child_by_field_name("type")
                .and_then(|t| lower_type(t, source)),
        ) else {
            continue;
        };
        out.push(FieldDecl {
            name,
            ty,
            span: span(decl),
        });
    }
    out
}

fn lower_invariants_block(node: Node, source: &str, ast: &mut Ast) -> Vec<Invariant> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        if decl.kind() != "invariant_decl" {
            continue;
        }
        let Some(name) = field_ident(decl, "name", source) else {
            continue;
        };
        let when = decl
            .child_by_field_name("when")
            .and_then(|b| lower_block_expr(b, source, ast));
        let require = decl
            .child_by_field_name("require")
            .and_then(|b| lower_block_expr(b, source, ast));
        out.push(Invariant {
            name,
            when,
            require,
            span: span(decl),
        });
    }
    out
}

/// `block_expr` 是 `{ expr }`，取其唯一具名子节点为表达式。
fn lower_block_expr(node: Node, source: &str, ast: &mut Ast) -> Option<ExprId> {
    let inner = first_named_child(node)?;
    lower_expr(inner, source, ast)
}

fn lower_semantic_identity(node: Node, source: &str) -> Option<SemanticIdentity> {
    let mut core_capability = Vec::new();
    let mut forbidden_drift = Vec::new();
    let mut drift_tolerance = None;

    // key / value 是平行的 multiple 字段，按出现顺序成对消费。
    let mut cursor = node.walk();
    let mut keys = Vec::new();
    let mut vals = Vec::new();
    for child in node.children_by_field_name("key", &mut cursor) {
        keys.push(child);
    }
    let mut cursor2 = node.walk();
    for child in node.children_by_field_name("value", &mut cursor2) {
        vals.push(child);
    }
    for (k, v) in keys.into_iter().zip(vals) {
        match text(k, source) {
            "core_capability" => core_capability = collect_bracket_strings(v, source),
            "forbidden_drift" => forbidden_drift = collect_bracket_strings(v, source),
            "drift_tolerance" => drift_tolerance = Some(text(v, source).to_string()),
            _ => {}
        }
    }

    Some(SemanticIdentity {
        core_capability,
        forbidden_drift,
        drift_tolerance,
        span: span(node),
    })
}

fn lower_evolution(node: Node, source: &str) -> Option<Evolution> {
    let mut allowed = Vec::new();
    let mut forbidden = Vec::new();
    let mut requires_gate = Vec::new();

    let mut kc = node.walk();
    let keys: Vec<Node> = node.children_by_field_name("key", &mut kc).collect();
    let mut vc = node.walk();
    let vals: Vec<Node> = node.children_by_field_name("value", &mut vc).collect();
    for (k, v) in keys.into_iter().zip(vals) {
        match text(k, source) {
            "allowed" => allowed = collect_bracket_strings(v, source),
            "forbidden" => forbidden = collect_bracket_strings(v, source),
            "requires_gate" => requires_gate = collect_bracket_strings(v, source),
            _ => {}
        }
    }

    Some(Evolution {
        allowed,
        forbidden,
        requires_gate,
        span: span(node),
    })
}

fn lower_state(node: Node, source: &str) -> Option<StateDef> {
    let name = field_ident(node, "name", source)?;
    let mut values = Vec::new();
    let mut cursor = node.walk();
    for sv in node.named_children(&mut cursor) {
        if sv.kind() != "state_value" {
            continue;
        }
        let Some(vname) = field_ident(sv, "name", source) else {
            continue;
        };
        values.push(StateValue {
            name: vname,
            assists: collect_assists(sv, source),
            span: span(sv),
        });
    }
    Some(StateDef {
        name,
        values,
        span: span(node),
    })
}

fn lower_error(node: Node, source: &str) -> Option<ErrorDef> {
    let name = field_ident(node, "name", source)?;
    let mut variants = Vec::new();
    let mut cursor = node.walk();
    for v in node.named_children(&mut cursor) {
        if v.kind() != "error_variant" {
            continue;
        }
        let Some(vname) = field_ident(v, "name", source) else {
            continue;
        };
        let mut fields = Vec::new();
        let mut fc = v.walk();
        for f in v.named_children(&mut fc) {
            if f.kind() != "variant_field" {
                continue;
            }
            let (Some(fname), Some(ty)) = (
                field_ident(f, "name", source),
                f.child_by_field_name("type")
                    .and_then(|t| lower_type(t, source)),
            ) else {
                continue;
            };
            fields.push(VariantField {
                name: fname,
                ty,
                span: span(f),
            });
        }
        variants.push(ErrorVariant {
            name: vname,
            fields,
            span: span(v),
        });
    }
    Some(ErrorDef {
        name,
        variants,
        span: span(node),
    })
}

fn lower_capability(node: Node, source: &str) -> Option<Capability> {
    let name = field_ident(node, "name", source)?;
    let mut allow = Vec::new();
    let mut deny = Vec::new();
    let mut cursor = node.walk();
    for block in node.named_children(&mut cursor) {
        match block.kind() {
            "allow_block" => allow.extend(collect_effects(block, source)),
            "deny_block" => deny.extend(collect_effects(block, source)),
            _ => {}
        }
    }
    Some(Capability {
        name,
        allow,
        deny,
        span: span(node),
    })
}

fn lower_effect_def(node: Node, source: &str) -> Option<EffectDef> {
    let name = field_ident(node, "name", source)?;
    let mut assists = Vec::new();
    let mut operations = Vec::new();
    let mut cursor = node.walk();
    for member in node.named_children(&mut cursor) {
        match member.kind() {
            "assist_field" => {
                if let Some(a) = lower_assist(member, source) {
                    assists.push(a);
                }
            }
            "effect_operation" => {
                if let Some(op) = lower_effect_operation(member, source) {
                    operations.push(op);
                }
            }
            _ => {}
        }
    }
    Some(EffectDef {
        name,
        assists,
        operations,
        span: span(node),
    })
}

fn lower_effect_operation(node: Node, source: &str) -> Option<EffectOperation> {
    let name = field_ident(node, "name", source)?;
    let mut params = Vec::new();
    let mut cursor = node.walk();
    for p in node.named_children(&mut cursor) {
        if p.kind() != "effect_param" {
            continue;
        }
        let (Some(pname), Some(ty)) = (
            field_ident(p, "name", source),
            p.child_by_field_name("type")
                .and_then(|t| lower_type(t, source)),
        ) else {
            continue;
        };
        params.push(EffectParam {
            name: pname,
            ty,
            span: span(p),
        });
    }
    Some(EffectOperation {
        name,
        params,
        span: span(node),
    })
}

fn lower_callable(node: Node, source: &str, ast: &mut Ast, kind: CallableKind) -> Option<Callable> {
    let name = field_ident(node, "name", source)?;

    let mut assists = Vec::new();
    let mut capability = None;
    let mut intent_conversion = false;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut effects = Vec::new();
    let mut errors = Vec::new();
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    let mut body = None;

    let mut cursor = node.walk();
    for member in node.named_children(&mut cursor) {
        match member.kind() {
            "assist_field" => {
                if let Some(a) = lower_assist(member, source) {
                    assists.push(a);
                }
            }
            "capability_binding" => capability = field_ident(member, "name", source),
            "intent_conversion_flag" => {
                intent_conversion = member
                    .child_by_field_name("value")
                    .map(|v| text(v, source) == "true")
                    .unwrap_or(false);
            }
            "input_block" => inputs.extend(lower_params(member, source, ast)),
            "output_block" => outputs.extend(lower_params(member, source, ast)),
            "effects_block" => effects.extend(collect_effects(member, source)),
            "errors_block" => errors.extend(collect_error_refs(member, source)),
            "requires_block" => requires.extend(collect_block_exprs(member, source, ast)),
            "ensures_block" => ensures.extend(collect_block_exprs(member, source, ast)),
            "body_block" => body = lower_body_block(member, source, ast),
            _ => {}
        }
    }

    Some(Callable {
        kind,
        name,
        assists,
        capability,
        intent_conversion,
        inputs,
        outputs,
        effects,
        errors,
        requires,
        ensures,
        body,
        span: span(node),
    })
}

fn lower_params(block: Node, source: &str, ast: &mut Ast) -> Vec<Param> {
    let mut out = Vec::new();
    // input_block/output_block 内只有一个可选的 param_list。
    let mut bc = block.walk();
    let Some(list) = block
        .named_children(&mut bc)
        .find(|n| n.kind() == "param_list")
    else {
        return out;
    };
    let mut cursor = list.walk();
    for p in list.named_children(&mut cursor) {
        if p.kind() != "param_decl" {
            continue;
        }
        let (Some(name), Some(ty)) = (
            field_ident(p, "name", source),
            p.child_by_field_name("type")
                .and_then(|t| lower_type(t, source)),
        ) else {
            continue;
        };
        let predicate = p
            .child_by_field_name("predicate")
            .and_then(|e| lower_expr(e, source, ast));
        out.push(Param {
            name,
            ty,
            predicate,
            span: span(p),
        });
    }
    out
}

fn collect_error_refs(block: Node, source: &str) -> Vec<Ident> {
    let mut out = Vec::new();
    let mut cursor = block.walk();
    for n in block.named_children(&mut cursor) {
        if n.kind() == "identifier" {
            out.push(ident(n, source));
        }
    }
    out
}

fn collect_block_exprs(block: Node, source: &str, ast: &mut Ast) -> Vec<ExprId> {
    let mut out = Vec::new();
    let mut cursor = block.walk();
    for n in block.named_children(&mut cursor) {
        if let Some(id) = lower_expr(n, source, ast) {
            out.push(id);
        }
    }
    out
}

fn lower_body_block(node: Node, source: &str, ast: &mut Ast) -> Option<Block> {
    let mut cursor = node.walk();
    let block = node
        .named_children(&mut cursor)
        .find(|n| n.kind() == "block")?;
    Some(lower_block(block, source, ast))
}

fn lower_task(node: Node, source: &str) -> Option<Task> {
    let name = field_ident(node, "name", source)?;
    let goal = node
        .child_by_field_name("value")
        .map(|v| str_lit(v, source));

    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut cursor = node.walk();
    for member in node.named_children(&mut cursor) {
        match member.kind() {
            "include_block" => {
                let mut ic = member.walk();
                for decl in member.named_children(&mut ic) {
                    if decl.kind() != "include_decl" {
                        continue;
                    }
                    let (Some(kind), Some(iname)) = (
                        decl.child_by_field_name("kind")
                            .and_then(|k| include_kind(text(k, source))),
                        field_ident(decl, "name", source),
                    ) else {
                        continue;
                    };
                    includes.push(IncludeDecl {
                        kind,
                        name: iname,
                        span: span(decl),
                    });
                }
            }
            "exclude_block" => excludes.extend(collect_effects(member, source)),
            _ => {}
        }
    }

    Some(Task {
        name,
        goal,
        includes,
        excludes,
        span: span(node),
    })
}

fn include_kind(s: &str) -> Option<IncludeKind> {
    Some(match s {
        "entity" => IncludeKind::Entity,
        "state" => IncludeKind::State,
        "error" => IncludeKind::Error,
        "capability" => IncludeKind::Capability,
        "transition" => IncludeKind::Transition,
        "action" => IncludeKind::Action,
        _ => return None,
    })
}

// ============ 共享辅助 ============

/// 收集一个容器节点直接子层的全部 `assist_field`。
fn collect_assists(node: Node, source: &str) -> Vec<AssistField> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "assist_field" {
            if let Some(a) = lower_assist(child, source) {
                out.push(a);
            }
        }
    }
    out
}

fn lower_assist(node: Node, source: &str) -> Option<AssistField> {
    let key = assist_key(node.child_by_field_name("key").map(|k| text(k, source))?)?;
    let mut values = Vec::new();
    if let Some(v) = node.child_by_field_name("value") {
        match v.kind() {
            "string" => values.push(str_lit(v, source)),
            "string_list" => {
                let mut cursor = v.walk();
                for s in v.named_children(&mut cursor) {
                    if s.kind() == "string" {
                        values.push(str_lit(s, source));
                    }
                }
            }
            _ => {}
        }
    }
    Some(AssistField {
        key,
        values,
        span: span(node),
    })
}

fn assist_key(s: &str) -> Option<AssistKey> {
    Some(match s {
        "meaning" => AssistKey::Meaning,
        "purpose" => AssistKey::Purpose,
        "because" => AssistKey::Because,
        "not" => AssistKey::Not,
        "examples" => AssistKey::Examples,
        "anti_patterns" => AssistKey::AntiPatterns,
        "plan" => AssistKey::Plan,
        "repair_notes" => AssistKey::RepairNotes,
        _ => return None,
    })
}

fn collect_bracket_strings(node: Node, source: &str) -> Vec<StrLit> {
    let mut out = Vec::new();
    if node.kind() != "bracket_string_list" {
        return out;
    }
    let mut cursor = node.walk();
    for s in node.named_children(&mut cursor) {
        if s.kind() == "string" {
            out.push(str_lit(s, source));
        }
    }
    out
}

fn collect_effects(block: Node, source: &str) -> Vec<Effect> {
    let mut out = Vec::new();
    let mut cursor = block.walk();
    for n in block.named_children(&mut cursor) {
        if n.kind() == "effect_ref" {
            if let Some(e) = lower_effect(n, source) {
                out.push(e);
            }
        }
    }
    out
}

fn lower_effect(node: Node, source: &str) -> Option<Effect> {
    // effect_ref 形态：`Pure` 或 `Family.Op` / `Family.Op(args)`。
    let family = node.child_by_field_name("family");
    let op = node.child_by_field_name("op");
    match (family, op) {
        (Some(family), Some(op)) => {
            let args = collect_effect_args(node, source);
            Some(Effect::Op {
                family: ident(family, source),
                op: ident(op, source),
                args,
                span: span(node),
            })
        }
        // 无 family/op 字段即保留字 `Pure`。
        _ => Some(Effect::Pure),
    }
}

/// 收集 effect_ref 的实参（字面量或绑定名）。
///
/// 注意：`family` / `op` 也是 effect_ref 的 `identifier` 子节点，需排除；
/// 只有 `(...)` 内的参数才是实参。
fn collect_effect_args(node: Node, source: &str) -> Vec<EffectArg> {
    let family_id = node.child_by_field_name("family").map(|n| n.id());
    let op_id = node.child_by_field_name("op").map(|n| n.id());
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for n in node.named_children(&mut cursor) {
        if Some(n.id()) == family_id || Some(n.id()) == op_id {
            continue;
        }
        let arg = match n.kind() {
            "string" => EffectArg::Str(str_lit(n, source)),
            "int" => EffectArg::Int {
                text: text(n, source).to_string(),
                span: span(n),
            },
            "bool" => EffectArg::Bool {
                value: text(n, source) == "true",
                span: span(n),
            },
            "identifier" => EffectArg::Ident(ident(n, source)),
            _ => continue,
        };
        out.push(arg);
    }
    out
}

// ============ 类型 ============

/// `type` 节点包裹一个 `named_type` / `intent_type` / `list_of` / `one_of` / `schema_of`。
fn lower_type(node: Node, source: &str) -> Option<TypeRef> {
    let inner = if node.kind() == "type" {
        first_named_child(node)?
    } else {
        node
    };
    match inner.kind() {
        "named_type" => {
            let id = first_named_child(inner)?;
            Some(TypeRef::Named {
                name: ident(id, source),
                span: span(inner),
            })
        }
        "intent_type" => {
            let head = field_ident(inner, "head", source)?;
            let arg = inner
                .child_by_field_name("arg")
                .and_then(|a| lower_type(a, source))?;
            Some(TypeRef::Intent {
                head,
                arg: Box::new(arg),
                span: span(inner),
            })
        }
        "list_of" => {
            let elem = inner
                .child_by_field_name("elem")
                .and_then(|e| lower_type(e, source))?;
            Some(TypeRef::ListOf {
                elem: Box::new(elem),
                span: span(inner),
            })
        }
        "one_of" => {
            let mut members = Vec::new();
            let mut cursor = inner.walk();
            for m in inner.children_by_field_name("member", &mut cursor) {
                if let Some(t) = lower_type(m, source) {
                    members.push(t);
                }
            }
            Some(TypeRef::OneOf {
                members,
                span: span(inner),
            })
        }
        "schema_of" => {
            let arg = inner
                .child_by_field_name("arg")
                .and_then(|a| lower_type(a, source))?;
            Some(TypeRef::SchemaOf {
                arg: Box::new(arg),
                span: span(inner),
            })
        }
        _ => None,
    }
}

// ============ body ============

fn lower_block(node: Node, source: &str, ast: &mut Ast) -> Block {
    let mut stmts = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(stmt) = lower_stmt(child, source, ast) {
            stmts.push(stmt);
        }
    }
    Block {
        stmts,
        span: span(node),
    }
}

/// 把单语句规范化为 block，供 match arm / 控制流统一持有。
fn stmt_to_block(stmt: Stmt) -> Block {
    let s = stmt.span();
    Block {
        stmts: vec![stmt],
        span: s,
    }
}

fn lower_stmt(node: Node, source: &str, ast: &mut Ast) -> Option<Stmt> {
    match node.kind() {
        "let_stmt" => {
            let name = field_ident(node, "name", source)?;
            let value = node
                .child_by_field_name("value")
                .and_then(|v| lower_expr(v, source, ast))?;
            // `mutable` 是匿名 token；通过扫描子节点判断其是否存在。
            let mutable = has_anonymous_token(node, "mutable");
            Some(Stmt::Let {
                mutable,
                name,
                value,
                span: span(node),
            })
        }
        "set_stmt" => {
            let name = field_ident(node, "name", source)?;
            let value = node
                .child_by_field_name("value")
                .and_then(|v| lower_expr(v, source, ast))?;
            Some(Stmt::Set {
                name,
                value,
                span: span(node),
            })
        }
        "return_stmt" => {
            let value = node
                .child_by_field_name("value")
                .and_then(|v| lower_expr(v, source, ast))?;
            Some(Stmt::Return {
                value,
                span: span(node),
            })
        }
        "raise_stmt" => {
            let value = node
                .child_by_field_name("value")
                .and_then(|v| lower_expr(v, source, ast))?;
            Some(Stmt::Raise {
                value,
                span: span(node),
            })
        }
        "print_stmt" => {
            let value = node
                .child_by_field_name("value")
                .and_then(|v| lower_expr(v, source, ast))?;
            Some(Stmt::Print {
                value,
                span: span(node),
            })
        }
        "if_stmt" => lower_if(node, source, ast),
        "match_stmt" => lower_match(node, source, ast),
        "repeat_stmt" => {
            let count = node
                .child_by_field_name("count")
                .and_then(|c| lower_expr(c, source, ast))?;
            let body = node
                .child_by_field_name("body")
                .map(|b| lower_block(b, source, ast))?;
            Some(Stmt::Repeat {
                count,
                body,
                span: span(node),
            })
        }
        "while_stmt" => {
            let condition = node
                .child_by_field_name("condition")
                .and_then(|c| lower_expr(c, source, ast))?;
            let body = node
                .child_by_field_name("body")
                .map(|b| lower_block(b, source, ast))?;
            Some(Stmt::While {
                condition,
                body,
                span: span(node),
            })
        }
        "expression_stmt" => {
            let inner = first_named_child(node)?;
            let value = lower_expr(inner, source, ast)?;
            Some(Stmt::Expr {
                value,
                span: span(node),
            })
        }
        _ => None,
    }
}

fn lower_if(node: Node, source: &str, ast: &mut Ast) -> Option<Stmt> {
    let condition = node
        .child_by_field_name("condition")
        .and_then(|c| lower_expr(c, source, ast))?;
    let consequence = node
        .child_by_field_name("consequence")
        .map(|b| lower_block(b, source, ast))?;
    let alternative = match node.child_by_field_name("alternative") {
        Some(alt) if alt.kind() == "if_stmt" => {
            lower_if(alt, source, ast).map(|s| ElseBranch::If(Box::new(s)))
        }
        Some(alt) => Some(ElseBranch::Block(lower_block(alt, source, ast))),
        None => None,
    };
    Some(Stmt::If {
        condition,
        consequence,
        alternative,
        span: span(node),
    })
}

fn lower_match(node: Node, source: &str, ast: &mut Ast) -> Option<Stmt> {
    let subject = node
        .child_by_field_name("subject")
        .and_then(|s| lower_expr(s, source, ast))?;
    let mut arms = Vec::new();
    let mut cursor = node.walk();
    for arm in node.named_children(&mut cursor) {
        if arm.kind() != "match_arm" {
            continue;
        }
        let Some(pattern) = arm
            .child_by_field_name("pattern")
            .and_then(|p| lower_pattern(p, source))
        else {
            continue;
        };
        let Some(body_node) = arm.child_by_field_name("body") else {
            continue;
        };
        let body = if body_node.kind() == "block" {
            lower_block(body_node, source, ast)
        } else {
            match lower_stmt(body_node, source, ast) {
                Some(s) => stmt_to_block(s),
                None => continue,
            }
        };
        arms.push(MatchArm {
            pattern,
            body,
            span: span(arm),
        });
    }
    Some(Stmt::Match {
        subject,
        arms,
        span: span(node),
    })
}

fn lower_pattern(node: Node, source: &str) -> Option<Pattern> {
    // pattern 的具名子节点：bool / qualified_name / type_pattern / variant_pattern；
    // 或裸 `Null` 关键字。
    if let Some(inner) = first_named_child(node) {
        match inner.kind() {
            "bool" => {
                return Some(Pattern::Bool {
                    value: text(inner, source) == "true",
                    span: span(node),
                });
            }
            "qualified_name" => {
                let head = field_ident(inner, "head", source)?;
                let value = field_ident(inner, "value", source)?;
                return Some(Pattern::State {
                    head,
                    value,
                    span: span(node),
                });
            }
            "type_pattern" => {
                let ty = field_ident(inner, "ty", source)?;
                let binding = field_ident(inner, "binding", source)?;
                return Some(Pattern::Type {
                    ty,
                    binding,
                    span: span(node),
                });
            }
            "variant_pattern" => {
                // 第一个 identifier 是 variant 名，其余是字段名绑定。
                let mut idents = Vec::new();
                let mut cursor = inner.walk();
                for child in inner.named_children(&mut cursor) {
                    if child.kind() == "identifier" {
                        idents.push(ident(child, source));
                    }
                }
                let mut it = idents.into_iter();
                let variant = it.next()?;
                return Some(Pattern::Variant {
                    variant,
                    fields: it.collect(),
                    span: span(node),
                });
            }
            _ => {}
        }
    }
    // 裸 `Null` 关键字 pattern。
    if text(node, source).trim() == "Null" {
        return Some(Pattern::Null { span: span(node) });
    }
    None
}

// ============ 表达式 ============

fn lower_expr(node: Node, source: &str, ast: &mut Ast) -> Option<ExprId> {
    let expr = build_expr(node, source, ast)?;
    Some(ast.alloc_expr(expr))
}

fn build_expr(node: Node, source: &str, ast: &mut Ast) -> Option<Expr> {
    match node.kind() {
        "string" => Some(Expr::Str(str_lit(node, source))),
        "int" => Some(Expr::Int {
            text: text(node, source).to_string(),
            span: span(node),
        }),
        "bool" => Some(Expr::Bool {
            value: text(node, source) == "true",
            span: span(node),
        }),
        "identifier" => Some(Expr::Ident(ident(node, source))),
        "paren_expr" => {
            let inner = first_named_child(node)?;
            build_expr(inner, source, ast)
        }
        "list_literal" => {
            let mut items = Vec::new();
            let mut cursor = node.walk();
            for c in node.named_children(&mut cursor) {
                if let Some(id) = lower_expr(c, source, ast) {
                    items.push(id);
                }
            }
            Some(Expr::List {
                items,
                span: span(node),
            })
        }
        "field_access" => {
            let base = node
                .child_by_field_name("base")
                .and_then(|b| lower_expr(b, source, ast))?;
            let field = field_ident(node, "field", source)?;
            Some(Expr::Field {
                base,
                field,
                span: span(node),
            })
        }
        "method_call" => {
            let base = node
                .child_by_field_name("base")
                .and_then(|b| lower_expr(b, source, ast))?;
            let method = field_ident(node, "method", source)?;
            let args = lower_call_args(node, source, ast);
            Some(Expr::MethodCall {
                base,
                method,
                args,
                span: span(node),
            })
        }
        "call_expr" => {
            let callee = field_ident(node, "callee", source)?;
            let args = lower_call_args(node, source, ast);
            Some(Expr::Call {
                callee,
                args,
                span: span(node),
            })
        }
        "entity_construction" => {
            let name = field_ident(node, "name", source)?;
            let mut fields = Vec::new();
            let mut cursor = node.walk();
            for fa in node.named_children(&mut cursor) {
                if fa.kind() != "field_assign" {
                    continue;
                }
                let (Some(fname), Some(value)) = (
                    field_ident(fa, "name", source),
                    fa.child_by_field_name("value")
                        .and_then(|v| lower_expr(v, source, ast)),
                ) else {
                    continue;
                };
                fields.push(FieldInit {
                    name: fname,
                    value,
                    span: span(fa),
                });
            }
            Some(Expr::Construct {
                name,
                fields,
                span: span(node),
            })
        }
        "unary_expr" => {
            // op 字段区分 `not`（布尔）与 `-`（算术取负）；operand 是其后的表达式。
            let op_text = node
                .child_by_field_name("op")
                .map(|o| text(o, source))
                .unwrap_or("not");
            let inner = node
                .child_by_field_name("op")
                .and_then(|o| o.next_named_sibling())
                .or_else(|| first_named_child(node))?;
            let operand = lower_expr(inner, source, ast)?;
            let s = span(node);
            Some(if op_text == "-" {
                Expr::Neg { operand, span: s }
            } else {
                Expr::Not { operand, span: s }
            })
        }
        "binary_expr" => {
            let left = node
                .child_by_field_name("left")
                .and_then(|l| lower_expr(l, source, ast))?;
            let right = node
                .child_by_field_name("right")
                .and_then(|r| lower_expr(r, source, ast))?;
            let op = node
                .child_by_field_name("op")
                .and_then(|o| bin_op(text(o, source)))?;
            Some(Expr::Binary {
                op,
                left,
                right,
                span: span(node),
            })
        }
        // 裸 `Null` 关键字作为表达式。
        _ if text(node, source).trim() == "Null" => Some(Expr::Null { span: span(node) }),
        _ => None,
    }
}

fn lower_call_args(node: Node, source: &str, ast: &mut Ast) -> Vec<ExprId> {
    // call_expr / method_call 的实参是除 callee/base/method 外的具名子表达式。
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        // 跳过作为字段的 method 名（identifier 字段）；实参都是表达式类节点。
        if c.kind() == "identifier" && node.child_by_field_name("method") == Some(c) {
            continue;
        }
        if c.kind() == "identifier" && node.child_by_field_name("callee") == Some(c) {
            continue;
        }
        // method_call 的 base 也是具名子节点，需跳过。
        if node.child_by_field_name("base") == Some(c) {
            continue;
        }
        if let Some(id) = lower_expr(c, source, ast) {
            out.push(id);
        }
    }
    out
}

fn bin_op(s: &str) -> Option<BinOp> {
    Some(match s {
        "or" => BinOp::Or,
        "and" => BinOp::And,
        "==" => BinOp::Eq,
        "!=" => BinOp::Ne,
        "<" => BinOp::Lt,
        "<=" => BinOp::Le,
        ">" => BinOp::Gt,
        ">=" => BinOp::Ge,
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        _ => return None,
    })
}

/// 判断节点是否直接含有给定文本的匿名 token 子节点。
fn has_anonymous_token(node: Node, token: &str) -> bool {
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        if !c.is_named() && c.kind() == token {
            return true;
        }
    }
    false
}
