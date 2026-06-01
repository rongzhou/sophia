//! Effect 表示与代数（algebraic effects）。
//!
//! 见 docs/language_design.md 6.3 / 第十三节、docs/language_implementation.md 7.3。
//! effect 规范化为 `(family, op, args)` 三元组：`Console.Write` 即 `(Console, Write, [])`，
//! `DB.Read("S")` 即 `(DB, Read, ["S"])`。这取代了原先 4 类硬编码变体，使 effect 族可由
//! `effect` 顶层声明扩展（内置 Console/DB/Llm/Tool/Stream 由 hir 预声明）。
//!
//! 相等 / 子集语义不变：family + op + 全部 args 都相等才算同一 effect。

use std::fmt;

/// effect 实参的规范化表示：区分字面量与绑定名。
///
/// 字面量（如 storage 名 `"Todos"`）在静态层可比较，参与精确相等
/// （`DB.Read("A") ≠ DB.Read("B")`）；绑定名（如来自 input 的 `model`）的运行时值
/// 静态未知，在 capability 匹配中视为通配（capability 授予该操作而非具体实参）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EffectArg {
    /// 字面量参数（规范化文本）。
    Lit(String),
    /// 绑定名参数（运行时值静态未知）。
    Binding(String),
}

/// 规范化 effect：`(family, op, args)` 三元组。
///
/// 注意：`Pure` 表示"无副作用"，与其他 effect 互斥；它不出现在 effect 集合中，
/// 而是用空集合表达（见 [`EffectSet`]）。`Effect` 只表示具体 effect 操作。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Effect {
    pub family: String,
    pub op: String,
    pub args: Vec<EffectArg>,
}

impl Effect {
    /// 构造一个 effect 操作。
    pub fn new(family: impl Into<String>, op: impl Into<String>, args: Vec<EffectArg>) -> Self {
        Effect {
            family: family.into(),
            op: op.into(),
            args,
        }
    }

    /// 由语法层 `Effect` 转换为规范化 effect；`Pure` 返回 `None`（由空集表达）。
    pub fn from_ast(e: &sophia_syntax::Effect) -> Option<Self> {
        match e {
            sophia_syntax::Effect::Pure => None,
            sophia_syntax::Effect::Op {
                family, op, args, ..
            } => Some(Effect::new(
                family.text.clone(),
                op.text.clone(),
                args.iter().map(effect_arg_of).collect(),
            )),
        }
    }

    /// 该 effect 是否被 capability 条目 `entry` 涵盖。
    ///
    /// family + op 必须相同；实参逐位置比较——两侧都是字面量则须相等，任一侧为绑定名
    /// 则视为通配（运行时值静态未知，capability 授予该操作）。参数个数须一致。
    pub fn covered_by(&self, entry: &Effect) -> bool {
        if self.family != entry.family || self.op != entry.op {
            return false;
        }
        if self.args.len() != entry.args.len() {
            return false;
        }
        self.args
            .iter()
            .zip(&entry.args)
            .all(|(used, allowed)| match (used, allowed) {
                (EffectArg::Lit(a), EffectArg::Lit(b)) => a == b,
                // 任一侧为绑定名 → 通配。
                _ => true,
            })
    }
}

/// 把语法层 effect 实参规范化（区分字面量 / 绑定名）。
fn effect_arg_of(a: &sophia_syntax::EffectArg) -> EffectArg {
    use sophia_syntax::EffectArg as A;
    match a {
        A::Str(s) => EffectArg::Lit(s.value.clone()),
        A::Int { text, .. } => EffectArg::Lit(text.clone()),
        A::Bool { value, .. } => EffectArg::Lit(value.to_string()),
        A::Ident(id) => EffectArg::Binding(id.text.clone()),
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.args.is_empty() {
            write!(f, "{}.{}", self.family, self.op)
        } else {
            let args = self
                .args
                .iter()
                .map(|a| match a {
                    EffectArg::Lit(s) => format!("\"{s}\""),
                    EffectArg::Binding(s) => s.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, "{}.{}({args})", self.family, self.op)
        }
    }
}

/// observable effect 集合（不含 `Pure`；空集即纯）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectSet {
    effects: Vec<Effect>,
}

impl EffectSet {
    pub fn new() -> Self {
        EffectSet::default()
    }

    /// 加入一个 effect（去重）。
    pub fn insert(&mut self, e: Effect) {
        if !self.effects.contains(&e) {
            self.effects.push(e);
        }
    }

    /// 是否为纯（空集）。
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }

    /// 是否包含某 effect。
    pub fn contains(&self, e: &Effect) -> bool {
        self.effects.contains(e)
    }

    /// 遍历。
    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.effects.iter()
    }
}
