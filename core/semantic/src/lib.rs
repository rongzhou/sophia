//! Sophia-Core Semantic IR。
//!
//! 这是 Sophia 最核心的架构层（见 docs/language_implementation.md 第六、七节）。
//! 内部三层结构，对外暴露统一接口：
//! - [`type_layer`]：类型推断与约束求解；
//! - [`effect_layer`]：效应分析与传播；
//! - [`contract_layer`]：capability 满足、error 传播。
//!
//! 推导信息存储在按 `ExprId` 索引的 Table（[`type_layer::TypeTable`]）中，
//! **不修改 AST/声明节点**：声明信息（[`model::SemanticModel`]）不可变，
//! Table 可重算（6.2 Table 模式）。
//!
//! 分层纪律：本 crate 属 `core`，零 IO、不依赖 `workflow`。诊断为编译器诊断
//! （携带 span），与工作流诊断严格分离（14.2）。

#![forbid(unsafe_code)]

pub mod effect;
pub mod model;
pub mod ty;
pub mod type_layer;

// type/effect/contract 三层的检查逻辑是本 crate 内部实现（对外只暴露 `analyze_program` /
// `analyze_one_callable` 的统一入口），不对外按路径访问。
pub(crate) mod contract_layer;
pub(crate) mod effect_layer;
pub(crate) mod union_check;

mod error;

pub use error::{SemanticDiagnostic, SemanticDiagnosticKind};
pub use model::SemanticModel;
pub use ty::{IntentKind, Ty};

use sophia_hir::AsgIndex;
use sophia_syntax::{Ast, Callable, Item};

/// 语义分析结果：声明模型 + 全部诊断。
pub struct Analysis {
    pub model: SemanticModel,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

/// 对整个程序做语义检查（type / effect / contract 三层）。
///
/// 前置条件：调用方应已通过 HIR 名称解析（[`AsgIndex`] 由 HIR 构建）。
/// 本函数假定引用基本可解析，对未解析类型以 [`Ty::Error`] 恢复，避免级联误报。
///
/// 诊断容错收集，确定性顺序（按 callable 名）。
pub fn analyze_program(asts: &[&Ast], index: &AsgIndex) -> Analysis {
    let model = SemanticModel::build(asts, index);
    let mut diagnostics = Vec::new();

    // 全程序级：`one of` 成员可区分性检查（设计 §2.2，按类型位置而非 callable）。
    union_check::check_unions(asts, index, &mut diagnostics);

    // 按 callable 名排序，保证诊断顺序确定。
    for name in model.callables.keys() {
        analyze_callable(name, &model, asts, index, &mut diagnostics);
    }

    Analysis { model, diagnostics }
}

/// 分析单个 callable（按名），返回其诊断。
///
/// 供需要**按 callable 归属**诊断的调用方（如 LSP 按文档分组）使用：模型从全程序
/// 构建以解析跨文件引用，但诊断只来自指定 callable。`model` 应由 [`SemanticModel::build`]
/// 在全程序 AST 上构建；`index` 携带库 op 契约（类型层表驱动校验 `Lib.Op` 用）。
pub fn analyze_one_callable(
    name: &str,
    model: &SemanticModel,
    asts: &[&Ast],
    index: &AsgIndex,
) -> Vec<SemanticDiagnostic> {
    let mut diagnostics = Vec::new();
    analyze_callable(name, model, asts, index, &mut diagnostics);
    diagnostics
}

/// 分析单个 callable，串联三层。
fn analyze_callable(
    name: &str,
    model: &SemanticModel,
    asts: &[&Ast],
    index: &AsgIndex,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) {
    // 找到 callable 所在 AST（用于遍历 body）。
    let Some((ast, callable)) = find_callable(name, asts) else {
        return;
    };
    let decl = model
        .callables
        .get(name)
        .expect("callable 必在 model 中（与 asts 同源）");

    // 1) 类型层：推导 + 类型/intent 检查，收集 used effects（库 op 经 index 契约表驱动校验）。
    let out = type_layer::TypeChecker::new(model, ast, index).check_callable(name);
    diagnostics.extend(out.diags);

    // 2) 效应层：used ⊆ declared、Pure 冲突。
    effect_layer::check_effects(decl, &out.used_effects, callable.span, diagnostics);

    // 3) 契约层：capability 满足、error 传播。
    contract_layer::check_contracts(callable, decl, &out.used_effects, model, ast, diagnostics);
}

/// 在 AST 集合中定位某 callable 及其所属 AST。
fn find_callable<'a>(name: &str, asts: &'a [&'a Ast]) -> Option<(&'a Ast, &'a Callable)> {
    for ast in asts {
        for item in &ast.items {
            if let Item::Action(c) | Item::Transition(c) = item {
                if c.name.text == name {
                    return Some((ast, c));
                }
            }
        }
    }
    None
}
