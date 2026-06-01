//! Sophia-Core HIR 层。
//!
//! 职责（见 docs/language_implementation.md 第五节）：名称解析、模块解析、
//! symbol binding、scope analysis。是第一个具有明确语义意义的中间层。
//!
//! 规则（5.2）：
//! - 所有引用必须可由 `asg_index.json` 解析；
//! - 禁止隐式 import；
//! - 禁止同名 shadowing（包括 body 局部变量）；
//! - 跨 domain 引用必须通过 boundary 或 task include 显式声明。
//!
//! 分层纪律：本 crate 属 `core`，零 IO。ASG index 由调用方（CLI 文件扫描）
//! 从内存 AST 集合构建后传入；HIR 只做纯确定性分析。
//!
//! 诊断采用容错收集：不在首个错误中断，便于一次反馈多个问题。

#![forbid(unsafe_code)]

mod builtins;
mod closure;
mod error;
mod index;
mod resolve;
mod scope;

pub use closure::{
    action_context, task_context, ClosureError, ClosureNode, ContextClosure, ContextEdge,
    ContextEdgeKind,
};
pub use error::{HirDiagnostic, HirDiagnosticKind, HirError, HirResult};
pub use index::{AsgIndex, EffectOpInfo, IndexInput, NodeInfo, NodeKind, ASG_INDEX_VERSION};
pub use resolve::resolve_item;
// `scope::{Binding, ScopeStack}` 是 resolve 的内部脚手架，不对外（crate 内经 `crate::scope` 直接用）。
// 重导出库契约类型，便于上层（CLI / 工具）构建 registry 与消费契约而无需再依赖 sophia-library。
pub use sophia_library::{
    LibraryContent, LibraryError, LibraryRegistry, OpContract, Scalar, TypeDesc,
};

use sophia_syntax::Ast;

/// 一个待解析的源文件单元：domain + 路径 + AST。
pub struct ProgramInput<'a> {
    pub domain: &'a str,
    pub path: &'a str,
    pub ast: &'a Ast,
}

/// 库随附的 Sophia 源码节点,解析为 **owned AST** 集——与用户 AST 同列进 index / model / 执行
/// （纯 Sophia 源码库就是「更多 Sophia 代码」,见 docs/stdlib_design.md §二.1）。
///
/// 调用方在自身作用域持有它,再把 [`Self::program_inputs`] 并入用户 inputs（供 resolve / index）、
/// [`Self::asts`] 并入用户 AST（供 `analyze_program` / `run_action`）。库节点的 domain = 库名
/// （隔离,见 `LibraryRegistry`）;用户跨 domain 引用库节点经 [`AsgIndex::is_library_domain`] 豁免。
pub struct LibrarySources {
    parsed: Vec<(String, String, Ast)>,
}

impl LibrarySources {
    /// 解析注册表里全部库 Sophia 源码节点为 owned AST（解析失败即 `Err`）。
    pub fn from_registry(registry: &LibraryRegistry) -> HirResult<Self> {
        let mut parsed = Vec::new();
        for src in registry.sophia_sources() {
            let ast = sophia_syntax::parse_ast(src.source.as_str()).map_err(|e| {
                HirError::LibrarySourceParse {
                    lib: src.lib.clone(),
                    path: src.path.clone(),
                    reason: e.to_string(),
                }
            })?;
            parsed.push((src.domain.clone(), src.path.clone(), ast));
        }
        Ok(LibrarySources { parsed })
    }

    /// 库节点的 `ProgramInput`（供 resolve / index 与用户 inputs 合并）。
    pub fn program_inputs(&self) -> Vec<ProgramInput<'_>> {
        self.parsed
            .iter()
            .map(|(d, p, a)| ProgramInput {
                domain: d,
                path: p,
                ast: a,
            })
            .collect()
    }

    /// 库节点的 AST 引用（供 `analyze_program` / `run_action` 与用户 AST 合并）。
    pub fn asts(&self) -> Vec<&Ast> {
        self.parsed.iter().map(|(_, _, a)| a).collect()
    }

    /// 是否无库源码节点（纯 effect-op 库 / 无库时为空,调用方可走零开销路径）。
    pub fn is_empty(&self) -> bool {
        self.parsed.is_empty()
    }
}

/// 对整个程序（一组源文件）做名称解析与 scope 分析（**无库**：仅语言内置 `Console`）。
///
/// 流程（对应 docs/language_implementation.md 第二节管线的 “节点索引 → 名称解析”）：
/// 1. 从全部输入构建 [`AsgIndex`]（含一文件一节点、禁止重名校验）；
/// 2. 对每个节点用该 index 解析引用、检查 scope，汇总诊断。
///
/// 用到标准库 / 三方库的程序应改用 [`resolve_program_with_libraries`]（注入库注册表）。
/// index 构建失败（硬错误）以 `Err` 返回；名称解析诊断以 `Ok((index, diags))` 返回。
pub fn resolve_program(inputs: &[ProgramInput<'_>]) -> HirResult<(AsgIndex, Vec<HirDiagnostic>)> {
    resolve_program_with_libraries(inputs, &LibraryRegistry::empty())
}

/// 同 [`resolve_program`]，但注入**库注册表**（标准库 + 三方库经清单构建）。
///
/// index 据 `registry` 叠加库特殊根 family 与 effect / op 契约，使库的 `Lib.Op(args)` 调用能被
/// 名称解析放行、被语义层表驱动校验——核心不硬编码具体库（见 docs/stdlib_design.md）。空注册表
/// 等价于 [`resolve_program`]。
pub fn resolve_program_with_libraries(
    inputs: &[ProgramInput<'_>],
    registry: &LibraryRegistry,
) -> HirResult<(AsgIndex, Vec<HirDiagnostic>)> {
    let index_inputs: Vec<IndexInput<'_>> = inputs
        .iter()
        .map(|i| IndexInput {
            domain: i.domain,
            path: i.path,
            ast: i.ast,
        })
        .collect();
    let index = AsgIndex::build(index_inputs)?.with_libraries(registry);

    let mut diags = Vec::new();
    // 按 path 排序保证诊断顺序确定。
    let mut ordered: Vec<&ProgramInput<'_>> = inputs.iter().collect();
    ordered.sort_by(|a, b| a.path.cmp(b.path));
    for input in ordered {
        for item in &input.ast.items {
            diags.extend(resolve_item(item, input.ast, &index, input.domain));
        }
    }
    Ok((index, diags))
}
