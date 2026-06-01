//! codegen 错误类型。
//!
//! 见 docs/wasm_codegen.md。codegen 是确定性工具层，错误以结构化 `Result` 返回，渲染交上层。

use thiserror::Error;

/// codegen 结果别名。
pub type CodegenResult<T> = Result<T, CodegenError>;

/// codegen 过程中的错误。
#[derive(Debug, Error)]
pub enum CodegenError {
    /// 输入契约不满足（如入口节点不在 Execution Graph 中、模型与图不一致）。
    /// 正常情况下不应发生——输入应来自已通过 `sophia-check` 的程序。
    #[error("codegen 输入契约不满足：{0}")]
    InvalidInput(String),

    /// 某构造的 emit 尚未实现（W1 占位 / 后续阶段按需细化）。**诚实标注"待接入"，绝不伪造产出。**
    #[error("WASM emit 尚未实现：{0}")]
    NotYetImplemented(String),
}
