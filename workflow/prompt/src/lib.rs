//! Prompt 模板管理。
//!
//! Prompt 是工作流引擎的核心资产（见 docs/engineering_architecture.md 第八节）：
//! - 用 `minijinja` 渲染，**不用字符串拼接**；
//! - 每个 template 对应一个 schema 文件，作为 `complete_structured` 的输入；
//! - 模板变更用 `insta` snapshot testing 捕获渲染结果，防止静默影响 LLM 行为。
//!
//! 模板内嵌于二进制（`include_str!`），保证产物自包含、可复现；调用方用 serde
//! 可序列化的上下文渲染。

#![forbid(unsafe_code)]

use minijinja::Environment;
use serde::Serialize;
use thiserror::Error;

/// Prompt 层结果别名。
pub type PromptResult<T> = Result<T, PromptError>;

/// Prompt 层错误。
#[derive(Debug, Error)]
pub enum PromptError {
    /// 未知模板名。
    #[error("未知模板：{0}")]
    UnknownTemplate(String),

    /// 模板渲染失败。
    #[error("模板渲染失败：{0}")]
    Render(String),
}

/// 内置模板：名字 → 源文本。
///
/// 对应 docs/engineering_architecture.md 8.1 的 prompt 模板族。每个模板对应一个
/// schema（见 [`schema_for`]）。
const TEMPLATES: &[(&str, &str)] = &[
    (
        "design_solution",
        include_str!("../templates/design_solution.md.jinja"),
    ),
    (
        "implement_design",
        include_str!("../templates/implement_design.md.jinja"),
    ),
    (
        "repair_code",
        include_str!("../templates/repair_code.md.jinja"),
    ),
    (
        "revise_design",
        include_str!("../templates/revise_design.md.jinja"),
    ),
    ("decision", include_str!("../templates/decision.md.jinja")),
    ("decompose", include_str!("../templates/decompose.md.jinja")),
];

/// 内置 JSON Schema：模板名 → schema 源文本。
const SCHEMAS: &[(&str, &str)] = &[
    (
        "design_result",
        include_str!("../schemas/design_result.json"),
    ),
    (
        "implement_result",
        include_str!("../schemas/implement_result.json"),
    ),
    ("decision", include_str!("../schemas/decision_node.json")),
    (
        "decompose_result",
        include_str!("../schemas/decompose_result.json"),
    ),
    ("pseudo_check", include_str!("../schemas/pseudo_check.json")),
    (
        "repair_result",
        include_str!("../schemas/repair_result.json"),
    ),
];

/// 工作流 prompt 步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PromptStep {
    Decision,
    Decompose,
    DesignSolution,
    ImplementDesign,
    RepairCode,
    ReviseDesign,
}

impl PromptStep {
    /// 全部内置工作流步骤，按稳定顺序返回。
    pub fn all() -> &'static [PromptStep] {
        &[
            PromptStep::Decision,
            PromptStep::Decompose,
            PromptStep::DesignSolution,
            PromptStep::ImplementDesign,
            PromptStep::RepairCode,
            PromptStep::ReviseDesign,
        ]
    }
}

/// 单个工作流 prompt 步骤的模板 / schema 绑定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptSpec {
    pub step: PromptStep,
    pub template: &'static str,
    pub schema: &'static str,
}

/// 取工作流步骤的模板 / schema 绑定。
pub fn spec_for(step: PromptStep) -> PromptSpec {
    match step {
        PromptStep::Decision => PromptSpec {
            step,
            template: "decision",
            schema: "decision",
        },
        PromptStep::Decompose => PromptSpec {
            step,
            template: "decompose",
            schema: "decompose_result",
        },
        PromptStep::DesignSolution => PromptSpec {
            step,
            template: "design_solution",
            schema: "design_result",
        },
        PromptStep::ImplementDesign => PromptSpec {
            step,
            template: "implement_design",
            schema: "implement_result",
        },
        PromptStep::RepairCode => PromptSpec {
            step,
            template: "repair_code",
            schema: "repair_result",
        },
        PromptStep::ReviseDesign => PromptSpec {
            step,
            template: "revise_design",
            schema: "design_result",
        },
    }
}

/// 内置 prompt 资产（system preamble 等）：名字 → 源文本。
///
/// 见 docs/engineering_architecture.md 8.3。与 templates / schemas 同为 prompt 核心资产，
/// 但不参与 jinja 渲染——它们是直接拼入 `system` 的稳定文本块（如语言语法基线）。
///
/// `sophia_syntax_baseline`：Sophia-Core 语法基线，供任何要求 LLM **产出 / 修复
/// `.sophia` 源码**的步骤（implement / repair）作 system preamble。**只含可泛化的标准
/// 语法规则与中立示例，不含任何具体任务的答案 / 领域名 / 逻辑**（防答案泄漏）。
const ASSETS: &[(&str, &str)] = &[(
    "sophia_syntax_baseline",
    include_str!("../assets/sophia_syntax_baseline.md"),
)];

