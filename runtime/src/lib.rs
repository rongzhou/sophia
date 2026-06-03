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
mod verify;
mod wasm_host;

pub use error::{RuntimeError, RuntimeResult};
pub use host::{HostFn, HostRegistry};
pub use interp::{Interpreter, Outcome};
pub use trace::{ExecutionSpan, SpanOutcome, Trace};
pub use value::{RaisedError, Value};
pub use verify::{
    run_hidden_case, run_hidden_cases, ExpectedOutcome, HiddenCase, VerificationResult,
};
pub use wasm_host::WasmHostFn;

use sophia_syntax::Ast;

/// 一次顶层执行的完整结果：执行结局 + effect 宿主 + 执行图 Trace 投影。
///
/// `trace` 是 Execution Graph 执行的投影（见 docs/language_implementation.md 9.4）：
/// 每次 callable 进入一条 span，携带其 `ExecNodeId` 与触发它的调用边 `ExecEdgeId`。
#[derive(Debug)]
pub struct Execution {
    /// 执行结局（正常返回 / 领域错误）。
    pub outcome: Outcome,
    /// effect 宿主注册表（含捕获的 console 输出 + 库 host）。
    pub host: HostRegistry,
    /// Execution Graph 执行的 Trace 投影（9.4）。
    pub trace: Trace,
}

/// 执行某 action / transition 的便捷入口（**空 host 注册表**：仅支持 Console / 纯逻辑）。
///
/// 用到库 effect（`File` / `Http` / 三方）的程序须用 [`run_action_with_host`] 注入注册了对应库
/// host 的 [`HostRegistry`]（标准库见 `sophia-stdlib`）。返回 [`Execution`]（结局 + 宿主 + Trace）。
pub fn run_action(
    model: &sophia_semantic::SemanticModel,
    asts: &[&Ast],
    name: &str,
    args: Vec<Value>,
) -> RuntimeResult<Execution> {
    let mut host = HostRegistry::new();
    let (outcome, trace) = run_action_with_host(model, asts, name, args, &mut host)?;
    Ok(Execution {
        outcome,
        host,
        trace,
    })
}

/// 用调用方提供的 effect 宿主注册表执行某 action / transition。
///
/// 供需要**注入库 host**（标准库 native / mock、三方 WASM，见 docs/stdlib_design.md）的协调层
/// 使用——`runtime` 自身不内置任何具体库。返回执行结局 + Trace 投影；宿主由调用方持有（其内部
/// 状态如 console / 文件桶 / 网络响应均在调用方手中）。
pub fn run_action_with_host(
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
