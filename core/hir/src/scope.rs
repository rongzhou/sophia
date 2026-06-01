//! 作用域分析（body scope）。
//!
//! 见 docs/language_design.md 第七节“作用域”：
//! - action input 是 body 根作用域变量；
//! - `let` / `let mutable` 声明 block-scoped local；`if` / `repeat` / match-arm 创建子作用域；
//! - 子作用域可读取外层变量，可 `set` 外层 mutable 变量；
//! - block 内声明的变量不泄漏到 block 外；
//! - `match` 类型 pattern（`Int x` / `Todo t`）/ variant pattern（`V { f }`）绑定的 name 仅在该 case body 内可见；
//! - **禁止 shadow 可见变量名**（包括外层变量），避免 LLM repair 中语义漂移。

use std::collections::HashMap;

/// 一个变量绑定。
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    /// 是否可变（`let mutable` 或 input 视情况；input 在起步子集中视为不可变）。
    pub mutable: bool,
}

/// 词法作用域栈。
///
/// 每个 frame 是一层 block 的局部变量；查找沿栈向外；声明时检查**全部可见层**
/// 以禁止 shadowing（不仅是当前层）。
#[derive(Debug, Default)]
pub struct ScopeStack {
    frames: Vec<HashMap<String, Binding>>,
}

impl ScopeStack {
    /// 新建并压入根作用域。
    pub fn new() -> Self {
        ScopeStack {
            frames: vec![HashMap::new()],
        }
    }

    /// 压入一个子作用域。
    pub fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    /// 弹出当前子作用域（其局部变量随之失效）。
    pub fn pop(&mut self) {
        debug_assert!(self.frames.len() > 1, "不应弹出根作用域");
        self.frames.pop();
    }

    /// 某名字在任意可见层是否已绑定（用于 shadowing 检查）。
    pub fn is_visible(&self, name: &str) -> bool {
        self.frames.iter().any(|f| f.contains_key(name))
    }

    /// 查找名字的绑定（从内层到外层）。
    pub fn lookup(&self, name: &str) -> Option<Binding> {
        self.frames.iter().rev().find_map(|f| f.get(name).copied())
    }

    /// 在当前层声明一个变量。调用方应先用 [`Self::is_visible`] 做 shadowing 检查。
    pub fn declare(&mut self, name: impl Into<String>, mutable: bool) {
        self.frames
            .last_mut()
            .expect("作用域栈至少有根层")
            .insert(name.into(), Binding { mutable });
    }
}
