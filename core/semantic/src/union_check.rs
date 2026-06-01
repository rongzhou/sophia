//! `one of` 联合类型的可区分性检查（设计 `docs/type_system.md` §2.2 / §七 / §九.5）。
//!
//! 规则：`one of { ... }` 的成员必须**两两可由 match tag 区分**，否则 match 分派有歧义
//! （运行时只能首个匹配胜出，破坏确定性）。tag 判定：
//! - 标量按类型名（`Int` / `Bool` / `Text` / `Unit` / `Uuid` / `Time`）；
//! - `Null` 是唯一字面；
//! - entity / state 按名；error variant 按 variant 名；
//! - **Intent 在运行时被擦除**（7.2），故 `Raw<Text>` / `Sanitized<Text>` / `Text` 的 tag 同为
//!   底层标量 —— 互相**不可区分**；
//! - 嵌套 `one of` 按联合语义展开其成员 tag。
//!
//! `Unknown` / `Error`（渐进 / 未解析恢复）不产生 tag，跳过（避免对未解析引用级联误报）。

use crate::error::{SemanticDiagnostic, SemanticDiagnosticKind as K};
use crate::model::lower_type;
use crate::ty::Ty;
use sophia_hir::AsgIndex;
use sophia_syntax::{Ast, Item, Span, TypeRef};
use std::collections::HashMap;

/// 对整个程序的全部类型位置做 `one of` 可区分性检查，诊断写入 `diags`。
pub fn check_unions(asts: &[&Ast], index: &AsgIndex, diags: &mut Vec<SemanticDiagnostic>) {
    for ast in asts {
        for item in &ast.items {
            visit_item(item, index, diags);
        }
    }
}

fn visit_item(item: &Item, index: &AsgIndex, diags: &mut Vec<SemanticDiagnostic>) {
    match item {
        Item::Entity(e) => {
            for f in &e.fields {
                visit_type(&f.ty, index, diags);
            }
        }
        Item::Error(e) => {
            for v in &e.variants {
                for f in &v.fields {
                    visit_type(&f.ty, index, diags);
                }
            }
        }
        Item::Transition(c) | Item::Action(c) => {
            for p in c.inputs.iter().chain(c.outputs.iter()) {
                visit_type(&p.ty, index, diags);
            }
        }
        Item::Effect(e) => {
            for op in &e.operations {
                for p in &op.params {
                    visit_type(&p.ty, index, diags);
                }
            }
        }
        Item::Domain(_) | Item::State(_) | Item::Capability(_) | Item::Task(_) => {}
    }
}

/// 递归访问一个类型引用；遇 `one of` 即检查其成员可区分性，并递归子类型。
fn visit_type(ty: &TypeRef, index: &AsgIndex, diags: &mut Vec<SemanticDiagnostic>) {
    match ty {
        TypeRef::Named { .. } => {}
        TypeRef::Intent { arg, .. } => visit_type(arg, index, diags),
        TypeRef::ListOf { elem, .. } => visit_type(elem, index, diags),
        TypeRef::SchemaOf { arg, .. } => visit_type(arg, index, diags),
        TypeRef::OneOf { members, span } => {
            check_one_of(members, *span, index, diags);
            for m in members {
                visit_type(m, index, diags);
            }
        }
    }
}

/// 检查单个 `one of { ... }` 的成员两两可区分（按 match tag）。
fn check_one_of(
    members: &[TypeRef],
    span: Span,
    index: &AsgIndex,
    diags: &mut Vec<SemanticDiagnostic>,
) {
    // tag → 首次出现的成员显示名，用于诊断信息。
    let mut seen: HashMap<String, String> = HashMap::new();
    for m in members {
        let mty = lower_type(m, index);
        for tag in member_tags(&mty) {
            let display = mty.to_string();
            if let Some(prev) = seen.get(&tag) {
                diags.push(SemanticDiagnostic::new(
                    K::IndistinguishableUnion,
                    span,
                    format!(
                        "`one of` 成员 `{display}` 与 `{prev}` 按 match tag 不可区分\
                         （tag `{tag}`）；intent 在运行时被擦除，底层类型相同的成员无法 match 分派"
                    ),
                ));
            } else {
                seen.insert(tag, display);
            }
        }
    }
}

/// 一个成员类型在 match 时的 tag 集合（嵌套 `one of` 展开为多个；渐进 / 恢复返回空）。
fn member_tags(ty: &Ty) -> Vec<String> {
    match ty {
        // Intent 运行时擦除：tag 取 inner。
        Ty::Intent(_, inner) => member_tags(inner),
        // 嵌套联合：按联合语义展开全部成员 tag。
        Ty::OneOf(members) => members.iter().flat_map(member_tags).collect(),
        Ty::Unit => vec!["Unit".into()],
        Ty::Bool => vec!["Bool".into()],
        Ty::Int => vec!["Int".into()],
        Ty::Text => vec!["Text".into()],
        Ty::Uuid => vec!["Uuid".into()],
        Ty::Time => vec!["Time".into()],
        Ty::Null => vec!["Null".into()],
        Ty::Entity(n) | Ty::State(n) | Ty::ErrorVariant(n) => vec![n.clone()],
        // list / schema 无对应类型 pattern；以单一 tag 表示（同类成员互相不可区分）。
        Ty::List(_) => vec!["<list>".into()],
        Ty::Schema(_) => vec!["<schema>".into()],
        // 渐进 / 恢复 / 记录：不产生 tag（跳过，避免级联误报）。
        Ty::Unknown | Ty::Error | Ty::Record(_) => vec![],
    }
}
