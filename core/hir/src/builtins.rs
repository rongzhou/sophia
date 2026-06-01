//! 内置名字表：标量类型、类型 wrapper、内置函数、特殊变量。
//!
//! 见 docs/language_design.md 第六节、docs/language_implementation.md 第十六节。
//! 这些名字不进入 ASG index（不是用户声明的节点），但名称解析需要识别它们，
//! 以区分“未解析引用”与“合法内置”。

/// 标量内置类型。
///
/// `Unit` / `Bool` / `Int` / `Text` 是 docs/language_design.md 第六节列出的标量；
/// `Uuid` / `Time` 在该文档的规范示例（TodoDomain）中作为原始类型反复使用，
/// 这里作为不透明内置标量纳入，以保证规范示例可被名称解析通过。
/// `Null` 是表达"无"的内置单值类型（见 docs/type_system.md §1.3），典型用作 `one of` 成员。
/// `Unknown` 是渐进类型顶（无参数）。
pub const SCALAR_TYPES: &[&str] = &[
    "Unit", "Bool", "Int", "Text", "Uuid", "Time", "Null", "Unknown",
];

/// Intent 包装类型（`<>` 专属，见 docs/type_system.md §1.2）。
///
/// 这 9 个是 Intent Types（语言设计 6.2 节）。结构类型（list / one of / schema）不在此——
/// 它们用 `of` 关键字族，由语法直接表达，不经 wrapper 头名解析。
pub const INTENT_WRAPPERS: &[&str] = &[
    "Raw",
    "Parsed",
    "Validated",
    "Sanitized",
    "Verified",
    "Authorized",
    "Secret",
    "Redacted",
];

/// 内置函数（可在 body 中作为 `callee` 调用，无需解析为节点）。
///
/// 当前仅 `to_text(Int)`（docs/language_implementation.md 16.5）。
pub const BUILTIN_FUNCTIONS: &[&str] = &["to_text"];

/// 内置 effect 族与操作（`Family.Op` → 参数个数）。
///
/// 见 docs/language_design.md 第十三节。这里**只保留 `Console`**——它是语言内置输出 effect 族
/// （`print` 触发），是「机制 vs 能力族」边界里的**例外**（输出原语保留为语言内置，见
/// docs/stdlib_design.md）。**标准库 / 三方库的 effect 族（`File` / `Http` / …）不在此表**——
/// 它们由库清单声明、经 [`sophia_library::LibraryRegistry`] 注入 `AsgIndex.effect_ops`，使库不
/// 渗透语言核心。用户 `effect` 顶层声明的领域 effect 也与这两类并入同一 effect 符号表。
/// 元组为 `(family, op, 参数个数)`。
pub const BUILTIN_EFFECT_OPS: &[(&str, &str, usize)] = &[("Console", "Write", 0)];

/// invariant 表达式中的隐式根变量。
pub const INVARIANT_SELF: &str = "self";

/// ensures 表达式中的隐式根变量。
pub const ENSURES_OUTPUT: &str = "output";

/// 判断名字是否为标量内置类型。
pub fn is_scalar_type(name: &str) -> bool {
    SCALAR_TYPES.contains(&name)
}

/// 判断名字是否为 Intent 包装类型（`<>` 头）。
pub fn is_intent_wrapper(name: &str) -> bool {
    INTENT_WRAPPERS.contains(&name)
}

/// 判断名字是否为内置函数。
pub fn is_builtin_function(name: &str) -> bool {
    BUILTIN_FUNCTIONS.contains(&name)
}
