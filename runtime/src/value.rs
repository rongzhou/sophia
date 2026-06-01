//! 运行时值模型。
//!
//! 见 docs/language_implementation.md 9.2、第十六节起步子集。解释器直接消费
//! Semantic 元信息执行，不经过中间语言。值与起步子集类型一一对应：
//! - 标量 Unit / Bool / Int / Text（含不透明 Uuid / Time 以文本承载）/ Null；
//! - 列表 List；`one of` 成员就是成员自身的值（无包装变体），被返回的 error variant 用 ErrorValue；
//! - entity 记录（字段名 → 值，标记 entity 名）；
//! - state value（标记联合：state 名 + value 名）。
//!
//! Intent 是类型层的静态属性，运行时不携带 intent 标签（intent 检查在编译期完成，
//! 见 7.2）；运行时值只保留结构。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// 运行时值。
///
/// 可序列化（serde）：除供解释器使用外，也作为 hidden-case 隐藏存储
/// （`sophia-runs/verifiers/hidden.json`）的值表示——单一值模型，不另设镜像类型。
/// 序列化为按 variant 标签的判别联合（外部标签 = variant 名）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Text(String),
    /// 列表（元素同构由编译期保证）。
    List(Vec<Value>),
    /// `Null`：表达"无"的单值（`one of` 的 Null 成员值）。
    Null,
    /// 作为 `one of` 成员被**返回**的 error variant 值（variant tag + 字段）。
    /// 形状与 [`RaisedError`] 同构，但语义是可恢复返回值，区别于 `raise` 的控制流中断。
    ErrorValue {
        variant: String,
        fields: BTreeMap<String, Value>,
    },
    /// entity 记录：entity 名 + 字段值（按字段名稳定排序）。
    Entity {
        name: String,
        fields: BTreeMap<String, Value>,
    },
    /// state value：state 名 + value 名（标记联合）。
    State {
        state: String,
        value: String,
    },
}

impl Value {
    /// 取整数值。
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// 取布尔值。
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => write!(f, "()"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Text(s) => write!(f, "{s}"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Null => write!(f, "Null"),
            Value::ErrorValue { variant, fields } => {
                write!(f, "{variant}")?;
                if !fields.is_empty() {
                    write!(f, " {{ ")?;
                    for (i, (k, v)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{k} = {v}")?;
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            }
            Value::Entity { name, fields } => {
                write!(f, "{name} {{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} = {v}")?;
                }
                write!(f, " }}")
            }
            Value::State { state, value } => write!(f, "{state}.{value}"),
        }
    }
}

/// 一个被 raise 的领域错误（variant tag + 字段值）。
#[derive(Debug, Clone, PartialEq)]
pub struct RaisedError {
    pub variant: String,
    pub fields: BTreeMap<String, Value>,
}

impl fmt::Display for RaisedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.variant)?;
        if !self.fields.is_empty() {
            write!(f, " {{ ")?;
            for (i, (k, v)) in self.fields.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{k} = {v}")?;
            }
            write!(f, " }}")?;
        }
        Ok(())
    }
}
