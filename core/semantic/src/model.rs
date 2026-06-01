//! 语义声明模型（Semantic Declaration Model）。
//!
//! 见 docs/language_implementation.md 6.2：IR 节点存储**声明信息**（类型签名、
//! effect 声明、capability 要求），保持不可变；推导结果存放在独立 Table 中。
//!
//! 本模块把语法层 AST + HIR `AsgIndex` 规范化为按节点名索引的声明视图
//! （entity 字段类型、state 值集、error variant 字段、capability allow/deny、
//! storage key/value、callable 签名）。这些视图是 type/effect/contract 三层
//! 分析的**只读输入**。

use crate::effect::{Effect, EffectSet};
use crate::ty::{IntentKind, Ty};
use sophia_hir::{AsgIndex, NodeKind};
use sophia_syntax::{Ast, CallableKind, Item, TypeRef};
use std::collections::BTreeMap;

/// 把语法层 `TypeRef` 规范化为 [`Ty`]。
///
/// 依赖 `index` 区分 entity / state 名；未知具名类型按标量名尝试，
/// 再不行则记为 [`Ty::Error`]（名称解析阶段已报未解析引用，此处不重复报错）。
pub fn lower_type(ty: &TypeRef, index: &AsgIndex) -> Ty {
    match ty {
        TypeRef::Named { name, .. } => lower_named(&name.text, index),
        TypeRef::Intent { head, arg, .. } => {
            let inner = lower_type(arg, index);
            lower_intent(&head.text, inner)
        }
        TypeRef::ListOf { elem, .. } => Ty::List(Box::new(lower_type(elem, index))),
        TypeRef::SchemaOf { arg, .. } => Ty::Schema(Box::new(lower_type(arg, index))),
        TypeRef::OneOf { members, .. } => {
            Ty::OneOf(members.iter().map(|m| lower_type(m, index)).collect())
        }
    }
}

fn lower_named(name: &str, index: &AsgIndex) -> Ty {
    match name {
        "Unit" => Ty::Unit,
        "Bool" => Ty::Bool,
        "Int" => Ty::Int,
        "Text" => Ty::Text,
        "Uuid" => Ty::Uuid,
        "Time" => Ty::Time,
        "Null" => Ty::Null,
        "Unknown" => Ty::Unknown,
        _ => match index.kind_of(name) {
            Some(NodeKind::Entity) => Ty::Entity(name.to_string()),
            Some(NodeKind::State) => Ty::State(name.to_string()),
            // 作为 `one of` 成员的 error variant。
            _ if index.variant(name).is_some() => Ty::ErrorVariant(name.to_string()),
            _ => Ty::Error,
        },
    }
}

fn lower_intent(head: &str, inner: Ty) -> Ty {
    if let Some(kind) = IntentKind::from_head(head) {
        // intent 严格一层：若 inner 已是 intent，保持但仍包一层（由检查器另行约束）。
        return Ty::Intent(kind, Box::new(inner));
    }
    Ty::Error
}

/// entity 的声明视图。
#[derive(Debug, Clone)]
pub struct EntityDecl {
    pub name: String,
    /// 字段按声明顺序（构造时需全字段覆盖，顺序无关但保留便于诊断）。
    pub fields: Vec<(String, Ty)>,
}

impl EntityDecl {
    pub fn field_ty(&self, name: &str) -> Option<&Ty> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }
}

/// state 的声明视图。
#[derive(Debug, Clone)]
pub struct StateDecl {
    pub name: String,
    pub values: Vec<String>,
}

impl StateDecl {
    pub fn has_value(&self, v: &str) -> bool {
        self.values.iter().any(|x| x == v)
    }
}

/// error variant 的声明视图。
#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub name: String,
    pub fields: Vec<(String, Ty)>,
}

impl VariantDecl {
    pub fn field_ty(&self, name: &str) -> Option<&Ty> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }
}

/// capability 的声明视图。
#[derive(Debug, Clone)]
pub struct CapabilityDecl {
    pub name: String,
    pub allow: Vec<Effect>,
    pub deny: Vec<Effect>,
}

/// callable（action / transition）的声明视图。
#[derive(Debug, Clone)]
pub struct CallableDecl {
    pub name: String,
    pub kind: CallableKind,
    pub capability: Option<String>,
    pub intent_conversion: bool,
    /// input 参数（名 → 类型），按声明顺序。
    pub inputs: Vec<(String, Ty)>,
    /// output 参数（名 → 类型），按声明顺序。
    pub outputs: Vec<(String, Ty)>,
    /// 显式声明的 effects。
    pub declared_effects: EffectSet,
    /// 显式声明的 error variant 名。
    pub declared_errors: Vec<String>,
}

