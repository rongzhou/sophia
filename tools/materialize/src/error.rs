//! Materialize 层错误类型。

use thiserror::Error;

/// materialize 层结果别名。
pub type MaterializeResult<T> = Result<T, MaterializeError>;

/// materialize 层错误。
#[derive(Debug, Error)]
pub enum MaterializeError {
    /// code_check gate 未通过（DiagnosticNode kind=code_check 非 pass）。
    #[error("code_check 未通过：{0}")]
    CheckFailed(String),

    /// constraint_audit gate 未通过。
    #[error("constraint_audit 未通过：{0}")]
    AuditFailed(String),

    /// artifact_diff（strip-assist 等价）gate 未通过。
    #[error("artifact_diff 未通过：{0}")]
    ArtifactDiffFailed(String),

    /// 起步阶段：runtime input/output validation 未通过。
    #[error("runtime validation 未通过：{0}")]
    RuntimeValidationFailed(String),

    /// 原子写入失败。
    #[error("物化写入失败：{0}")]
    Write(String),
}
