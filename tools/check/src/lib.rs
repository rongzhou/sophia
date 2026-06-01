//! 确定性检查器（checker + diagnostics）。
//!
//! 见 docs/language_implementation.md 7.6 最小可实现检查集、docs/engineering_architecture.md
//! 第九节、docs/language_design.md 5.1。组装已完成的核心层为可调用的 `Checker`：
//! - 名称解析（HIR）；
//! - 语义三层（type / effect / contract）；
//! - **strip-assist 等价门禁**：移除全部 Semantic Assist 字段后，Semantic IR 必须不变。
//!
//! 同步、无 IO（IO 由 CLI 协调层承担）。诊断结构化返回，渲染交由上层。

#![forbid(unsafe_code)]

mod strip_assist;

pub use strip_assist::{check_strip_assist_equivalence, StripAssistOutcome};

use sophia_hir::{
    resolve_program_with_libraries, AsgIndex, HirDiagnostic, LibrarySources, ProgramInput,
};
use sophia_semantic::{analyze_program, SemanticDiagnostic, SemanticModel};
use sophia_syntax::Ast;
use thiserror::Error;

/// check 层结果别名。
pub type CheckResult<T> = Result<T, CheckError>;

/// check 层硬错误（无法继续）。
#[derive(Debug, Error)]
pub enum CheckError {
    /// ASG index 构建失败（重名 / 一文件多节点等）。
    #[error("ASG index 构建失败：{0}")]
    IndexBuild(String),
}

/// 一次 check 的完整报告。
pub struct CheckReport {
    /// HIR 名称解析诊断。
    pub hir: Vec<HirDiagnostic>,
    /// 语义三层诊断。
    pub semantic: Vec<SemanticDiagnostic>,
    /// strip-assist 等价门禁结果。
    pub strip_assist: StripAssistOutcome,
}

impl CheckReport {
    /// 是否完全通过（无诊断且 strip-assist 等价）。
    pub fn passed(&self) -> bool {
        self.hir.is_empty() && self.semantic.is_empty() && self.strip_assist.equivalent
    }
}

/// 对一组源文件运行全套确定性检查。
///
/// `sources` 是 `(domain, path, source)` 列表（IO 由调用方完成）。本函数自行解析、
/// 构建 index 与模型，并跑 strip-assist 等价门禁（需重解析 stripped 版本）。
///
/// 前置：调用方应已过滤语法错误（语法错误时各核心层行为未定义）。
pub fn check_program(sources: &[(String, String, String)]) -> CheckResult<CheckReport> {
    // 解析原始 AST。
    let asts: Vec<Ast> = sources
        .iter()
        .map(|(_, _, src)| {
            sophia_syntax::parse_str(src.as_str())
                .expect("parse")
                .to_ast()
        })
        .collect();

    let registry = sophia_stdlib::standard_registry();

    // 库随附 Sophia 源码（标准库当前无，三方纯 Sophia 库有）解析为 owned AST，与用户源码同列
    // 进 index / model（纯 Sophia 库节点须可解析；见 docs/stdlib_design.md §二.1）。
    let lib_srcs = LibrarySources::from_registry(&registry)
        .map_err(|e| CheckError::IndexBuild(e.to_string()))?;

    let mut inputs: Vec<ProgramInput> = sources
        .iter()
        .zip(&asts)
        .map(|((domain, path, _), ast)| ProgramInput { domain, path, ast })
        .collect();
    inputs.extend(lib_srcs.program_inputs());

    let (index, hir) = resolve_program_with_libraries(&inputs, &registry)
        .map_err(|e| CheckError::IndexBuild(e.to_string()))?;

    // 用户 AST + 库 AST 同列分析（库节点须建模才能解析调用）；用户诊断与库诊断分离由调用方按需处理，
    // 此处汇总全部语义诊断（库源码自身若有问题也应暴露）。
    let mut ast_refs: Vec<&Ast> = asts.iter().collect();
    ast_refs.extend(lib_srcs.asts());
    let analysis = analyze_program(&ast_refs, &index);

    // strip-assist 等价门禁：对同一批用户源码重解析 + 移除 assist，比对形式核心指纹（库上下文对称）。
    let strip_assist = check_strip_assist_equivalence(sources, &registry, &index)?;

    Ok(CheckReport {
        hir,
        semantic: analysis.diagnostics,
        strip_assist,
    })
}

/// 由原始 AST 集合与 index 构建模型指纹（供 strip-assist 比对复用）。
pub(crate) fn fingerprint(asts: &[&Ast], index: &AsgIndex) -> String {
    SemanticModel::build(asts, index).formal_fingerprint()
}
