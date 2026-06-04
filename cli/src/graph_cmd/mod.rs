//! Development Graph 工作流子命令。
//!
//! 见 docs/engineering_architecture.md 9.2、docs/workflow_graph_spec.md。
//! 这些命令在 `sophia-runs/graph/dev_graph.sqlite` 上以事件溯源 append 节点 / 边
//! （仅增、不可变 N1/N2/I9）。确定性命令（不调用 LLM）：`init`（建库）/ `start`（人类目标）/
//! `context`（推导 active context）/ `nodes`（列节点）/ `select` / `materialize`（重跑 gate +
//! staging/rename 写盘）。调用 LLM 的命令：`design`（→ Pseudocode）/ `implement_loop`（implement→check→repair
//! 预算闭环）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use sophia_engine::{
    code_check, design_solution, run_implement_loop, ImplementLoopConfig, ImplementLoopOutcome,
    LibrarySelectionPolicy, LoopStepOutcome,
};
use sophia_graph_db::{
    derive_active_context, ActiveContext, DiagnosticItem, DiagnosticKind, DiagnosticPayload,
    DiagnosticSeverity, EdgeKind, GraphStore, NodeId, NodeRole, ObjectivePayload,
};
use sophia_hir::LibraryRegistry;
use sophia_llm::{BackendConfig, CompletionRequest, HttpLlmClient, StructuredConfig};
use sophia_prompt::PromptRegistry;

mod gate;

use crate::commands::library_registry;

pub use gate::{materialize, select};

/// Development Graph SQLite 文件的标准路径。
fn graph_path(root: &Path) -> PathBuf {
    root.join("sophia-runs/graph/dev_graph.sqlite")
}

/// 打开（或创建）Development Graph 存储。
pub(super) fn open_store(root: &Path) -> Result<GraphStore> {
    let path = graph_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建图目录 {} 失败", parent.display()))?;
    }
    GraphStore::open(&path)
        .with_context(|| format!("打开 Development Graph {} 失败", path.display()))
}

/// 解析规范 NodeId（`N0001`）字符串。
pub(super) fn parse_node(s: &str) -> Result<NodeId> {
    NodeId::parse(s).with_context(|| format!("非法 NodeId `{s}`"))
}

/// 校验 `id` 是 `addresses→` 允许的目标域（Objective | Milestone | FirstSlice）。
///
/// `what` 用于错误信息（如 `"design"` / `"implement-loop"`）。
fn expect_target_domain(store: &GraphStore, id: NodeId, what: &str) -> Result<()> {
    match store.role_of(id) {
        Some(NodeRole::Objective | NodeRole::Milestone | NodeRole::FirstSlice) => Ok(()),
        Some(other) => {
            anyhow::bail!("{id} 是 {other:?}，{what} 需 Objective | Milestone | FirstSlice")
        }
        None => anyhow::bail!("{id} 不存在于 Development Graph"),
    }
}

