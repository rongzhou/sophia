//! Runtime input/output validation。
//!
//! 见 docs/language_implementation.md 16.1–16.3、构建顺序 step 5：解释器在 action
//! 边界用 entity / state / error metadata 校验值，**直接消费 Semantic 元信息，
//! 不经过中间语言**。
//!
//! 校验是结构性的：值的形状必须与声明类型一致。Intent 是编译期静态属性，
//! 运行时值不携带 intent（见 value.rs），因此 intent wrapper 在校验时被剥离，
//! 只校验 inner 结构。

use crate::value::Value;
use sophia_semantic::{SemanticModel, Ty};

/// 校验值 `v` 是否符合类型 `ty`。返回 `Err(描述)` 说明不符之处。
pub fn check_value(v: &Value, ty: &Ty, model: &SemanticModel) -> Result<(), String> {
    match ty {
        // 渐进 / 恢复类型：运行时退化为动态，跳过结构校验。
        Ty::Unknown | Ty::Error => Ok(()),
        // schema of T：起步阶段按 inner 结构校验。
        Ty::Schema(inner) => check_value(v, inner, model),
        // Intent 是静态属性，剥离后校验 inner。
        Ty::Intent(_, inner) => check_value(v, inner, model),

        Ty::Unit => expect(matches!(v, Value::Unit), "Unit", v),
        Ty::Bool => expect(matches!(v, Value::Bool(_)), "Bool", v),
        Ty::Int => expect(matches!(v, Value::Int(_)), "Int", v),
        // Text / Uuid / Time 在运行时都以文本承载。
        Ty::Text | Ty::Uuid | Ty::Time => expect(matches!(v, Value::Text(_)), "Text", v),

        Ty::Null => expect(matches!(v, Value::Null), "Null", v),

        Ty::List(elem) => match v {
            Value::List(items) => {
                for it in items {
                    check_value(it, elem, model)?;
                }
                Ok(())
            }
            _ => type_err("List", v),
        },
        // `one of { M, ... }`：值须匹配某成员类型。
        Ty::OneOf(members) => {
            if members.iter().any(|m| check_value(v, m, model).is_ok()) {
                Ok(())
            } else {
                type_err("one of {...}", v)
            }
        }
        // error variant 成员（被返回）：值须是同名 ErrorValue，且字段结构一致。
        Ty::ErrorVariant(name) => match v {
            Value::ErrorValue { variant, fields } if variant == name => {
                let decl = model
                    .variants
                    .get(name)
                    .ok_or_else(|| format!("未知 error variant `{name}`"))?;
                for (fname, fty) in &decl.fields {
                    let fv = fields
                        .get(fname)
                        .ok_or_else(|| format!("variant `{name}` 缺字段 `{fname}`"))?;
                    check_value(fv, fty, model)?;
                }
                Ok(())
            }
            _ => type_err(&format!("error variant `{name}`"), v),
        },
        Ty::Entity(name) => match v {
            Value::Entity { name: vn, fields } => {
                if vn != name {
                    return Err(format!("期望 entity `{name}`，实际 entity `{vn}`"));
                }
                let decl = model
                    .entities
                    .get(name)
                    .ok_or_else(|| format!("未知 entity `{name}`"))?;
                // 字段集合必须完全匹配（构造时全字段覆盖由编译期保证；运行时复核）。
                for (fname, fty) in &decl.fields {
                    let fv = fields
                        .get(fname)
                        .ok_or_else(|| format!("entity `{name}` 缺字段 `{fname}`"))?;
                    check_value(fv, fty, model)?;
                }
                for fname in fields.keys() {
                    if decl.field_ty(fname).is_none() {
                        return Err(format!("entity `{name}` 含未知字段 `{fname}`"));
                    }
                }
                Ok(())
            }
            _ => type_err(&format!("entity `{name}`"), v),
        },
        Ty::State(name) => match v {
            Value::State { state, value } => {
                if state != name {
                    return Err(format!("期望 state `{name}`，实际 `{state}`"));
                }
                let decl = model
                    .states
                    .get(name)
                    .ok_or_else(|| format!("未知 state `{name}`"))?;
                if decl.has_value(value) {
                    Ok(())
                } else {
                    Err(format!("state `{name}` 无值 `{value}`"))
                }
            }
            _ => type_err(&format!("state `{name}`"), v),
        },
        // Record 仅用于 ensures 谓词作用域，不作为运行时边界类型出现。
        Ty::Record(_) => Ok(()),
    }
}

fn expect(ok: bool, expected: &str, v: &Value) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        type_err(expected, v)
    }
}

fn type_err(expected: &str, v: &Value) -> Result<(), String> {
    Err(format!("期望 {expected}，实际值 {v}"))
}
