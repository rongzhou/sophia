//! Strip-assist 等价门禁。
//!
//! 见 docs/language_design.md 5.1、docs/language_implementation.md 12.4：
//! 移除全部 Semantic Assist 字段（meaning / not / purpose / ... 及 entity 的
//! semantic_identity / evolution）后，Formal Core 与 Semantic IR 必须**完全不变**。
//!
//! 实现：对同一批源码解析两份 AST，其一调用 `Ast::strip_assists`，分别构建
//! `SemanticModel` 并比对形式核心指纹（`formal_fingerprint`，确定性、无 span、无 assist）。
//! 指纹一致即等价；不一致说明某处形式核心推导读取了 assist 字段（设计禁止）。

use crate::{fingerprint, CheckError, CheckResult};
use sophia_hir::{resolve_program, AsgIndex, LibraryRegistry, LibrarySources, ProgramInput};
use sophia_semantic::analyze_program;
use sophia_syntax::Ast;

/// strip-assist 门禁结果。
#[derive(Debug, Clone)]
pub struct StripAssistOutcome {
    /// 移除 assist 前后 Semantic IR 是否等价。
    pub equivalent: bool,
    /// 不等价时的差异说明（首个差异点摘要）。
    pub detail: Option<String>,
}

/// 对一批源码做 strip-assist 等价门禁。
///
/// `registry` 是该程序的库注册表（标准库 / 标准库 + 三方）；`original_index` 是原始源码**已并入
/// 库源码**构建的 index（复用，避免重复构建）。stripped 版本重新解析、移除 assist，并**对称地**
/// 并入同一批库源码、用同一 `registry` 独立构建 index 与模型比对——两侧库上下文完全一致，差异只能
/// 来自用户代码的 assist 移除（否则库 op / 库节点解析不对称会造成误判）。
///
/// 比对覆盖**整个 Semantic IR 的可观测投影**（design 5.1 要求 Semantic IR 不变）：
/// 形式核心指纹（声明 IR）+ 语义三层诊断输出（type/effect/contract 分析结果）。
/// 任一不同即判定 strip-assist 改变了形式核心 / IR。指纹只覆盖**用户源码**（库源码两侧相同、相消）。
pub fn check_strip_assist_equivalence(
    sources: &[(String, String, String)],
    registry: &LibraryRegistry,
    original_index: &sophia_hir::AsgIndex,
) -> CheckResult<StripAssistOutcome> {
    // 原始：模型指纹 + 语义诊断（用户 AST，库上下文由 original_index 提供）。
    let original_asts: Vec<Ast> = parse_all(sources, false);
    let original_fp = ir_fingerprint(&original_asts, original_index);

    // 库源码（与原始侧并入 index 的同一批）——stripped 侧须对称并入，否则库节点 / 库 op 解析不对称。
    let lib_srcs = LibrarySources::from_registry(registry)
        .map_err(|e| CheckError::IndexBuild(e.to_string()))?;

    // stripped：移除 assist 后重新构建 index 与指纹（并入同一批库源码 + 同一 registry）。
    let stripped_asts: Vec<Ast> = parse_all(sources, true);
    let mut stripped_inputs: Vec<ProgramInput> = sources
        .iter()
        .zip(&stripped_asts)
        .map(|((domain, path, _), ast)| ProgramInput { domain, path, ast })
        .collect();
    stripped_inputs.extend(lib_srcs.program_inputs());
    let (stripped_index, _diags) = resolve_program(&stripped_inputs, registry)
        .map_err(|e| CheckError::IndexBuild(e.to_string()))?;
    let stripped_fp = ir_fingerprint(&stripped_asts, &stripped_index);

    if original_fp == stripped_fp {
        Ok(StripAssistOutcome {
            equivalent: true,
            detail: None,
        })
    } else {
        Ok(StripAssistOutcome {
            equivalent: false,
            detail: Some(first_diff(&original_fp, &stripped_fp)),
        })
    }
}

/// 解析全部源码；`strip` 为真时移除 assist。
fn parse_all(sources: &[(String, String, String)], strip: bool) -> Vec<Ast> {
    sources
        .iter()
        .map(|(_, _, src)| {
            let mut ast = sophia_syntax::parse_str(src.as_str())
                .expect("parse")
                .to_ast();
            if strip {
                ast.strip_assists();
            }
            ast
        })
        .collect()
}

/// Semantic IR 指纹：声明模型形式核心指纹 + 语义三层诊断（确定性顺序）。
fn ir_fingerprint(asts: &[Ast], index: &AsgIndex) -> String {
    let refs: Vec<&Ast> = asts.iter().collect();
    let model_fp = fingerprint(&refs, index);
    let analysis = analyze_program(&refs, index);
    let diag_fp: String = analysis
        .diagnostics
        .iter()
        .map(|d| format!("{}|{}\n", d.code(), d.message))
        .collect();
    format!("{model_fp}\n=== diagnostics ===\n{diag_fp}")
}

/// 给出两份指纹首个差异行的摘要（便于诊断，不泄漏全文）。
fn first_diff(a: &str, b: &str) -> String {
    for (i, (la, lb)) in a.lines().zip(b.lines()).enumerate() {
        if la != lb {
            return format!("第 {} 行差异：`{}` ≠ `{}`", i + 1, la.trim(), lb.trim());
        }
    }
    "指纹长度不同".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_diff_points_to_changed_line() {
        let a = "line1\nline2\nline3";
        let b = "line1\nCHANGED\nline3";
        let d = first_diff(a, b);
        assert!(d.contains("第 2 行"), "应定位到第 2 行：{d}");
    }

    #[test]
    fn equivalent_fingerprints_have_no_diff_path() {
        // 同串无差异行 → 走「长度不同」兜底（此处长度相同但无差异，
        // 仅用于确认相等时不会误报差异行）。
        let a = "x\ny";
        assert_eq!(first_diff(a, a), "指纹长度不同");
    }
}