/// `sophia graph init`：初始化 Development Graph 存储。
pub fn init(root: &Path) -> Result<ExitCode> {
    let path = graph_path(root);
    if path.exists() {
        println!("Development Graph 已存在：{}", path.display());
        return Ok(ExitCode::SUCCESS);
    }
    // 打开即创建空事件流（schema 在 open 时建立）。
    let store = open_store(root)?;
    let n = store.nodes().count();
    println!(
        "已初始化 Development Graph：{}（{n} 个节点）",
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// `sophia graph start <title>`：以人类目标开启工作流，创建 ObjectiveNode。
pub fn start(root: &Path, title: &str, description: Option<&str>) -> Result<ExitCode> {
    if title.trim().is_empty() {
        anyhow::bail!("目标标题不能为空");
    }
    let mut store = open_store(root)?;
    let description = description.unwrap_or(title).to_string();
    let id = store
        .as_human()
        .objective(
            title,
            ObjectivePayload {
                title: title.to_string(),
                description,
            },
        )
        .context("创建 ObjectiveNode 失败")?;
    println!("已创建目标 {id}（provenance=human）：{title}");
    Ok(ExitCode::SUCCESS)
}

/// `sophia graph context`：推导并展示当前 active context（不写图）。
pub fn context(root: &Path) -> Result<ExitCode> {
    let store = open_store(root)?;
    let ctx = derive_active_context(&store);

    println!("Active Context（digest {}）：", ctx.digest);
    print_section(
        "绑定目标",
        ctx.bound_objectives
            .iter()
            .map(|o| format!("{} {}", o.id, o.title)),
    );
    match &ctx.active_milestone {
        Some(m) => println!("  active milestone：{} {}", m.id, m.name),
        None => println!("  active milestone：（无）"),
    }
    print_section(
        "绑定约束",
        ctx.bound_constraints
            .iter()
            .map(|c| format!("{} [{:?}] {}", c.id, c.kind, c.statement)),
    );
    print_section(
        "绑定验收条件",
        ctx.bound_acceptance_criteria
            .iter()
            .map(|a| format!("{} {}", a.id, a.statement)),
    );
    print_section(
        "待处理变更请求",
        ctx.open_change_requests
            .iter()
            .map(|cr| format!("{} {}", cr.id, cr.request)),
    );
    print_section(
        "未决澄清问题",
        ctx.outstanding_questions
            .iter()
            .map(|q| format!("{} {}", q.id, q.body)),
    );
    Ok(ExitCode::SUCCESS)
}

/// `sophia graph nodes`：列出图中全部节点（按 ID 升序）。
pub fn nodes(root: &Path) -> Result<ExitCode> {
    let store = open_store(root)?;
    let count = store.nodes().count();
    if count == 0 {
        println!("Development Graph 为空。");
        return Ok(ExitCode::SUCCESS);
    }
    println!("Development Graph（{count} 个节点）：");
    for node in store.nodes() {
        println!(
            "  {:<7} {:<20} {:<14} {}",
            node.meta.id.as_string(),
            format!("{:?}", node.meta.role),
            format!("{:?}", node.meta.provenance),
            node.meta.summary
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// 打印一个带标题的列表区块（空时显示「（无）」）。
fn print_section(title: &str, items: impl Iterator<Item = String>) {
    let collected: Vec<String> = items.collect();
    if collected.is_empty() {
        println!("  {title}：（无）");
    } else {
        println!("  {title}：");
        for item in collected {
            println!("    {item}");
        }
    }
}

/// 存放工作流中间产物（未物化的 `.pseudo` / 候选文件）的目录。
pub(super) fn artifacts_dir(root: &Path) -> PathBuf {
    root.join("sophia-runs/graph/artifacts")
}

/// `sophia graph design <NodeId>`：为目标域生成结构化伪代码（调用 LLM）。
///
/// 流程：解析目标域节点 → 由 active context 渲染 `design_solution` prompt →
/// `engine::design_solution`（建 snapshot → LLM → 建 PseudocodeNode / RawLlmNode 兜底）→
/// 成功则把 `.pseudo` 正文落盘到 `sophia-runs/graph/artifacts/<PseudoId>.pseudo`。
///
/// LLM 失败（后端不可用 / schema 不符）以失败退出码呈现，并保留 RawLlmNode（不伪造成功）。
pub fn design(
    root: &Path,
    node: &str,
    model: &str,
    mode: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<ExitCode> {
    let target = parse_node(node)?;
    let mut store = open_store(root)?;
    expect_target_domain(&store, target, "design")?;

    let client = build_client(model, mode, base_url, api_key)?;
    let prompt = PromptRegistry::new();
    let registry = library_registry(root)?;
    let library_policy = LibrarySelectionPolicy::from_names(registry.lib_names());

    // prompt 在调用时刻据 active context 渲染（与 snapshot 同源，见 architecture §8.4）。
    let model = model.to_string();
    let outcome = tokio_block_on(design_solution(
        &mut store,
        &client,
        |ctx: &sophia_graph_db::ActiveContext| {
            render_design_request(&prompt, ctx, &model, &registry)
        },
        &StructuredConfig::default(),
        &library_policy,
        target,
    ))
    .context("design_solution 执行失败")?;

    match outcome {
        LoopStepOutcome::Succeeded(art) => {
            let path = match write_pseudo_artifact(root, art.node, &art.text, &art.libraries) {
                Ok(path) => path,
                Err(e) => {
                    record_artifact_write_failure(&mut store, art.node, "pseudo artifact", &e)?;
                    return Err(e);
                }
            };
            let libs_note = if art.libraries.is_empty() {
                String::new()
            } else {
                format!("；选用库 [{}]", art.libraries.join(", "))
            };
            println!(
                "已创建伪代码 {}（addresses→ {target}）；正文写入 {}{libs_note}",
                art.node,
                path.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        LoopStepOutcome::Failed { raw_llm, error } => {
            eprintln!("design 失败：{error}");
            eprintln!("已保留兜底节点 {raw_llm}（attempted→ {target}）；未伪造成功。");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// 由 active context 渲染 `design_solution` prompt。
///
/// 内置模板 + 受控上下文键：渲染失败属内部编程错误（模板 / 上下文不匹配），以 `expect`
/// 暴露为不变量违反，而非可恢复错误（与 `CliImplementPrompts` 渲染、`step_schema` 同构）。
fn render_design_request(
    prompt: &PromptRegistry,
    ctx: &ActiveContext,
    model: &str,
    registry: &LibraryRegistry,
) -> CompletionRequest {
    let objective = ctx
        .bound_objectives
        .first()
        .map(|o| format!("{}：{}", o.title, o.description))
        .unwrap_or_else(|| "（无绑定目标）".to_string());
    let constraints: Vec<String> = ctx
        .bound_constraints
        .iter()
        .map(|c| c.statement.clone())
        .collect();
    let acceptance: Vec<String> = ctx
        .bound_acceptance_criteria
        .iter()
        .map(|a| a.statement.clone())
        .collect();

    let rendered = prompt
        .render(
            "design_solution",
            serde_json::json!({
                "objective": objective,
                "constraints": constraints,
                "acceptance_criteria": acceptance,
                // 起步阶段：graph Objective ↔ 项目 action 链接尚未建模（见 dev_checklist_v1 结转项），
                // 故 context_files 诚实留空，不臆造文件。
                "context_files": Vec::<String>::new(),
                "stdlib_catalog": registry.catalog(),
            }),
        )
        .expect("渲染 design_solution 模板失败（内置模板 + 受控上下文，属内部不变量）");

    let mut req = CompletionRequest::new(model, rendered);
    req.system = Some(sophia_prompt::design_system_prompt());
    req
}

/// 把 `.pseudo` 正文落盘到 artifacts 目录。同时把 design 阶段所选标准库写入伴生
/// `<node>.libs`（每行一个库名）——graph design→implement 跨两次 CLI 调用，所选库须随伪代码
/// 一起持久化，否则 implement 阶段拿不到（见 docs/stdlib_implementation.md §二）。空集不写
/// `.libs` 文件（读取侧缺文件即视为无库，向后兼容旧伪代码产物）。
fn write_pseudo_artifact(
    root: &Path,
    node: NodeId,
    text: &str,
    libraries: &[String],
) -> Result<PathBuf> {
    let dir = artifacts_dir(root);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("创建 artifacts 目录 {} 失败", dir.display()))?;
    let path = dir.join(format!("{}.pseudo", node.as_string()));
    std::fs::write(&path, text).with_context(|| format!("写入 {} 失败", path.display()))?;
    if !libraries.is_empty() {
        let libs_path = dir.join(format!("{}.libs", node.as_string()));
        std::fs::write(&libs_path, libraries.join("\n"))
            .with_context(|| format!("写入 {} 失败", libs_path.display()))?;
    }
    Ok(path)
}

/// 读取伪代码节点的伴生 `.libs`（design 阶段所选标准库）；缺文件视为无库（向后兼容）。
fn read_pseudo_libraries(root: &Path, node: NodeId) -> Vec<String> {
    let libs_path = artifacts_dir(root).join(format!("{}.libs", node.as_string()));
    std::fs::read_to_string(&libs_path)
        .ok()
        .map(|s| {
            s.lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn validate_library_refs(libraries: &[String], registry: &LibraryRegistry) -> Result<()> {
    for lib in libraries {
        if registry.prompt_asset(lib).is_none() {
            anyhow::bail!(
                "伪代码引用了当前项目库注册表中不存在的库 `{lib}`；请重新运行 graph design"
            );
        }
    }
    Ok(())
}

/// 构造 LLM 后端（OpenAI 兼容 / Ollama）。
fn build_client(
    model: &str,
    mode: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<HttpLlmClient> {
    let _ = model; // model 随每次请求传入；后端构造只需 mode / endpoint / key。
    let key = api_key
        .map(|s| s.to_string())
        .or_else(|| std::env::var("SOPHIA_LLM_API_KEY").ok());

    let mut config = match mode {
        "openai" => {
            let mut c = BackendConfig::openai("");
            c.api_key = key;
            c.openai_response_format = env_truthy("SOPHIA_LLM_OPENAI_RESPONSE_FORMAT");
            c
        }
        "ollama" => {
            let mut c = BackendConfig::ollama();
            c.api_key = key;
            c
        }
        other => anyhow::bail!("不支持的后端模式 `{other}`（支持 openai / ollama）"),
    };
    if let Some(url) = base_url {
        config.base_url = url.to_string();
    }
    HttpLlmClient::new(config).context("构造 LLM 后端失败")
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// 在一次性 current-thread tokio 运行时上阻塞执行一个 future（CLI 协调层异步边界）。
fn tokio_block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("构造 tokio 运行时")
        .block_on(fut)
}

/// `sophia graph implement-loop <NodeId> --pseudo <PseudoId>`：实现伪代码并在预算内修复。
///
/// implement → 注入的确定性 code_check（桥接 `tools/check`）→ repair 收敛循环
/// （见 docs/language_design.md 10.9）。通过 code_check 则把候选文件正文落盘到
/// `sophia-runs/graph/artifacts/`（未物化，物化是后续 select/materialize 的显式步骤）。
#[allow(clippy::too_many_arguments)]
pub fn implement_loop(
    root: &Path,
    node: &str,
    pseudo: &str,
    max_repairs: u32,
    model: &str,
    mode: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<ExitCode> {
    let target = parse_node(node)?;
    let pseudo_id = parse_node(pseudo)?;
    let mut store = open_store(root)?;
    expect_target_domain(&store, target, "implement-loop")?;
    if store.role_of(pseudo_id) != Some(NodeRole::Pseudocode) {
        anyhow::bail!("{pseudo_id} 不是 Pseudocode 节点");
    }

    let client = build_client(model, mode, base_url, api_key)?;
    let prompt = PromptRegistry::new();
    let registry = library_registry(root)?;
    // 读取伪代码正文（图节点不存正文，4.4.3；implement 提供者需要它）。
    let pseudo_path = artifacts_dir(root).join(format!("{}.pseudo", pseudo_id.as_string()));
    let pseudocode_text = std::fs::read_to_string(&pseudo_path).with_context(|| {
        format!(
            "读取伪代码正文 {} 失败（先运行 `graph design`？）",
            pseudo_path.display()
        )
    })?;

    let config = ImplementLoopConfig {
        max_repair_attempts: max_repairs,
        structured: StructuredConfig::default(),
    };

    // design 阶段所选标准库（伴生 `.libs`，缺文件视为无库）——按需注入对应库资产（S2）。
    let libraries = read_pseudo_libraries(root, pseudo_id);
    validate_library_refs(&libraries, &registry)?;

    // prompt 提供者：implement / repair 请求在调用时刻据 active context 渲染（§8.4）。
    let prompts = CliImplementPrompts {
        prompt: &prompt,
        registry: &registry,
        model: model.to_string(),
    };

    let outcome = tokio_block_on(run_implement_loop(
        &mut store,
        &client,
        &prompts,
        &config,
        target,
        pseudo_id,
        &pseudocode_text,
        // design 阶段 LLM 所选标准库，自伴生 `.libs` 读回（graph design→implement 跨两次 CLI 调用，
        // 经 sidecar 持久化贯通；见 docs/stdlib_implementation.md §二）。
        &libraries,
        code_check,
    ))
    .context("implement-loop 执行失败")?;

    match outcome {
        ImplementLoopOutcome::Passed {
            code,
            files,
            attempts,
        } => {
            let written = match write_code_artifacts(root, code, &files) {
                Ok(written) => written,
                Err(e) => {
                    record_artifact_write_failure(&mut store, code, "code artifact", &e)?;
                    return Err(e);
                }
            };
            println!(
                "implement-loop 通过（{attempts} 次尝试）：候选 {code}，{} 个文件写入 {}",
                written.len(),
                artifacts_dir(root).join(code.as_string()).display()
            );
            for p in &written {
                println!("    {p}");
            }
            println!("（候选未物化；后续 select / materialize 才写入 domains/）");
            Ok(ExitCode::SUCCESS)
        }
        ImplementLoopOutcome::BudgetExhausted {
            last_code,
            last_diagnostic,
            attempts,
        } => {
            eprintln!(
                "implement-loop 预算耗尽（{attempts} 次尝试仍未通过 code_check）：\
                 最后候选 {last_code}，诊断 {last_diagnostic}。"
            );
            Ok(ExitCode::FAILURE)
        }
        ImplementLoopOutcome::Failed { raw_llm, error } => {
            eprintln!("implement-loop 失败：{error}");
            eprintln!("已保留兜底节点 {raw_llm}（attempted→ {target}）；未伪造成功。");
            Ok(ExitCode::FAILURE)
        }
    }
}

// 注入的确定性 code_check（候选文件 → CodeCheck 诊断）统一在 `sophia_engine::code_check`
// （workflow 层——code_check 是工作流的确定性 gate，与 `run_implement_loop` 同层；此前
// CLI / e2e / benchmark 三处重复，已收敛）。

/// CLI implement-loop 的 prompt 提供者（实现 engine 的 `StepPrompts`，见架构 §8.4）。
///
/// 本命令只走 implement / repair（不含 decision / design——伪代码已由先前 `graph design`
/// 产出并落盘）。两步请求都在调用时刻据 active context 渲染；语法基线由共享 prompt 资产注入
/// （§8.3，决断性事实、无任务答案）。
struct CliImplementPrompts<'a> {
    prompt: &'a PromptRegistry,
    registry: &'a LibraryRegistry,
    model: String,
}

impl CliImplementPrompts<'_> {
    /// implement / repair 共用的 system preamble：委派 `sophia_prompt::implement_system_prompt`
    /// （常驻语法基线 + 按需标准库资产 + 输出形状，三处 implement 步骤的单一文案来源）。
    ///
    /// `libraries` 是 design 阶段所选标准库（自伴生 `.libs` 读回，见 `implement_loop`）；其完整资产
    /// 文本由库注册表算得（`registry.preamble`，见 docs/stdlib_design.md）。
    fn system(&self, libraries: &[String]) -> String {
        let lib_refs: Vec<&str> = libraries.iter().map(|s| s.as_str()).collect();
        let stdlib_block = self.registry.preamble(&lib_refs);
        sophia_prompt::implement_system_prompt(&stdlib_block)
    }
}

impl sophia_engine::StepPrompts for CliImplementPrompts<'_> {
    fn decision(
        &self,
        _ctx: &ActiveContext,
        _focus: NodeId,
        _progress: sophia_engine::GoalProgress,
    ) -> CompletionRequest {
        // 本命令不发起 decision；调度器场景才会用到（CLI 暂未接入总调度器）。
        unreachable!("implement-loop 命令不发起 decision 步骤")
    }

    fn design(&self, _ctx: &ActiveContext, _focus: NodeId) -> CompletionRequest {
        unreachable!("implement-loop 命令不发起 design 步骤")
    }

    fn decompose(&self, _ctx: &ActiveContext, _focus: NodeId) -> CompletionRequest {
        unreachable!("implement-loop 命令不发起 decompose 步骤")
    }

    fn revise(
        &self,
        _ctx: &ActiveContext,
        _focus: NodeId,
        _pseudocode: &str,
        _diagnostics: &[DiagnosticItem],
    ) -> CompletionRequest {
        unreachable!("implement-loop 命令不发起 revise 步骤")
    }

    fn implement(
        &self,
        ctx: &ActiveContext,
        _focus: NodeId,
        pseudocode: &str,
        libraries: &[String],
    ) -> CompletionRequest {
        let constraints: Vec<String> = ctx
            .bound_constraints
            .iter()
            .map(|c| c.statement.clone())
            .collect();
        let rendered = self
            .prompt
            .render(
                "implement_design",
                serde_json::json!({
                    "pseudocode": pseudocode,
                    "context_files": Vec::<String>::new(),
                    "constraints": constraints,
                }),
            )
            .expect("渲染 implement_design 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(self.system(libraries));
        req
    }

    fn repair(
        &self,
        _ctx: &ActiveContext,
        _focus: NodeId,
        files: &[(String, String)],
        diagnostics: &[DiagnosticItem],
        libraries: &[String],
    ) -> CompletionRequest {
        // repair 模板的 files 槽喂完整正文（默认模板只列路径，正文要显式给）。
        let file_blocks: Vec<String> = files
            .iter()
            .map(|(path, content)| format!("{path}:\n{content}"))
            .collect();
        let rendered = self
            .prompt
            .render(
                "repair_code",
                serde_json::json!({
                    "files": file_blocks,
                    "diagnostics": diagnostics,
                }),
            )
            .expect("渲染 repair_code 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(self.system(libraries));
        req
    }
}

/// 把候选文件正文落盘到 `artifacts/<CodeId>/<相对路径>`（未物化）。
pub(super) fn write_code_artifacts(
    root: &Path,
    code: NodeId,
    files: &[(String, String)],
) -> Result<Vec<String>> {
    let base = artifacts_dir(root).join(code.as_string());
    let mut written = Vec::new();
    for (rel, content) in files {
        // 与 code_check / materialize 的边界契约一致。
        sophia_engine::validate_candidate_path(rel).map_err(anyhow::Error::msg)?;
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录 {} 失败", parent.display()))?;
        }
        std::fs::write(&path, content).with_context(|| format!("写入 {} 失败", path.display()))?;
        written.push(rel.clone());
    }
    Ok(written)
}

fn record_artifact_write_failure(
    store: &mut GraphStore,
    target: NodeId,
    artifact_kind: &str,
    error: &anyhow::Error,
) -> Result<()> {
    let payload = DiagnosticPayload {
        kind: DiagnosticKind::ArtifactWrite,
        ok: false,
        diagnostics: vec![DiagnosticItem {
            code: "ARTIFACT-WRITE".to_string(),
            severity: DiagnosticSeverity::Error,
            problem: format!("{artifact_kind} 写入失败：{error}"),
            location: Some(target.as_string()),
        }],
    };
    let diagnostic = store
        .as_deterministic()
        .diagnostic("artifact write failed", payload)
        .context("创建 ArtifactWrite DiagnosticNode 失败")?;
    store
        .append_edge(diagnostic, target, EdgeKind::Checks)
        .context("连接 artifact-write checks→ 边失败")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sophia_engine::StepPrompts;
    use sophia_graph_db::{CodePayload, EdgeKind, NodePayload};
    use sophia_hir::LibraryContent;

    fn temp_root(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sophia_graph_cmd_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn code_check_passes_clean_candidate() {
        // 一个语义干净的 action 候选应通过 code_check（ok=true）。
        let files = vec![(
            "MathDomain/actions/AddOne.sophia".to_string(),
            "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }"
                .to_string(),
        )];
        let payload = code_check(&files);
        assert_eq!(payload.kind, sophia_graph_db::DiagnosticKind::CodeCheck);
        assert!(payload.ok, "干净候选应通过：{:?}", payload.diagnostics);
    }

    #[test]
    fn code_check_flags_syntax_error() {
        // 语法错误候选应 ok=false，且带 SYNTAX 诊断。
        let files = vec![(
            "D/actions/Bad.sophia".to_string(),
            "action Bad { input { n: Int } output { r: Int } body { return ".to_string(),
        )];
        let payload = code_check(&files);
        assert!(!payload.ok, "语法错误应不通过");
        assert!(payload.diagnostics.iter().any(|d| d.code == "SYNTAX"));
    }

    #[test]
    fn code_check_flags_invalid_candidate_path_before_parse() {
        let files = vec![(
            "Bad.sophia".to_string(),
            "action Bad { input { n: Int } output { r: Int } body { return n } }".to_string(),
        )];
        let payload = code_check(&files);
        assert!(!payload.ok);
        assert_eq!(payload.diagnostics[0].code, "PATH");
    }

    #[test]
    fn pseudo_libraries_sidecar_roundtrips() {
        // S2 graph 路径缺陷修复：design 阶段所选库须随伪代码经伴生 `.libs` 持久化，
        // implement 阶段读回——否则 graph design→implement 跨命令丢失库选择。
        let root = temp_root("libs_rt");
        let node = NodeId::parse("N0007").unwrap();

        // 写入：伪代码正文 + 所选库 ["http"]。
        write_pseudo_artifact(&root, node, "# Purpose\n...", &["http".to_string()]).unwrap();
        // 读回：应取得 ["http"]。
        assert_eq!(read_pseudo_libraries(&root, node), vec!["http".to_string()]);

        // 空集：不写 `.libs`，读回为空（向后兼容旧产物）。
        let node2 = NodeId::parse("N0008").unwrap();
        write_pseudo_artifact(&root, node2, "# Purpose\n...", &[]).unwrap();
        assert!(read_pseudo_libraries(&root, node2).is_empty());
        assert!(
            !artifacts_dir(&root)
                .join(format!("{}.libs", node2.as_string()))
                .exists(),
            "空库集不应写 .libs 文件"
        );

        // 完全缺文件（旧伪代码产物）：读回为空，不 panic。
        let node3 = NodeId::parse("N0009").unwrap();
        assert!(read_pseudo_libraries(&root, node3).is_empty());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn artifact_write_failure_is_recorded_on_target_node() {
        let root = temp_root("artifact_write_diag");
        let mut store = open_store(&root).unwrap();
        let code = store
            .as_llm()
            .code(
                "code",
                CodePayload {
                    files: vec!["D/actions/A.sophia".to_string()],
                },
            )
            .unwrap();

        record_artifact_write_failure(
            &mut store,
            code,
            "code artifact",
            &anyhow::anyhow!("disk full"),
        )
        .unwrap();

        let diagnostic = store
            .nodes()
            .find(|node| {
                matches!(
                    &node.payload,
                    NodePayload::Diagnostic(p)
                        if p.kind == DiagnosticKind::ArtifactWrite
                            && !p.ok
                            && p.diagnostics.iter().any(|d| d.code == "ARTIFACT-WRITE")
                )
            })
            .map(|node| node.meta.id)
            .expect("应记录 ArtifactWrite 诊断");
        assert!(store.has_edge(diagnostic, code, EdgeKind::Checks));
    }

    #[test]
    fn unknown_pseudo_library_is_rejected() {
        let registry = sophia_stdlib::standard_registry();
        let libs = vec!["missing-lib".to_string()];

        let err = validate_library_refs(&libs, &registry).unwrap_err();
        assert!(
            err.to_string().contains("不存在的库 `missing-lib`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn implement_prompt_uses_injected_project_registry() {
        let registry = extra_library_registry();
        let prompt = PromptRegistry::new();
        let prompts = CliImplementPrompts {
            prompt: &prompt,
            registry: &registry,
            model: "mock".to_string(),
        };
        let store = GraphStore::open_in_memory().unwrap();
        let ctx = sophia_graph_db::derive_active_context(&store);

        let req = prompts.implement(
            &ctx,
            NodeId::parse("N0001").unwrap(),
            "# Purpose\n...",
            &["extra".to_string()],
        );

        assert!(
            req.system
                .expect("system prompt")
                .contains("PROJECT EXTRA ASSET TOKEN"),
            "implement system prompt 应注入传入 registry 的项目库资产"
        );
    }

    #[test]
    fn design_prompt_uses_injected_project_registry_catalog() {
        let registry = extra_library_registry();
        let prompt = PromptRegistry::new();
        let store = GraphStore::open_in_memory().unwrap();
        let ctx = sophia_graph_db::derive_active_context(&store);

        let req = render_design_request(&prompt, &ctx, "mock", &registry);

        assert!(
            req.prompt.contains("`extra`") && req.prompt.contains("项目三方库"),
            "design prompt 应注入传入 registry 的项目库目录"
        );
    }

    fn extra_library_registry() -> LibraryRegistry {
        LibraryRegistry::build(vec![LibraryContent {
            dir_name: "extra".into(),
            manifest_toml: r#"
[library]
name = "extra"
summary = "项目三方库"
abi_version = 1

[prompt]
asset = "extra.md"
"#
            .into(),
            asset_text: "PROJECT EXTRA ASSET TOKEN".into(),
            sophia_sources: vec![],
            host_wasm: None,
        }])
        .unwrap()
    }

    #[test]
    fn code_check_flags_semantic_error() {
        // print 但未声明 Console.Write effect → 语义诊断。
        let files = vec![(
            "D/actions/Bad.sophia".to_string(),
            "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }"
                .to_string(),
        )];
        let payload = code_check(&files);
        assert!(!payload.ok, "未声明 effect 应不通过");
        assert!(
            payload
                .diagnostics
                .iter()
                .any(|d| d.code.contains("EFFECT")),
            "应含 effect 诊断：{:?}",
            payload.diagnostics
        );
    }

    #[test]
    fn domain_of_path_takes_first_segment() {
        // domain_of_path 现归 sophia_engine（H1 收敛）；此处验证 CLI 路径用法符合预期。
        assert_eq!(
            sophia_engine::domain_of_path("TodoDomain/entities/Todo.sophia"),
            "TodoDomain"
        );
        assert_eq!(sophia_engine::domain_of_path("D/A.sophia"), "D");
    }

    /// constraint audit verifier 执行器闭环：runtime 跑 hidden case → 映射为
    /// VerifierOutcome → audit 判定。证明声明了可执行 verifier 的 invariant 不再硬阻断，
    /// 而是由真实执行结果驱动 regression gate（pass / fail 都不伪造）。
    #[test]
    fn hidden_case_runner_closes_audit_gate() {
        use sophia_runtime::{run_hidden_case, ExpectedOutcome, HiddenCase, Value};

        // 一个 invariant：AddOne(41) 必须返回 42（hidden case 驱动 regression gate）。
        let src = "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }";
        let ast = sophia_syntax::parse_ast(src).unwrap();
        let index = sophia_hir::AsgIndex::build(
            vec![sophia_hir::IndexInput {
                domain: "D",
                path: "domains/D/actions/AddOne.sophia",
                ast: &ast,
            }],
            &sophia_hir::LibraryRegistry::empty(),
        )
        .unwrap();
        let refs = vec![&ast];
        let model = sophia_semantic::analyze_program(&refs, &index).model;

        // 执行侧（runtime）：真正跑 hidden case。
        let case = HiddenCase {
            verifier_ref: "hc:add_one".into(),
            entry_action: "AddOne".into(),
            args: vec![Value::Int(41)],
            expected: ExpectedOutcome::Returns(Value::Int(42)),
        };
        let result = run_hidden_case(&model, &refs, &case);
        assert!(result.passed, "hidden case 应通过：{}", result.detail);

        // 协调层：映射为 VerifierOutcome 注入审计判定（tools/audit）。形状一一对应，零损耗。
        let to_outcome = |r: &sophia_runtime::VerificationResult| sophia_audit::VerifierOutcome {
            verifier_ref: r.verifier_ref.clone(),
            passed: r.passed,
            detail: r.detail.clone(),
        };
        let outcome = to_outcome(&result);
        let constraint = sophia_audit::Constraint {
            id: "N0001".into(),
            kind: sophia_audit::ConstraintKind::Invariant,
            statement: "AddOne 自增 1".into(),
            verifier: Some((sophia_audit::VerifierKind::HiddenCase, "hc:add_one".into())),
        };
        let report = sophia_audit::audit_constraints(&[constraint], &[outcome]).unwrap();
        assert!(report.ok(), "verifier 通过 → regression gate 应放行");

        // 反例：期望错误值 → 执行判 fail → gate 阻断（不伪造通过）。
        let bad_case = HiddenCase {
            verifier_ref: "hc:add_one".into(),
            entry_action: "AddOne".into(),
            args: vec![Value::Int(41)],
            expected: ExpectedOutcome::Returns(Value::Int(999)),
        };
        let bad = to_outcome(&run_hidden_case(&model, &refs, &bad_case));
        let constraint2 = sophia_audit::Constraint {
            id: "N0002".into(),
            kind: sophia_audit::ConstraintKind::Invariant,
            statement: "AddOne 自增 1".into(),
            verifier: Some((sophia_audit::VerifierKind::HiddenCase, "hc:add_one".into())),
        };
        let report2 = sophia_audit::audit_constraints(&[constraint2], &[bad]).unwrap();
        assert!(!report2.ok(), "verifier 失败 → regression gate 应阻断");
    }
}
