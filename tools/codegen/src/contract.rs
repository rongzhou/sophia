//! codegen 输入契约（A1：冻结，不改 IR 形状）。
//!
//! 见 docs/wasm_codegen.md §三。本模块把「codegen 消费什么」**代码化冻结**为单一入口
//! [`CodegenInput`]。冻结的含义：codegen **只读消费**以下三者，**绝不**反向要求改它们的形状
//! （docs/language_implementation.md §12.1）。任何"为了 codegen 方便而改 IR"的冲动都违反此契约。
//!
//! ## 冻结的三个输入
//!
//! 1. **`SemanticModel`**（`sophia_semantic::SemanticModel`，声明视图）——entity 字段类型 / state
//!    值集 / error variant 字段 / capability allow·deny / callable 签名（inputs / outputs /
//!    declared_effects / declared_errors / capability / intent_conversion）。codegen 据此生成函数
//!    签名、值 ABI 的结构布局、effect host import 的注入判定、runtime validation 的元信息。
//!
//! 2. **`ExecGraph`**（`sophia_exec_ir::ExecGraph`，callable 粒度执行图）——决定 emit 哪些 WASM
//!    function（每节点一个 action / transition）与调用关系（`Control` 边 → WASM `call`）。**body
//!    语句级执行不在图上展开**（设计如此，见 exec-ir）：body 由 codegen 像解释器一样遍历 AST 生成。
//!
//! 3. **AST body + 可重算的 `TypeTable`**（`sophia_syntax::Ast` + `sophia_semantic::type_layer`）——
//!    codegen 遍历 callable 的 body AST 生成函数体指令（与解释器同源，**不引入新 body IR**，见 §三
//!    决策点 ①）。需要静态分派时（如 `Add` 的 Int vs Text）按 `ExprId` 查 `TypeTable`。`TypeTable`
//!    是 Table 模式可重算产物（语义 6.2），codegen 经 `TypeChecker::check_callable` 按需重算，
//!    **不要求** `analyze_program` 额外暴露它（保持语义 crate 接口不变）。
//!
//! ## 不在契约里的东西
//!
//! - **不消费 HIR `AsgIndex`**：名称解析已在 check 阶段完成并固化进 `SemanticModel`；codegen 不重做
//!   名称解析。
//! - **不消费诊断**：输入应来自**已通过 `sophia-check`** 的程序（与解释器一致：执行前已 check 通过）。
//! - **不引入新 IR 层**：没有 lowered body IR、没有 codegen 专用中间表示（YAGNI + 避免双真相源）。

use sophia_exec_ir::ExecGraph;
use sophia_hir::{AsgIndex, LibraryRegistry};
use sophia_semantic::SemanticModel;
use sophia_syntax::Ast;

/// codegen 的冻结输入契约（A1）。
///
/// 把 [`SemanticModel`]（声明视图）、[`ExecGraph`]（执行图）、全程序 AST 集合捆为单一只读入口。
/// `ExecGraph` 由 [`CodegenInput::new`] 从模型 + AST 构建（与解释器构图同源
/// `ExecGraph::from_model`），保证 codegen 与解释器看到**同一张执行图**。
///
/// 生命周期 `'a` 借用调用方持有的模型与 AST——codegen 零拷贝、不持有所有权。
pub struct CodegenInput<'a> {
    /// 语义声明模型（只读）。
    model: &'a SemanticModel,
    /// 全程序 AST 集合（跨 callable 调用需覆盖整个程序；与解释器 `asts` 同形）。
    asts: &'a [&'a Ast],
    /// callable 粒度执行图（决定 emit 哪些 function 与调用边）。
    graph: ExecGraph,
    /// 携带库 op 契约的 index（仅 `library_op` / `is_library_family`，由标准库注册表注入）。
    /// emit 重算 `TypeTable` 时类型层据此对 `Lib.Op(args)` 给出返回类型（表驱动，见
    /// docs/stdlib_design.md）；nodes 来自 `model`，故这里只需 library-only index。
    lib_index: AsgIndex,
    /// 完整库注册表。emit 阶段据此从实际使用到的库 op 派生 host import 表。
    registry: &'a LibraryRegistry,
}

impl<'a> CodegenInput<'a> {
    /// 由语义模型 + 全程序 AST + 库注册表构建 codegen 输入。
    ///
    /// 内部用 [`ExecGraph::from_model`] 构图——与解释器（`runtime::Interpreter::new`）**同一份**
    /// 构图逻辑，保证两后端看到同一执行图（差测试等价的前提之一）。库 op 的类型契约必须由调用方显式
    /// 传入（标准库确定性路径传 `standard_registry()`，CLI 生产路径传 full registry）。
    pub fn new(
        model: &'a SemanticModel,
        asts: &'a [&'a Ast],
        registry: &'a LibraryRegistry,
    ) -> Self {
        let graph = ExecGraph::from_model(model, asts);
        let lib_index = AsgIndex::new(registry);
        CodegenInput {
            model,
            asts,
            graph,
            lib_index,
            registry,
        }
    }

    /// 语义声明模型（只读）。
    pub fn model(&self) -> &SemanticModel {
        self.model
    }

    /// 全程序 AST 集合（只读）。
    pub fn asts(&self) -> &[&'a Ast] {
        self.asts
    }

    /// callable 粒度执行图（只读）。
    pub fn graph(&self) -> &ExecGraph {
        &self.graph
    }

    /// 库契约 index（只读，供 emit 重算 TypeTable 时类型层查 `library_op`）。
    pub fn lib_index(&self) -> &AsgIndex {
        &self.lib_index
    }

    /// 完整库注册表（只读）。
    pub fn registry(&self) -> &LibraryRegistry {
        self.registry
    }
}
