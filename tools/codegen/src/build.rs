//! 从源码到 WASM artifact 的便捷入口 + strip-assist artifact 层等价门禁（W5 / A5）。
//!
//! 见 docs/wasm_codegen.md §七.2 / §八。`tools/check` 的 strip-assist 门禁在 **IR 层**比对
//! （移除 assist 前后 `SemanticModel` 指纹 + 语义诊断不变）；本模块把它推进到 **artifact 层**：
//! 移除全部 Semantic Assist 字段前后，emit 的 `.wasm` **字节序列逐字节相等**
//! （docs/language_design.md §5.1、docs/language_implementation.md §12.4）。
//!
//! 前提：emit 确定性（值布局字典序、常量池稳定序、段顺序固定，见 `emit`）——assist 不参与
//! 任何形式核心 / 值布局，故移除后字节必然不变；若不变性被破坏，说明某处 emit 读取了 assist。

use crate::contract::CodegenInput;
use crate::emit_module;
use crate::error::{CodegenError, CodegenResult};
use sophia_hir::ProgramInput;
use sophia_semantic::SemanticModel;
use sophia_syntax::{parse_ast, Ast};

/// 由 `(domain, path, source)` 源码直接 emit WASM 字节。
///
/// `strip` 为真时移除全部 Semantic Assist 字段后再 emit（artifact 门禁用）。前提：源码已通过
/// `sophia-check`（名称解析 + 语义三层 + IR 层 strip-assist），故此处对解析 / index 失败按
/// [`CodegenError::InvalidInput`] 返回（不应发生）。
pub fn emit_from_sources(
    sources: &[(String, String, String)],
    strip: bool,
) -> CodegenResult<Vec<u8>> {
    let asts: Vec<Ast> = sources
        .iter()
        .map(|(_, _, src)| {
            let mut ast = parse_ast(src.as_str())
                .map_err(|e| CodegenError::InvalidInput(format!("解析失败：{e}")))?;
            if strip {
                ast.strip_assists();
            }
            Ok(ast)
        })
        .collect::<CodegenResult<_>>()?;

    let inputs: Vec<ProgramInput> = sources
        .iter()
        .zip(&asts)
        .map(|((domain, path, _), ast)| ProgramInput {
            domain: domain.as_str(),
            path: path.as_str(),
            ast,
        })
        .collect();
    let (index, _diags) =
        sophia_hir::resolve_program_with_libraries(&inputs, &sophia_stdlib::standard_registry())
            .map_err(|e| CodegenError::InvalidInput(format!("名称解析失败：{e}")))?;
    let model = SemanticModel::build(&asts.iter().collect::<Vec<_>>(), &index);
    let refs: Vec<&Ast> = asts.iter().collect();
    let input = CodegenInput::new(&model, &refs);
    emit_module(&input)
}

/// strip-assist **artifact 层**等价门禁结果。
#[derive(Debug, Clone)]
pub struct ArtifactDiffOutcome {
    /// 移除 assist 前后 `.wasm` 字节是否逐字节相等。
    pub equivalent: bool,
    /// 不等价时的差异说明（首个差异字节偏移 + 长度）。
    pub detail: Option<String>,
}

/// 对一批源码做 strip-assist artifact 层等价门禁：emit 原始版 + 移除 assist 版两份 `.wasm`，
/// 断言字节序列逐字节相等。
pub fn check_artifact_strip_equivalence(
    sources: &[(String, String, String)],
) -> CodegenResult<ArtifactDiffOutcome> {
    let original = emit_from_sources(sources, false)?;
    let stripped = emit_from_sources(sources, true)?;
    if original == stripped {
        Ok(ArtifactDiffOutcome {
            equivalent: true,
            detail: None,
        })
    } else {
        Ok(ArtifactDiffOutcome {
            equivalent: false,
            detail: Some(first_byte_diff(&original, &stripped)),
        })
    }
}

/// 首个差异字节偏移摘要（不泄漏全文）。
fn first_byte_diff(a: &[u8], b: &[u8]) -> String {
    if a.len() != b.len() {
        return format!(
            "artifact 长度不同：原 {} 字节，stripped {} 字节",
            a.len(),
            b.len()
        );
    }
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            return format!("artifact 第 {i} 字节差异：原 {x:#04x} ≠ stripped {y:#04x}");
        }
    }
    "无差异".to_string()
}
