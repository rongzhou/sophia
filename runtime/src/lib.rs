//! Sophia Runtime。
//!
//! 见 docs/language_implementation.md 第九节。v0 唯一执行后端是 Rust 进程内
//! 解释器。职责：execution graph 执行、effect tracking、runtime input/output
//! validation、execution debugging。
//!
//! runtime input/output validation 直接消费 entity / state / error metadata
//! （Semantic 声明模型），不经过任何中间语言（step 5）。
//!
//! 异步边界（9.3）：Sophia 当前语言 / runtime 语义无 await / 并发 / 共享可变状态，
//! 解释核心是同步纯逻辑；IO 副作用经 [`HostRegistry`] 以同步 host 调用委派给宿主。
//! Rust async 只属于 LLM / LSP 等工具链 IO 外壳，不代表 runtime 有异步执行目标。

#![forbid(unsafe_code)]

mod error;
mod host;
mod interp;
mod trace;
mod validate;
mod value;
mod value_wire;
mod verify;
mod wasm_host;
mod wasm_program;

pub use error::{RuntimeError, RuntimeResult};
pub use host::{HostFn, HostRegistry};
pub use interp::{Interpreter, Outcome};
pub use trace::{ExecutionSpan, SpanOutcome, Trace};
pub use value::{RaisedError, Value};
pub use verify::{
    run_hidden_case, run_hidden_cases, ExpectedOutcome, HiddenCase, VerificationResult,
};
pub use wasm_host::WasmHostFn;
pub use wasm_program::WasmProgramRunner;

use sophia_syntax::Ast;

/// 用调用方提供的 effect 宿主注册表执行某 action / transition。
///
/// 调用方必须显式提供 host：纯逻辑 / Console 程序传空 [`HostRegistry`]，需要库 effect 的程序先注册
/// 标准库 native / mock 或三方 WASM host（见 docs/stdlib_design.md）。返回执行结局 + Trace 投影；
/// 宿主由调用方持有（其内部状态如 console / 文件桶 / 网络响应均在调用方手中）。
pub fn run_action(
    model: &sophia_semantic::SemanticModel,
    asts: &[&Ast],
    name: &str,
    args: Vec<Value>,
    host: &mut HostRegistry,
) -> RuntimeResult<(Outcome, Trace)> {
    let mut interp = Interpreter::new(model, asts, host);
    let outcome = interp.run(name, args)?;
    let trace = interp.into_trace();
    Ok((outcome, trace))
}
