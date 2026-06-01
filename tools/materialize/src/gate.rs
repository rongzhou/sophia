//! Materialize Gate 的编译期类型状态链。
//!
//! 见 docs/language_implementation.md 第十五节、docs/language_design.md 10.10。
//! Gate 顺序由 Rust 类型系统在编译期保证，而非运行时 if-else：
//!
//! ```text
//! Unchecked → CheckPassed → AuditPassed → RuntimeValidated → Selected → materialize
//! ```
//!
//! `materialize` 只能在 `CodeCandidate<Selected>` 上调用；编译器阻止任何跳过 gate
//! 的路径——例如无法对 `Unchecked` 调用 `materialize`，也无法跳过 audit 直接 select。
//!
//! 与设计文档的四状态相比，这里把「起步阶段 runtime validation 通过」（10.10）显式
//! 编码为独立状态 `RuntimeValidated`，使该门禁同样在类型层强制，而非散落的运行时判断。

use crate::error::{MaterializeError, MaterializeResult};
use std::marker::PhantomData;

// ---- gate 状态标记（零大小类型） ----

/// 未检查。
pub struct Unchecked;
/// code_check 通过。
pub struct CheckPassed;
/// constraint_audit 通过。
pub struct AuditPassed;
/// runtime input/output validation 通过（起步阶段门禁，10.10）。
pub struct RuntimeValidated;
/// 已被 SelectionNode 选中。
pub struct Selected;

/// 一份确定性检查报告：是否通过 + 失败说明。
///
/// 由对应的确定性工具（check / audit / 解释器 runtime validation）产出；
/// 本 gate 只消费其 pass/fail 结论，不重复实现检查逻辑。
#[derive(Debug, Clone)]
pub struct GateReport {
    pub passed: bool,
    pub detail: String,
}

impl GateReport {
    /// 通过的报告。
    pub fn pass() -> Self {
        GateReport {
            passed: true,
            detail: String::new(),
        }
    }

    /// 失败的报告，附说明。
    pub fn fail(detail: impl Into<String>) -> Self {
        GateReport {
            passed: false,
            detail: detail.into(),
        }
    }
}

/// 候选 CodeNode 的类型状态包装。
///
/// 类型参数 `S` 在编译期编码 gate 进度。`files` 是候选的 `.sophia` 文件
/// （路径 → 内容），随状态推进不变，最终在 materialize 时原子写入。
pub struct CodeCandidate<S> {
    files: Vec<(String, String)>,
    _state: PhantomData<S>,
}

// 手写 Debug：不要求状态标记类型 `S: Debug`（标记是零大小类型，无需展示）。
impl<S> std::fmt::Debug for CodeCandidate<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeCandidate")
            .field("state", &std::any::type_name::<S>())
            .field("file_count", &self.files.len())
            .finish()
    }
}

impl CodeCandidate<Unchecked> {
    /// 从候选文件集合构造未检查候选。
    pub fn new(files: Vec<(String, String)>) -> Self {
        CodeCandidate {
            files,
            _state: PhantomData,
        }
    }

    /// gate 1：code_check（DiagnosticNode kind=code_check 为 pass）。
    pub fn run_check(self, report: &GateReport) -> MaterializeResult<CodeCandidate<CheckPassed>> {
        if report.passed {
            Ok(self.transition())
        } else {
            Err(MaterializeError::CheckFailed(report.detail.clone()))
        }
    }
}

impl CodeCandidate<CheckPassed> {
    /// gate 2：constraint_audit（DiagnosticNode kind=constraint_audit）。
    pub fn run_audit(self, report: &GateReport) -> MaterializeResult<CodeCandidate<AuditPassed>> {
        if report.passed {
            Ok(self.transition())
        } else {
            Err(MaterializeError::AuditFailed(report.detail.clone()))
        }
    }
}

impl CodeCandidate<AuditPassed> {
    /// gate 3：strip-assist / artifact_diff 与起步阶段 runtime validation。
    ///
    /// 设计 10.10 把 artifact_diff 与 runtime validation 列为 materialize 的并列门禁；
    /// 二者都通过才进入 `RuntimeValidated`。
    pub fn run_runtime_validation(
        self,
        artifact_diff: &GateReport,
        runtime: &GateReport,
    ) -> MaterializeResult<CodeCandidate<RuntimeValidated>> {
        if !artifact_diff.passed {
            return Err(MaterializeError::ArtifactDiffFailed(
                artifact_diff.detail.clone(),
            ));
        }
        if !runtime.passed {
            return Err(MaterializeError::RuntimeValidationFailed(
                runtime.detail.clone(),
            ));
        }
        Ok(self.transition())
    }
}

impl CodeCandidate<RuntimeValidated> {
    /// gate 4：由 SelectionNode 选中。选择本身不会失败（已通过全部检查门禁）。
    pub fn select(self) -> CodeCandidate<Selected> {
        self.transition()
    }
}

impl CodeCandidate<Selected> {
    /// 物化：把候选文件原子写入 `target_root`。
    ///
    /// 仅 `Selected` 状态可调用——编译器保证物化必经全部 gate。原子语义见
    /// [`crate::write::atomic_write_all`]。
    pub fn materialize(
        self,
        target_root: &std::path::Path,
    ) -> MaterializeResult<MaterializeOutcome> {
        crate::write::atomic_write_all(target_root, &self.files)?;
        Ok(MaterializeOutcome {
            target_root: target_root.to_path_buf(),
            files: self.files.into_iter().map(|(p, _)| p).collect(),
        })
    }

    /// 候选文件路径（供物化前构造 MaterializeNode payload）。
    pub fn file_paths(&self) -> Vec<String> {
        self.files.iter().map(|(p, _)| p.clone()).collect()
    }
}

impl<S> CodeCandidate<S> {
    /// 状态推进：保留 files，仅更换类型标记（零成本）。
    fn transition<T>(self) -> CodeCandidate<T> {
        CodeCandidate {
            files: self.files,
            _state: PhantomData,
        }
    }
}

/// 物化结果：写入根目录与文件列表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializeOutcome {
    pub target_root: std::path::PathBuf,
    pub files: Vec<String>,
}
