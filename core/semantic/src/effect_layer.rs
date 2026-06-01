//! 效应层：代数效应分析与传播。
//!
//! 见 docs/language_implementation.md 7.3。检查：
//! - body 使用的所有 effect 必须包含在 callable 声明的 effects 中；
//! - `Pure` 与其他 effect 互斥；
//! - 被调用 action 的 observable effects 必须是调用方 effects 的子集
//!   （`Pure` 不要求调用方重复声明）。
//!
//! 推导信息（body 实际 used effects）由类型层遍历时一并收集：遇到 `print`
//! 并入 `Console.Write`，遇到 action 调用并入被调用方声明的 effects。因此
//! 「被调用方 effect ⊆ 调用方 effect」与「body effect ⊆ 声明 effect」在本层
//! 统一表达为「used ⊆ declared」，无需重复遍历 body。

use crate::effect::EffectSet;
use crate::error::{SemanticDiagnostic, SemanticDiagnosticKind as K};
use crate::model::CallableDecl;
use sophia_syntax::Span;

/// 对一个 callable 做 effect 检查。
///
/// `used` 是类型层收集到的 body 实际 effect 集合（含被调用 action 的 effect）；
/// `decl_span` 用于诊断定位（指向 callable 声明）。
pub fn check_effects(
    decl: &CallableDecl,
    used: &EffectSet,
    decl_span: Span,
    diags: &mut Vec<SemanticDiagnostic>,
) {
    let declared = &decl.declared_effects;

    // 1) used ⊆ declared：body 使用（含调用传播）的每个 effect 必须被声明。
    //    这同时覆盖「被调用方 effect 必须由调用方声明」（7.3 子集规则）。
    for e in used.iter() {
        if !declared.contains(e) {
            diags.push(SemanticDiagnostic::new(
                K::UndeclaredEffect,
                decl_span,
                format!("`{}` 使用了未声明的 effect {}", decl.name, e),
            ));
        }
    }

    // 2) Pure 冲突：声明为纯（空集）却使用了 effect。
    if declared.is_pure() && !used.is_pure() {
        diags.push(SemanticDiagnostic::new(
            K::PureConflict,
            decl_span,
            format!("`{}` 声明为 Pure（无 effect），但产生了 effect", decl.name),
        ));
    }
}
