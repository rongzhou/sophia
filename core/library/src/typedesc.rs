//! 受限中立类型描述符（mini-DSL，见 docs/stdlib_design.md）。
//!
//! 库的 op 签名只能用这个**受限词汇**描述——刻意只覆盖现有库已用到的形状，不开放任意类型。
//! 这是「库不渗透语言核心」的关键约束之一：库**引用**核心标量 / intent 词汇，**不能定义**新种类。
//!
//! ```text
//! TypeDesc := Scalar                  # Int | Bool | Text | Unit
//!           | Intent "<" Scalar ">"   # Raw<Text> | Sanitized<Text> | ...（intent 名取自核心固定集）
//! ```
//!
//! 故意**不支持**：库自定义 entity/state/error 作参/返、泛型、`one of`、`list of`。将来某库需要
//! 更复杂签名时，扩此 DSL 并过设计门（YAGNI，不预先开放）。

use serde::{Deserialize, Serialize};

/// 语言核心标量（库可引用）。与 `hir::builtins::SCALAR_TYPES` 的可作签名子集对齐。
/// 这里只放**库签名可用**的标量（Int/Bool/Text/Unit）；Uuid/Time/Null/Unknown 不开放给库签名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scalar {
    Int,
    Bool,
    Text,
    Unit,
}

impl Scalar {
    fn parse(s: &str) -> Option<Scalar> {
        match s {
            "Int" => Some(Scalar::Int),
            "Bool" => Some(Scalar::Bool),
            "Text" => Some(Scalar::Text),
            "Unit" => Some(Scalar::Unit),
            _ => None,
        }
    }

    /// 标量名（与语言核心类型名一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Scalar::Int => "Int",
            Scalar::Bool => "Bool",
            Scalar::Text => "Text",
            Scalar::Unit => "Unit",
        }
    }
}

/// 受限类型描述符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeDesc {
    /// 裸标量。
    Scalar(Scalar),
    /// intent 包装的标量（intent 名 + inner 标量）。intent 名校验交由消费方（语义层比对核心
    /// `INTENT_WRAPPERS`）——本层只解析形状，保持 core/library 不依赖 hir。
    Intent { intent: String, inner: Scalar },
}

impl TypeDesc {
    /// 从描述符字符串解析（`"Text"` / `"Raw<Text>"`）。
    ///
    /// 解析只校验**形状**（标量名合法、`Intent<Scalar>` 结构正确）；intent 名是否属核心固定集
    /// 由语义层在比对时判定（本层不依赖 hir 的 `INTENT_WRAPPERS`，避免反向依赖）。
    pub fn parse(s: &str) -> Result<TypeDesc, String> {
        let s = s.trim();
        if let Some(open) = s.find('<') {
            if !s.ends_with('>') {
                return Err(format!("`{s}`：intent 包装须以 `>` 结尾"));
            }
            let intent = s[..open].trim();
            let inner = s[open + 1..s.len() - 1].trim();
            if intent.is_empty() {
                return Err(format!("`{s}`：缺 intent 名"));
            }
            let scalar = Scalar::parse(inner)
                .ok_or_else(|| format!("`{s}`：inner `{inner}` 不是受支持标量"))?;
            Ok(TypeDesc::Intent {
                intent: intent.to_string(),
                inner: scalar,
            })
        } else {
            let scalar =
                Scalar::parse(s).ok_or_else(|| format!("`{s}` 不是受支持标量或 Intent<Scalar>"))?;
            Ok(TypeDesc::Scalar(scalar))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalar() {
        assert_eq!(
            TypeDesc::parse("Text").unwrap(),
            TypeDesc::Scalar(Scalar::Text)
        );
        assert_eq!(
            TypeDesc::parse("Unit").unwrap(),
            TypeDesc::Scalar(Scalar::Unit)
        );
    }

    #[test]
    fn parses_intent() {
        assert_eq!(
            TypeDesc::parse("Raw<Text>").unwrap(),
            TypeDesc::Intent {
                intent: "Raw".into(),
                inner: Scalar::Text
            }
        );
        assert_eq!(
            TypeDesc::parse("Sanitized<Text>").unwrap(),
            TypeDesc::Intent {
                intent: "Sanitized".into(),
                inner: Scalar::Text
            }
        );
    }

    #[test]
    fn rejects_unknown_scalar_and_malformed() {
        assert!(TypeDesc::parse("Widget").is_err());
        assert!(TypeDesc::parse("Raw<Widget>").is_err());
        assert!(TypeDesc::parse("Raw<Text").is_err());
        assert!(TypeDesc::parse("<Text>").is_err());
    }
}
