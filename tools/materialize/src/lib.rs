//! Materialize gate 与原子文件写入。
//!
//! 见 docs/language_design.md 10.10、docs/language_implementation.md 第十五节。
//! Materialize 顺序用 Rust 类型状态在编译期保证：
//! `Unchecked → CheckPassed → AuditPassed → RuntimeValidated → Selected → materialize`。
//! `materialize` 只能在 `CodeCandidate<Selected>` 上调用，编译器阻止任何跳过 gate 的路径。
//!
//! materialize 原子：先写临时 staging 目录，全部成功后再 rename 替换目标文件。
//!
//! 本 crate 属 tools 层；只消费确定性检查报告（[`GateReport`]），不重复实现检查逻辑，
//! 也不依赖 workflow 图（MaterializeNode 的创建由编排层在 gate 通过后单独完成）。
//!
//! # 编译期 gate 保证
//!
//! 跳过 gate 直接物化是**编译错误**——`materialize` 只在 `CodeCandidate<Selected>` 上存在：
//!
//! ```compile_fail
//! use sophia_materialize::CodeCandidate;
//! let c = CodeCandidate::new(vec![]);
//! // Unchecked 状态没有 materialize 方法 → 编译失败。
//! c.materialize(std::path::Path::new("/tmp/x")).unwrap();
//! ```
//!
//! 正确路径必须依次通过 check → audit → runtime validation → select：
//!
//! ```
//! use sophia_materialize::{CodeCandidate, GateReport};
//! let selected = CodeCandidate::new(vec![("D/A.sophia".into(), "entity A {}".into())])
//!     .run_check(&GateReport::pass()).unwrap()
//!     .run_audit(&GateReport::pass()).unwrap()
//!     .run_runtime_validation(&GateReport::pass(), &GateReport::pass()).unwrap()
//!     .select();
//! assert_eq!(selected.file_paths().len(), 1);
//! ```

#![forbid(unsafe_code)]

mod error;
mod gate;
mod score;
mod write;

pub use error::{MaterializeError, MaterializeResult};
pub use gate::{
    AuditPassed, CheckPassed, CodeCandidate, GateReport, MaterializeOutcome, RuntimeValidated,
    Selected, Unchecked,
};
pub use score::{rank_candidates, score_candidate, Score, ScoreInputs, ScoreWeights};
pub use write::atomic_write_all;
