//! Runtime 层错误类型。
//!
//! 区分两类：
//! - [`RuntimeError`]：解释器无法继续的硬错误（校验失败、未知 action 等）；
//! - 领域错误（`raise`）不是 `RuntimeError`，而是正常执行结果 [`crate::Outcome::Raised`]。
//!   仅当领域错误跨调用边界冒泡且无处承接时，才以 [`RuntimeError::Raised`] 表达。

use crate::value::RaisedError;
use thiserror::Error;

/// runtime 层结果别名。
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// runtime 层硬错误。
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// runtime input/output validation 失败，或解释期结构性错误。
    #[error("runtime 校验失败：{0}")]
    Validation(String),

    /// 被调用方 raise 的领域错误冒泡到调用点（起步子集无 error handle，
    /// 直接向上传播由顶层呈现）。
    #[error("领域错误：{0}")]
    Raised(RaisedError),
}