impl CallableDecl {
    /// 单输出 action 的输出类型（起步子集多数为单输出）。
    pub fn sole_output_ty(&self) -> Option<&Ty> {
        if self.outputs.len() == 1 {
            Some(&self.outputs[0].1)
        } else {
            None
        }
    }
}

/// 全项目语义声明模型：按名索引各类声明视图。
///
/// 使用 `BTreeMap` 保证遍历顺序确定（输出确定性）。
#[derive(Debug, Default)]
pub struct SemanticModel {
    pub entities: BTreeMap<String, EntityDecl>,
    pub states: BTreeMap<String, StateDecl>,
    /// variant 名 → variant 声明。
    pub variants: BTreeMap<String, VariantDecl>,
    pub capabilities: BTreeMap<String, CapabilityDecl>,
    pub callables: BTreeMap<String, CallableDecl>,
}

impl SemanticModel {
    /// 形式核心指纹：模型的确定性 `Debug` 表示。
    ///
    /// 用于 strip-assist 等价门禁（docs/language_design.md 5.1）：移除 Semantic Assist
    /// 字段前后，Formal Core / Semantic IR 必须完全一致。模型只承载声明信息
    /// （字段类型、签名、effect、error、capability 等），**不含 assist、不含 span**，
    /// 且全部用 `BTreeMap` 稳定排序，因此其 `Debug` 串是确定性的形式核心指纹。
    pub fn formal_fingerprint(&self) -> String {
        format!("{self:#?}")
    }

    /// 从一组 AST 与 `AsgIndex` 构建声明模型。
    ///
    /// 仅搬运声明信息，不做检查（检查由三层负责）。名称解析已在 HIR 完成，
    /// 这里对未解析类型记 [`Ty::Error`] 以避免级联误报。
    pub fn build(asts: &[&Ast], index: &AsgIndex) -> Self {
        let mut model = SemanticModel::default();
        for ast in asts {
            for item in &ast.items {
                model.add_item(item, index);
            }
        }
        model
    }

    fn add_item(&mut self, item: &Item, index: &AsgIndex) {
        match item {
            Item::Entity(e) => {
                let decl = EntityDecl {
                    name: e.name.text.clone(),
                    fields: e
                        .fields
                        .iter()
                        .map(|f| (f.name.text.clone(), lower_type(&f.ty, index)))
                        .collect(),
                };
                self.entities.insert(decl.name.clone(), decl);
            }
            Item::State(s) => {
                let decl = StateDecl {
                    name: s.name.text.clone(),
                    values: s.values.iter().map(|v| v.name.text.clone()).collect(),
                };
                self.states.insert(decl.name.clone(), decl);
            }
            Item::Error(e) => {
                for v in &e.variants {
                    let decl = VariantDecl {
                        name: v.name.text.clone(),
                        fields: v
                            .fields
                            .iter()
                            .map(|f| (f.name.text.clone(), lower_type(&f.ty, index)))
                            .collect(),
                    };
                    self.variants.insert(decl.name.clone(), decl);
                }
            }
            Item::Capability(c) => {
                let decl = CapabilityDecl {
                    name: c.name.text.clone(),
                    allow: c.allow.iter().filter_map(Effect::from_ast).collect(),
                    deny: c.deny.iter().filter_map(Effect::from_ast).collect(),
                };
                self.capabilities.insert(decl.name.clone(), decl);
            }
            Item::Action(c) | Item::Transition(c) => {
                let mut declared_effects = EffectSet::new();
                for e in &c.effects {
                    if let Some(eff) = Effect::from_ast(e) {
                        declared_effects.insert(eff);
                    }
                }
                let decl = CallableDecl {
                    name: c.name.text.clone(),
                    kind: c.kind,
                    capability: c.capability.as_ref().map(|i| i.text.clone()),
                    intent_conversion: c.intent_conversion,
                    inputs: c
                        .inputs
                        .iter()
                        .map(|p| (p.name.text.clone(), lower_type(&p.ty, index)))
                        .collect(),
                    outputs: c
                        .outputs
                        .iter()
                        .map(|p| (p.name.text.clone(), lower_type(&p.ty, index)))
                        .collect(),
                    declared_effects,
                    declared_errors: c.errors.iter().map(|i| i.text.clone()).collect(),
                };
                self.callables.insert(decl.name.clone(), decl);
            }
            // effect 声明的语义处理见 HIR 符号表；domain/task 无声明模型。
            Item::Domain(_) | Item::Task(_) | Item::Effect(_) => {}
        }
    }
}
