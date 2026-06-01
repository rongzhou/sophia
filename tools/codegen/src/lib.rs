//! Sophia → WASM codegen（工作流 A）。
//!
//! 见 docs/wasm_codegen.md（设计门，已定稿）。把 Sophia 的语义投影为可部署的 WASM artifact，
//! 使执行后端从「仅 Rust 进程内解释器」扩展到「可被 Node / Python / 浏览器 / 边缘 runtime 嵌入
//! 运行」。本 crate 属 **tools 层**：确定性、**零 IO**（emit 出的字节由 CLI 协调层落盘）、
//! **不调用 LLM**、**不修改 IR**。
//!
//! ## 首要不变量（贯穿全 crate）
//!
//! **解释器（`sophia-runtime`）是唯一语义真相源（oracle）。** 本 crate 的任何输出都必须与解释器
//! 逐 hidden case **等价**（差测试，W3 起接入）；codegen **消费** IR、**绝不反向要求改 IR / AST
//! 形状**（docs/language_implementation.md §12.1）。引入第二条语义真相源是被禁止的。
//!
//! ## 阶段进度（见 docs/wasm_codegen.md §九）
//!
//! - **W1（A1，已落地）**：冻结输入契约（[`CodegenInput`]）+ crate 骨架。
//! - **W2（A2，本阶段）**：最小 emit——值 `Unit`/`Bool`/`Int`/`Null` 的值 ABI + 函数 ABI（Outcome
//!   包装 + raise 冒泡）+ 标量算术 / 比较 / 布尔 / 一元 / `if`-`else` / `let`-`set` / `return` / 跨
//!   callable 调用。未覆盖构造诚实返回 [`CodegenError::NotYetImplemented`]（不伪造产出）。

#![forbid(unsafe_code)]

mod abi;
mod build;
mod contract;
mod emit;
mod error;

pub use build::{check_artifact_strip_equivalence, emit_from_sources, ArtifactDiffOutcome};
pub use contract::CodegenInput;
pub use error::{CodegenError, CodegenResult};

/// 把一个已通过语义检查的 Sophia 程序 emit 为 WASM 模块字节（`.wasm`）。
///
/// **W2 覆盖**：值 `Unit` / `Bool` / `Int` / `Null`；标量算术 / 比较 / 布尔 / 一元 / `if`-`else` /
/// `let`-`set` / `return` / 跨 callable 调用（Outcome 包装 + raise 冒泡 ABI）。**未覆盖构造**
/// （`match` / `repeat` / `raise` / `print` / Text / List / Entity / State / effect）诚实返回
/// [`CodegenError::NotYetImplemented`]——绝不伪造产出（见 docs/wasm_codegen.md §九，待后续增量）。
///
/// 前置：`input` 必须来自**已通过 `sophia-check`**（名称解析 + 语义三层 + strip-assist）的程序；
/// codegen 不重复检查（与解释器一致：执行前程序已 check 通过）。emit 的字节确定（段顺序 / 名字
/// 字典序 / 布局固定），服务 strip-assist artifact 层比对（W5）。
pub fn emit_module(input: &CodegenInput<'_>) -> CodegenResult<Vec<u8>> {
    emit::emit(input)
}