/// Prompt 注册表：持有 minijinja 环境与全部内置模板。
pub struct PromptRegistry {
    env: Environment<'static>,
}

impl PromptRegistry {
    /// 加载全部内置模板。
    pub fn new() -> Self {
        let mut env = Environment::new();
        // 未定义变量报错而非静默成空，避免模板与上下文不一致被掩盖。
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        for (name, src) in TEMPLATES {
            env.add_template(name, src)
                .expect("内置模板应能编译（由 snapshot 测试守护）");
        }
        PromptRegistry { env }
    }

    /// 渲染指定模板。
    pub fn render<S: Serialize>(&self, name: &str, ctx: S) -> PromptResult<String> {
        let tmpl = self
            .env
            .get_template(name)
            .map_err(|_| PromptError::UnknownTemplate(name.to_string()))?;
        tmpl.render(ctx)
            .map_err(|e| PromptError::Render(e.to_string()))
    }

    /// 内置模板名列表（字典序，确定性）。
    pub fn template_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = PromptStep::all()
            .iter()
            .map(|step| spec_for(*step).template)
            .collect();
        names.sort_unstable();
        names
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 取某模板 / 用途对应的 JSON Schema 源文本（`complete_structured` 的输入）。
pub fn schema_for(name: &str) -> Option<&'static str> {
    SCHEMAS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, src)| *src)
}

/// 取某 prompt 资产（system preamble 等）的源文本（见 docs/engineering_architecture.md 8.3）。
///
/// 例如 `preamble("sophia_syntax_baseline")` 取 Sophia-Core 语法基线，供 implement /
/// repair 步骤拼入 `system`。资产内嵌于二进制（`include_str!`），自包含可复现。
pub fn preamble(name: &str) -> Option<&'static str> {
    ASSETS.iter().find(|(n, _)| *n == name).map(|(_, src)| *src)
}

// ---- 工作流步骤的 system prompt（规范文案，单一来源） ----
//
// design / implement / repair 的 system prompt 文案此前在 e2e harness / benchmark sophia_mode /
// CLI graph_cmd 三处各有一份"碰巧一致"的副本，易静默漂移（graph_cmd 副本已漂移过）。
// 这里收敛为 prompt crate 的单一来源——它本就是 prompt 资产的归属层（preamble / 模板）。
//
// **标准库知识不在 prompt crate**：库目录 / 库资产由 `sophia-stdlib` 的 `LibraryRegistry` 承载
// （清单驱动，见 docs/stdlib_design.md）。design 模板的 `stdlib_catalog` 变量由调用方传入
// `registry.catalog()`；implement system prompt 的库资产块由调用方传入 `registry.preamble(libs)`。

/// design 阶段 system prompt：产语义伪代码 + 从库目录选库（不注入语法基线，semantics > format）。
pub fn design_system_prompt() -> String {
    "你是 Sophia 工作流的设计者。只输出一个 JSON 对象，严格符合 design_result schema：\
     purpose（字符串）、pseudocode（字符串）、libraries（字符串数组，从库目录选用的标准库名；\
     不用库则空数组）。pseudocode 是**单个**结构化伪代码文档（可在其中用文字描述多个业务部分），\
     **不是**文件数组，也不是 Sophia 源码或文件布局。不要输出 markdown 围栏。"
        .to_string()
}

/// implement / repair 共用 system prompt：常驻语法基线（§8.3）+ 按需标准库资产（S2）+ 输出形状。
///
/// `stdlib_block` 是调用方据 design 所选库从库注册表算得的资产文本（`registry.preamble(libs)`，
/// 见 docs/stdlib_design.md §三）；空串 = 仅常驻基线（零回归）。prompt crate 不持有库内容——库知识
/// 归 `sophia-stdlib`，本函数只负责把它拼进统一的 system 文案。
pub fn implement_system_prompt(stdlib_block: &str) -> String {
    let baseline = preamble("sophia_syntax_baseline")
        .expect("prompt crate 应内置 sophia_syntax_baseline 资产");
    let stdlib_block = if stdlib_block.is_empty() {
        String::new()
    } else {
        format!("{stdlib_block}\n\n")
    };
    format!(
        "你是 Sophia 工作流的实现者，把伪代码实现为可通过确定性检查的 Sophia-Core 源码。\n\n\
         {baseline}\n\n\
         {stdlib_block}\
         输出要求：只输出一个 JSON 对象。实现阶段形状为 \
         {{\"files\":[{{\"path\":\"<domain-first 路径>\",\"content\":\"<完整 .sophia 源码>\"}}]}}；\
         若收到修复诊断，则形状为 {{\"files\":[...同上...],\"changes\":[\"<每处修改简述>\"]}}。\
         不要包裹 implement_result / repair_result 之类外层键，不要输出 markdown 围栏。\
         每个顶层 node 一个文件，path 用 domain-first 布局：\
         `<域名>/<复数类别>/<节点名>.sophia`（类别如 actions / states / entities / errors）。"
    )
}
