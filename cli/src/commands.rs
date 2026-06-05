//! CLI 子命令实现。
//!
//! 见 docs/engineering_architecture.md 第九节。每个命令把已完成的库层
//! （syntax / hir / semantic / runtime）组装为可执行流程，IO 与呈现集中于此。

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sophia_hir::{
    action_context, resolve_item, resolve_program, task_context, AsgIndex, ContextClosure,
    LibraryRegistry, LibrarySources, ProgramInput,
};
use sophia_semantic::{analyze_one_callable, analyze_program, SemanticModel};
use sophia_syntax::{Ast, Block, ElseBranch, Expr, ExprId, Item, Span, Stmt};

/// CLI 用的**完整**库注册表：标准库 + 以**项目根**发现的三方库（启动时一次性扫描合并后冻结，
/// 见 docs/stdlib_design.md §五.1）。三方库根 = `<root>/sophia_libs/` + `$SOPHIA_LIB_PATH`；发现
/// 失败（清单非法 / `abi_version` 不符 / family/op/domain 冲突）诚实报错退出，不静默跳过 / 部分
/// 加载。确定性子门禁（`tools/check::check_program` 等）仍只用 `standard_registry`——三方发现是
/// 协调层启动行为，不进核心确定性门禁。
pub(crate) fn library_registry(root: &Path) -> Result<LibraryRegistry> {
    sophia_stdlib::full_registry_for(root).map_err(|e| anyhow::anyhow!("三方库发现失败：{e}"))
}

/// 构建命令所需的库上下文：完整注册表（标准库 + 三方发现）+ 库随附 Sophia 源码（owned AST）。
///
/// 调用方在自身作用域持有返回值（库 AST 的所有者），再把 [`LibrarySources::program_inputs`] 并入
/// 用户 inputs（供 resolve / index）、[`LibrarySources::asts`] 并入用户 AST（供 analyze / run）。
/// 纯 Sophia 库节点（如 `SophiaDigest`）须经此并入才可解析 / 执行；effect-op 库（标准库 / 三方
/// WASM）无源码节点，`LibrarySources` 为空、零开销（见 docs/stdlib_design.md §二.1）。
fn library_context(root: &Path) -> Result<(LibraryRegistry, LibrarySources)> {
    let registry = library_registry(root)?;
    let lib_srcs = LibrarySources::from_registry(&registry)
        .map_err(|e| anyhow::anyhow!("解析库随附 Sophia 源码失败：{e}"))?;
    Ok((registry, lib_srcs))
}

use crate::project::Project;
use crate::render;

const BUILD_MANIFEST_NAME: &str = "program.sophia-build.json";

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn registry_fingerprint(registry: &LibraryRegistry) -> String {
    let mut h = Sha256::new();
    for lib in registry.lib_names() {
        h.update(b"lib\0");
        h.update(lib.as_bytes());
        if let Some(asset) = registry.prompt_asset(lib) {
            h.update(b"\0summary\0");
            h.update(asset.summary.as_bytes());
            h.update(b"\0asset\0");
            h.update(asset.asset_text.as_bytes());
        }
        if let Some(bytes) = registry.host_wasm(lib) {
            h.update(b"\0host_wasm\0");
            h.update(sha256_hex(bytes).as_bytes());
        }
    }
    for src in registry.sophia_sources() {
        h.update(b"source\0");
        h.update(src.lib.as_bytes());
        h.update(b"\0");
        h.update(src.domain.as_bytes());
        h.update(b"\0");
        h.update(src.path.as_bytes());
        h.update(b"\0");
        h.update(src.source.as_bytes());
    }
    for op in registry.ops() {
        h.update(b"op\0");
        h.update(op.lib.as_bytes());
        h.update(b"\0");
        h.update(op.family.as_bytes());
        h.update(b"\0");
        h.update(op.op.as_bytes());
        h.update(b"\0");
        h.update(op.host_fn.as_bytes());
        h.update(b"\0");
        h.update(if op.effectful {
            b"effectful".as_slice()
        } else {
            b"pure".as_slice()
        });
        for p in &op.params {
            h.update(b"\0param\0");
            h.update(type_desc_string(p).as_bytes());
        }
        h.update(b"\0returns\0");
        h.update(type_desc_string(&op.returns).as_bytes());
    }
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn type_desc_string(desc: &sophia_library::TypeDesc) -> String {
    match desc {
        sophia_library::TypeDesc::Scalar(s) => s.as_str().to_string(),
        sophia_library::TypeDesc::Intent { intent, inner } => {
            format!("{intent}<{}>", inner.as_str())
        }
    }
}

fn collect_used_library_ops(
    asts: &[&Ast],
    registry: &LibraryRegistry,
) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for ast in asts {
        for item in &ast.items {
            if let Item::Action(c) | Item::Transition(c) = item {
                if let Some(body) = &c.body {
                    collect_used_ops_block(body, ast, registry, &mut out);
                }
            }
        }
    }
    out
}

fn collect_used_ops_block(
    block: &Block,
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    for stmt in &block.stmts {
        collect_used_ops_stmt(stmt, ast, registry, out);
    }
}

fn collect_used_ops_stmt(
    stmt: &Stmt,
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Set { value, .. }
        | Stmt::Return { value, .. }
        | Stmt::Raise { value, .. }
        | Stmt::Print { value, .. }
        | Stmt::Expr { value, .. } => collect_used_ops_expr(*value, ast, registry, out),
        Stmt::If {
            condition,
            consequence,
            alternative,
            ..
        } => {
            collect_used_ops_expr(*condition, ast, registry, out);
            collect_used_ops_block(consequence, ast, registry, out);
            match alternative {
                Some(ElseBranch::Block(b)) => collect_used_ops_block(b, ast, registry, out),
                Some(ElseBranch::If(s)) => collect_used_ops_stmt(s, ast, registry, out),
                None => {}
            }
        }
        Stmt::Match { subject, arms, .. } => {
            collect_used_ops_expr(*subject, ast, registry, out);
            for arm in arms {
                collect_used_ops_block(&arm.body, ast, registry, out);
            }
        }
        Stmt::Repeat { count, body, .. } => {
            collect_used_ops_expr(*count, ast, registry, out);
            collect_used_ops_block(body, ast, registry, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_used_ops_expr(*condition, ast, registry, out);
            collect_used_ops_block(body, ast, registry, out);
        }
    }
}

fn collect_used_ops_expr(
    id: ExprId,
    ast: &Ast,
    registry: &LibraryRegistry,
    out: &mut BTreeSet<(String, String)>,
) {
    match ast.expr(id) {
        Expr::MethodCall {
            base, method, args, ..
        } => {
            if let Expr::Ident(root) = ast.expr(*base) {
                if registry.op(&root.text, &method.text).is_some() {
                    out.insert((root.text.clone(), method.text.clone()));
                }
            }
            collect_used_ops_expr(*base, ast, registry, out);
            for &arg in args {
                collect_used_ops_expr(arg, ast, registry, out);
            }
        }
        Expr::List { items, .. } => {
            for &item in items {
                collect_used_ops_expr(item, ast, registry, out);
            }
        }
        Expr::Field { base, .. } => collect_used_ops_expr(*base, ast, registry, out),
        Expr::Call { args, .. } => {
            for &arg in args {
                collect_used_ops_expr(arg, ast, registry, out);
            }
        }
        Expr::Construct { fields, .. } => {
            for field in fields {
                collect_used_ops_expr(field.value, ast, registry, out);
            }
        }
        Expr::Not { operand, .. } | Expr::Neg { operand, .. } => {
            collect_used_ops_expr(*operand, ast, registry, out)
        }
        Expr::Binary { left, right, .. } => {
            collect_used_ops_expr(*left, ast, registry, out);
            collect_used_ops_expr(*right, ast, registry, out);
        }
        Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Null { .. }
        | Expr::Ident(_)
        | Expr::Str(_) => {}
    }
}

fn write_build_bundle(
    out_dir: &Path,
    wasm_bytes: &[u8],
    registry: &LibraryRegistry,
    asts: &[&Ast],
    model: &SemanticModel,
) -> Result<()> {
    let used_ops = collect_used_library_ops(asts, registry);
    let mut imports = Vec::new();
    for (family, op) in used_ops {
        let contract = registry
            .op(&family, &op)
            .ok_or_else(|| anyhow::anyhow!("manifest 生成时发现未知库 op `{family}.{op}`"))?;
        let params: Vec<String> = contract.params.iter().map(type_desc_string).collect();
        let returns = type_desc_string(&contract.returns);
        let mut entry = serde_json::json!({
            "library": contract.lib,
            "family": contract.family,
            "op": contract.op,
            "host_fn": contract.host_fn,
            "module": format!("sophia_lib:{}", contract.lib),
            "name": contract.host_fn,
            "params": params,
            "returns": returns,
            "provider": if registry.host_wasm(&contract.lib).is_some() { "wasm" } else { "native" },
        });
        if let Some(host_bytes) = registry.host_wasm(&contract.lib) {
            let rel = format!("hosts/{}/host.wasm", contract.lib);
            let host_path = out_dir.join(&rel);
            if let Some(parent) = host_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("创建 {} 失败", parent.display()))?;
            }
            std::fs::write(&host_path, host_bytes)
                .with_context(|| format!("写入 {} 失败", host_path.display()))?;
            entry["host_wasm"] = serde_json::Value::String(rel);
            entry["host_wasm_sha256"] = serde_json::Value::String(sha256_hex(host_bytes));
        }
        imports.push(entry);
    }

    let all_import_names: Vec<String> = imports
        .iter()
        .map(|v| {
            format!(
                "{}.{}",
                v["family"].as_str().unwrap(),
                v["op"].as_str().unwrap()
            )
        })
        .collect();
    let mut exports = Vec::new();
    for (name, decl) in &model.callables {
        if matches!(decl.kind, sophia_syntax::CallableKind::Action) {
            exports.push(serde_json::json!({
                "kind": "action",
                "name": name,
                "reachable_imports": all_import_names,
            }));
        }
    }

    let manifest = serde_json::json!({
        "schema_version": 1,
        "sophia_version": env!("CARGO_PKG_VERSION"),
        "registry_fingerprint": registry_fingerprint(registry),
        "wasm_sha256": sha256_hex(wasm_bytes),
        "imports": imports,
        "exports": exports,
    });
    let raw = serde_json::to_string_pretty(&manifest).context("序列化 build manifest 失败")?;
    let manifest_path = out_dir.join(BUILD_MANIFEST_NAME);
    std::fs::write(&manifest_path, raw)
        .with_context(|| format!("写入 {} 失败", manifest_path.display()))?;
    Ok(())
}

fn validate_build_bundle(root: &Path, wasm_bytes: &[u8], registry: &LibraryRegistry) -> Result<()> {
    let manifest_path = root.join("sophia-runs/build").join(BUILD_MANIFEST_NAME);
    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            eprintln!(
                "WASM build manifest 不存在：{}。请重新运行 `sophia build {}`。",
                manifest_path.display(),
                root.display()
            );
            return Err(anyhow::anyhow!("缺少 WASM build manifest"));
        }
        Err(e) => return Err(e).with_context(|| format!("读取 {} 失败", manifest_path.display())),
    };
    let manifest: serde_json::Value =
        serde_json::from_str(&raw).context("解析 WASM build manifest 失败")?;
    let want_wasm = manifest
        .get("wasm_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("WASM build manifest 缺 wasm_sha256"))?;
    let actual_wasm = sha256_hex(wasm_bytes);
    if want_wasm != actual_wasm {
        return Err(anyhow::anyhow!(
            "WASM artifact hash 与 build manifest 不一致：manifest={want_wasm}, actual={actual_wasm}"
        ));
    }
    let want_registry = manifest
        .get("registry_fingerprint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("WASM build manifest 缺 registry_fingerprint"))?;
    let actual_registry = registry_fingerprint(registry);
    if want_registry != actual_registry {
        return Err(anyhow::anyhow!(
            "当前 registry 与 build manifest 不一致：manifest={want_registry}, actual={actual_registry}；请重新运行 `sophia build`"
        ));
    }
    let imports = manifest
        .get("imports")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("WASM build manifest 缺 imports"))?;
    for import in imports {
        if import.get("provider").and_then(|v| v.as_str()) != Some("wasm") {
            continue;
        }
        let rel = import
            .get("host_wasm")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("WASM provider import 缺 host_wasm"))?;
        let want = import
            .get("host_wasm_sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("WASM provider import 缺 host_wasm_sha256"))?;
        let path = root.join("sophia-runs/build").join(rel);
        let bytes =
            std::fs::read(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
        let actual = sha256_hex(&bytes);
        if want != actual {
            return Err(anyhow::anyhow!(
                "host.wasm hash 与 build manifest 不一致：{} manifest={}, actual={}",
                path.display(),
                want,
                actual
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunBackend {
    Interpreter,
    Wasm,
}

/// `sophia init`：创建标准目录结构与 sophia.toml（5.2）。
pub fn init(dir: &Path, name: Option<&str>) -> Result<ExitCode> {
    let project_name = name
        .map(|s| s.to_string())
        .or_else(|| {
            dir.canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        })
        .unwrap_or_else(|| "sophia_project".to_string());

    // 标准目录（docs/engineering_architecture.md 第五节）。
    for sub in [
        "domains",
        "sophia-runs/generated",
        "sophia-runs/task_closures",
        "sophia-runs/build",
        "sophia-runs/graph",
    ] {
        let p = dir.join(sub);
        std::fs::create_dir_all(&p).with_context(|| format!("创建目录 {} 失败", p.display()))?;
    }

    let toml_path = dir.join("sophia.toml");
    if toml_path.exists() {
        println!("sophia.toml 已存在，跳过。");
    } else {
        std::fs::write(&toml_path, default_sophia_toml(&project_name))
            .with_context(|| format!("写入 {} 失败", toml_path.display()))?;
        println!("已创建 {}", toml_path.display());
    }
    println!("已初始化 Sophia 项目于 {}", dir.display());
    Ok(ExitCode::SUCCESS)
}

/// `sophia.toml` 最小内容（5.2）。
fn default_sophia_toml(name: &str) -> String {
    format!(
        r#"[project]
name = "{name}"
version = "0.1.0"
sophia_version = "0.1"

[source]
domain_root = "domains"
generated_dir = "sophia-runs/generated"

[layout]
strategy = "domain_first"
one_top_level_node_per_file = true
forbid_global_kind_dirs = true

[build]
target = "wasm"
out_dir = "sophia-runs/build"

[check]
require_strip_assist_equivalence = true
forbid_implicit_imports = true
forbid_shadowing = true
require_explicit_cross_domain_boundary = true
"#
    )
}

/// `sophia parse <file>`：解析单文件并报告语法诊断。
pub fn parse(file: &Path) -> Result<ExitCode> {
    let source = std::fs::read_to_string(file)
        .with_context(|| format!("无法读取文件 {}", file.display()))?;
    let tree = sophia_syntax::parse_str(source).context("语法层解析失败")?;
    let diagnostics = tree.errors();
    let path = file.display().to_string();

    if diagnostics.is_empty() {
        println!("OK：{path} 解析通过，无语法错误。");
        return Ok(ExitCode::SUCCESS);
    }
    eprintln!("{path} 存在 {} 处语法诊断：", diagnostics.len());
    for d in &diagnostics {
        eprintln!("{}", render::syntax_line(&path, d));
    }
    Ok(ExitCode::FAILURE)
}

/// `sophia index`：扫描并生成 asg_index.json（17.2）。
pub fn index(root: &Path) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        return Ok(ExitCode::FAILURE);
    }

    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, _diags) = resolve_program(&inputs, &registry).context("构建 ASG index 失败")?;

    let json = index.to_json().context("序列化 ASG index 失败")?;
    let out_path = root.join("sophia-runs/asg_index.json");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out_path, &json)
        .with_context(|| format!("写入 {} 失败", out_path.display()))?;
    println!("已生成 {}（{} 个节点）", out_path.display(), index.len());
    Ok(ExitCode::SUCCESS)
}

/// `sophia graph`：输出 ASG 摘要。
pub fn graph(root: &Path) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        return Ok(ExitCode::FAILURE);
    }
    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, _diags) = resolve_program(&inputs, &registry).context("构建 ASG index 失败")?;

    println!("ASG 摘要（{} 个顶层节点）：", index.len());
    // BTreeMap 已按名排序。
    for (name, info) in index.nodes() {
        println!(
            "  {:<24} {:<12} {}",
            name,
            format!("{:?}", info.kind),
            info.domain
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// 一条带文件归属的诊断（HIR 名称解析 / 语义三层），供 `check` / `repair-context` 复用。
///
/// `code` 是稳定诊断码（如 `CHECK-EFFECT-001` 或 HIR `kind`），`rel_path` 是归属文件，
/// `span` 是 0 基位置（呈现时转 1 基）。`callable` 是诊断所在的 action / transition 名
/// （若诊断源自可调用体），供 repair-context 计算其 action-rooted 语义闭包。
pub struct CollectedDiagnostic {
    pub rel_path: String,
    pub span: Span,
    pub code: String,
    pub message: String,
    pub callable: Option<String>,
}

/// 收集全项目的 HIR + 语义诊断（按文件精确归属，与 `check` / LSP 同口径）。
///
/// 确定性、不调用 LLM。语法层错误不在此处（调用方应先 [`report_syntax_errors`]）；
/// 本函数假设语法已干净，专注名称解析与语义三层诊断。返回的顺序按项目文件字典序
/// （`Project::load` 已排序）、文件内按 item 顺序，保证稳定。
fn collect_diagnostics(
    project: &Project,
    index: &AsgIndex,
    model: &SemanticModel,
    asts: &[&Ast],
) -> Vec<CollectedDiagnostic> {
    let mut out = Vec::new();
    for file in &project.files {
        for item in &file.ast.items {
            for d in resolve_item(item, &file.ast, index, &file.domain) {
                out.push(CollectedDiagnostic {
                    rel_path: file.rel_path.clone(),
                    span: d.span,
                    code: format!("{:?}", d.kind),
                    message: d.message.clone(),
                    callable: None,
                });
            }
            if let Item::Action(c) | Item::Transition(c) = item {
                for d in analyze_one_callable(&c.name.text, model, asts, index) {
                    out.push(CollectedDiagnostic {
                        rel_path: file.rel_path.clone(),
                        span: d.span,
                        code: d.code().to_string(),
                        message: d.message.clone(),
                        callable: Some(c.name.text.clone()),
                    });
                }
            }
        }
    }
    out
}

/// `sophia check`：语法 + 名称解析 + 语义三层检查。
pub fn check(root: &Path) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        return Ok(ExitCode::FAILURE);
    }

    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, _hir_diags) = resolve_program(&inputs, &registry).context("名称解析失败")?;
    // 用户 AST + 库随附 Sophia 源码 AST 同列建模（库节点须建模才能解析其调用 / 被调用）。
    let mut asts: Vec<&Ast> = project.files.iter().map(|f| &f.ast).collect();
    asts.extend(lib_srcs.asts());
    let model = SemanticModel::build(&asts, &index);

    // 按文件精确归属诊断（仅用户文件——库源码诊断不在用户项目 check 范围；避免跨文件 0 基 span
    // 碰撞，与 LSP 一致）。
    let diags = collect_diagnostics(&project, &index, &model, &asts);
    if !diags.is_empty() {
        for d in &diags {
            eprintln!(
                "{}",
                render::diag_line(&d.rel_path, d.span, &d.code, &d.message)
            );
        }
        eprintln!("check 未通过（{} 条诊断）。", diags.len());
        return Ok(ExitCode::FAILURE);
    }

    // strip-assist 等价门禁（sophia.toml require_strip_assist_equivalence；design 5.1）。库上下文
    // 两侧对称（同一 registry + 同一批库源码），差异只能来自用户代码 assist 移除。
    let outcome =
        sophia_check::check_strip_assist_equivalence(&strip_sources(&project), &registry, &index)
            .context("strip-assist 门禁执行失败")?;
    if !outcome.equivalent {
        eprintln!(
            "strip-assist 等价门禁未通过：{}",
            outcome.detail.unwrap_or_default()
        );
        return Ok(ExitCode::FAILURE);
    }

    println!(
        "OK：check 通过（{} 个文件，strip-assist 等价）。",
        project.files.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// 把项目文件投影为 `(domain, path, source)`（strip-assist 门禁需重解析 source）。
fn strip_sources(project: &Project) -> Vec<(String, String, String)> {
    project
        .files
        .iter()
        .map(|f| (f.domain.clone(), f.rel_path.clone(), f.source.clone()))
        .collect()
}

/// `sophia context`：生成 action-rooted 语义上下文或 task closure（§8）。
///
/// 确定性、不调用 LLM：扫描项目 → 构建 ASG index → 从 root 计算语义闭包 → 稳定输出
/// 节点、解释边、文件列表（`--sources` 时附带源码内容）。供 `graph design` 等下游消费。
pub fn context(
    root: &Path,
    action: Option<&str>,
    task: Option<&str>,
    with_sources: bool,
) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        return Ok(ExitCode::FAILURE);
    }
    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, _diags) = resolve_program(&inputs, &registry).context("构建 ASG index 失败")?;
    let mut asts: Vec<&Ast> = project.files.iter().map(|f| &f.ast).collect();
    asts.extend(lib_srcs.asts());

    let closure = match (action, task) {
        (Some(a), None) => action_context(a, &asts, &index).context("计算 action 闭包失败")?,
        (None, Some(t)) => task_context(t, &asts, &index).context("计算 task 闭包失败")?,
        (None, None) => anyhow::bail!("需指定 --action <Name> 或 --task <Name>"),
        (Some(_), Some(_)) => anyhow::bail!("--action 与 --task 互斥"),
    };

    render_closure(&project, &closure, with_sources);
    Ok(ExitCode::SUCCESS)
}

/// 稳定呈现语义闭包（节点 / 边 / 文件，可选源码）。
fn render_closure(project: &Project, closure: &ContextClosure, with_sources: bool) {
    println!("语义闭包（root = {}）：", closure.root);

    println!("节点（{}）：", closure.nodes.len());
    for n in &closure.nodes {
        println!(
            "  {:<24} {:<10} {}",
            n.name,
            format!("{:?}", n.kind),
            n.path
        );
    }

    println!("边（{}）：", closure.edges.len());
    for e in &closure.edges {
        println!("  {} --{}-> {}", e.from, e.kind.name(), e.to);
    }

    println!("文件（{}）：", closure.files.len());
    for f in &closure.files {
        println!("  {f}");
    }

    if with_sources {
        println!("源码：");
        for f in &closure.files {
            let src = project
                .files
                .iter()
                .find(|lf| &lf.rel_path == f)
                .map(|lf| lf.source.as_str())
                .unwrap_or("");
            println!("===== {f} =====");
            println!("{src}");
        }
    }
}

/// `sophia build`：check 通过后 emit WASM artifact（v1 工作流 A，W5）。
///
/// 见 docs/wasm_codegen.md §八。流程：① `check`（含 IR 层 strip-assist）；② **strip-assist artifact
/// 层门禁**——移除 assist 前后 emit 的 `.wasm` 必须逐字节相等（判据 3，`language_design.md` §5.1）；
/// ③ emit `.wasm` 落 `sophia-runs/build/program.wasm`。codegen 未覆盖的构造（`to_text`/`List`，无 v1
/// 演示需求触发）会诚实报 `NotYetImplemented`——如实标注、不伪造产出。
pub fn build(root: &Path) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        return Ok(ExitCode::FAILURE);
    }

    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, _hir_diags) = resolve_program(&inputs, &registry).context("名称解析失败")?;
    let mut asts: Vec<&Ast> = project.files.iter().map(|f| &f.ast).collect();
    asts.extend(lib_srcs.asts());
    let model = SemanticModel::build(&asts, &index);

    let diags = collect_diagnostics(&project, &index, &model, &asts);
    if !diags.is_empty() {
        for d in &diags {
            eprintln!(
                "{}",
                render::diag_line(&d.rel_path, d.span, &d.code, &d.message)
            );
        }
        eprintln!("check 未通过（{} 条诊断）。", diags.len());
        return Ok(ExitCode::FAILURE);
    }

    let sources = strip_sources(&project);
    match sophia_check::check_strip_assist_equivalence(&sources, &registry, &index) {
        Ok(outcome) => {
            if !outcome.equivalent {
                eprintln!(
                    "strip-assist IR 层门禁未通过：{}",
                    outcome.detail.unwrap_or_default()
                );
                return Ok(ExitCode::FAILURE);
            }
        }
        Err(e) => {
            eprintln!("strip-assist IR 层门禁执行失败：{e}");
            return Ok(ExitCode::FAILURE);
        }
    }

    // strip-assist artifact 层门禁（判据 3）：移除 assist 前后 .wasm 逐字节相等。
    match sophia_codegen::check_artifact_strip_equivalence(&sources, &registry) {
        Ok(outcome) => {
            if !outcome.equivalent {
                eprintln!(
                    "strip-assist artifact 层门禁未通过：{}",
                    outcome.detail.unwrap_or_default()
                );
                return Ok(ExitCode::FAILURE);
            }
        }
        Err(sophia_codegen::CodegenError::NotYetImplemented(what)) => {
            eprintln!("build：WASM codegen 尚未覆盖该程序的构造（{what}）。");
            eprintln!("（解释执行仍可用：`sophia run <Action>`。codegen 覆盖面见 docs/wasm_codegen.md §九。）");
            return Ok(ExitCode::FAILURE);
        }
        Err(e) => {
            eprintln!("build：emit 失败：{e}");
            return Ok(ExitCode::FAILURE);
        }
    }

    // emit 最终 artifact（原始版，含 assist 与否字节相同，已由门禁保证）。
    let bytes = match sophia_codegen::emit_from_sources(&sources, &registry, false) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("build：emit 失败：{e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let out_dir = root.join("sophia-runs/build");
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("创建 {} 失败", out_dir.display()))?;
    let out_path = out_dir.join("program.wasm");
    std::fs::write(&out_path, &bytes)
        .with_context(|| format!("写入 {} 失败", out_path.display()))?;
    if let Err(e) = write_build_bundle(&out_dir, &bytes, &registry, &asts, &model) {
        eprintln!("build：写入 WASM build bundle 失败：{e}");
        return Ok(ExitCode::FAILURE);
    }
    println!(
        "build：已 emit WASM artifact {}（{} 字节，strip-assist artifact 等价，manifest 已写入）。",
        out_path.display(),
        bytes.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// `sophia run <action>`：执行某 action。
pub fn run_action(
    root: &Path,
    action: &str,
    raw_args: &[String],
    with_trace: bool,
    backend: RunBackend,
) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        return Ok(ExitCode::FAILURE);
    }

    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, hir_diags) = resolve_program(&inputs, &registry).context("名称解析失败")?;
    if !hir_diags.is_empty() {
        eprintln!("名称解析未通过，无法运行（先 `sophia check`）。");
        return Ok(ExitCode::FAILURE);
    }
    // 用户 AST + 库随附 Sophia 源码 AST 同列（纯 Sophia 库节点须建模 / 可执行）。
    let mut asts: Vec<&Ast> = project.files.iter().map(|f| &f.ast).collect();
    asts.extend(lib_srcs.asts());
    let analysis = analyze_program(&asts, &index);
    if !analysis.diagnostics.is_empty() {
        eprintln!("语义检查未通过，无法运行（先 `sophia check`）。");
        return Ok(ExitCode::FAILURE);
    }

    // 解析实参。
    let args = parse_args(raw_args).context("解析实参失败")?;

    match backend {
        RunBackend::Interpreter => run_interpreter_action(
            root,
            &registry,
            &analysis.model,
            &asts,
            action,
            args,
            with_trace,
        ),
        RunBackend::Wasm => {
            run_wasm_action(root, &registry, &analysis.model, action, args, with_trace)
        }
    }
}

/// 据入口 action 的声明 effect 判断是否需注入标准库 **native** host（真实网络 / 文件 IO）。
///
/// 含 `Http.Get` / `File.Read` / `File.Write` 则需真实 host；纯逻辑 / Console 程序不需（零开销）。
/// 三方 WASM 库 op（如 `WasmHash.Mix`）多为 `effectful=false`，不经声明 effect 体现——其 host 由
/// [`sophia_stdlib::register_wasm_library_hosts`] 据注册表无条件注册（见 `run_interpreter_action`）。
fn needs_native_host(model: &sophia_semantic::SemanticModel, action: &str) -> bool {
    model
        .callables
        .get(action)
        .map(|d| {
            d.declared_effects.iter().any(|e| {
                (e.family == "Http" && e.op == "Get")
                    || (e.family == "File" && (e.op == "Read" || e.op == "Write"))
            })
        })
        .unwrap_or(false)
}

/// 组装 host 注册表并解释执行：
/// - **三方 WASM 库 host**：据注册表 `host.wasm` 经 [`WasmHostFn`] 注册（无三方 WASM 库时 no-op；
///   ABI 不符 / 装载失败诚实 `Err` 阻断，见 docs/stdlib_design.md §五.3）；
/// - **标准库 native host**：仅当入口 action 声明真实 IO effect 时注册（纯逻辑程序零开销）。
///
/// 二者互补：标准库（无 host.wasm）走 native，三方 WASM 库（有 host.wasm）走 `WasmHostFn`。库 host
/// 失败一律物化为硬错误阻断，绝不伪造成功。
fn run_interpreter_action(
    root: &Path,
    registry: &LibraryRegistry,
    model: &sophia_semantic::SemanticModel,
    asts: &[&Ast],
    action: &str,
    args: Vec<sophia_runtime::Value>,
    with_trace: bool,
) -> Result<ExitCode> {
    let mut host = sophia_runtime::HostRegistry::new();
    // 三方 WASM 库 host（注册表持 host.wasm 字节者）。无三方 WASM 库时为 no-op。
    sophia_stdlib::register_wasm_library_hosts(&mut host, registry)
        .map_err(|e| anyhow::anyhow!("注册三方 WASM 库 host 失败：{e}"))?;
    // 标准库 native host（仅当声明真实 IO effect）。
    if needs_native_host(model, action) {
        sophia_stdlib::register_native_hosts(&mut host, root)
            .map_err(|e| anyhow::anyhow!("注册标准库 native host 失败：{e}"))?;
    }
    match sophia_runtime::run_action(model, asts, action, args, &mut host) {
        Ok((outcome, trace)) => present_run(&host.console, outcome, &trace, with_trace),
        Err(e) => {
            eprintln!("运行失败：{e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// 组装 host 注册表并执行 `sophia build` 产出的非浏览器 WASM artifact。
///
/// 前置的项目加载 / 库发现 / 名称解析 / 语义分析与解释器共用 [`run_action`] 的单一路径；本函数只负责
/// 后端特有的 artifact 装载和 runner 调用。
fn run_wasm_action(
    root: &Path,
    registry: &LibraryRegistry,
    model: &sophia_semantic::SemanticModel,
    action: &str,
    args: Vec<sophia_runtime::Value>,
    with_trace: bool,
) -> Result<ExitCode> {
    if with_trace {
        eprintln!("WASM 后端暂不支持 --trace；请使用默认解释器后端。");
        return Ok(ExitCode::FAILURE);
    }

    let wasm_path = root.join("sophia-runs/build/program.wasm");
    let bytes = match std::fs::read(&wasm_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            eprintln!(
                "WASM artifact 不存在：{}。请先运行 `sophia build {}`。",
                wasm_path.display(),
                root.display()
            );
            return Ok(ExitCode::FAILURE);
        }
        Err(e) => {
            return Err(e).with_context(|| format!("读取 {} 失败", wasm_path.display()));
        }
    };

    let runner = match sophia_runtime::WasmProgramRunner::new(&bytes, registry) {
        Ok(runner) => runner,
        Err(e) => {
            eprintln!("WASM 运行器初始化失败：{e}");
            return Ok(ExitCode::FAILURE);
        }
    };
    if let Err(e) = validate_build_bundle(root, &bytes, registry) {
        eprintln!("WASM build bundle 校验失败：{e}");
        return Ok(ExitCode::FAILURE);
    }

    let mut host = sophia_runtime::HostRegistry::new();
    sophia_stdlib::register_wasm_library_hosts(&mut host, registry)
        .map_err(|e| anyhow::anyhow!("注册三方 WASM 库 host 失败：{e}"))?;
    if needs_native_host(model, action) {
        sophia_stdlib::register_native_hosts(&mut host, root)
            .map_err(|e| anyhow::anyhow!("注册标准库 native host 失败：{e}"))?;
    }

    match runner.run(model, action, &args, true, &mut host) {
        Ok(outcome) => present_run(&host.console, outcome, &sophia_runtime::Trace::new(), false),
        Err(e) => {
            eprintln!("WASM 运行失败：{e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// 统一呈现一次 run 的结果：回放 console、可选 trace、返回值 / raise。
fn present_run(
    console: &[String],
    outcome: sophia_runtime::Outcome,
    trace: &sophia_runtime::Trace,
    with_trace: bool,
) -> Result<ExitCode> {
    for line in console {
        println!("{line}");
    }
    if with_trace {
        render::print_trace(trace);
    }
    match outcome {
        sophia_runtime::Outcome::Returned(v) => {
            println!("=> {v}");
            Ok(ExitCode::SUCCESS)
        }
        sophia_runtime::Outcome::Raised(e) => {
            eprintln!("raise {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// `sophia smoke`：一键烟雾测试（init → check → build → run），确定性、不调用 LLM。
///
/// 见 docs/engineering_architecture.md 9.1。把已有确定性命令串成一条最短可信链路，
/// 用于快速验证项目处于可解释执行状态。流程：
/// 1. 确保 `sophia.toml` 存在（缺失则先 `init`，幂等）；
/// 2. `check`（语法 + 名称解析 + 语义三层 + strip-assist 等价）；任一失败即止；
/// 3. `build`（emit WASM artifact + strip-assist artifact 层门禁）；
/// 4. 若提供 `--action`，则 `run` 它（带可选 `--arg`）；未提供则跳过运行步骤。
///
/// 任一步失败即以失败退出码返回（忠实反映，不伪造通过）。
pub fn smoke(
    root: &Path,
    action: Option<&str>,
    raw_args: &[String],
    backend: RunBackend,
) -> Result<ExitCode> {
    println!("== smoke：init → check → build → run ==");

    // 步骤 1：确保项目骨架存在（init 幂等）。
    if root.join("sophia.toml").exists() {
        println!("[1/4] init：sophia.toml 已存在，跳过。");
    } else {
        println!("[1/4] init：创建项目骨架……");
        if init(root, None)? != ExitCode::SUCCESS {
            eprintln!("smoke 中止：init 失败。");
            return Ok(ExitCode::FAILURE);
        }
    }

    // 步骤 2：check。
    println!("[2/4] check……");
    if check(root)? != ExitCode::SUCCESS {
        eprintln!("smoke 中止：check 未通过。");
        return Ok(ExitCode::FAILURE);
    }

    // 步骤 3：build（emit WASM artifact；内部复核 check + strip-assist artifact 门禁）。
    println!("[3/4] build……");
    if build(root)? != ExitCode::SUCCESS {
        eprintln!("smoke 中止：build 未通过。");
        return Ok(ExitCode::FAILURE);
    }

    // 步骤 4：run（仅当指定了 action）。
    match action {
        Some(name) => {
            println!("[4/4] run {name}……");
            if run_action(root, name, raw_args, false, backend)? != ExitCode::SUCCESS {
                eprintln!("smoke 中止：run {name} 未通过。");
                return Ok(ExitCode::FAILURE);
            }
        }
        None => {
            println!("[4/4] run：未指定 --action，跳过（仅做 check/build 烟雾）。");
        }
    }

    println!("OK：smoke 通过。");
    Ok(ExitCode::SUCCESS)
}

/// `sophia repair-context --error <code>`：为 LLM 修复循环生成结构化上下文，**不调用模型**。
///
/// 见 docs/language_implementation.md 14.3、docs/engineering_architecture.md 9.1。
/// 确定性聚合：从 `check` 同口径的诊断里筛出匹配 `error_code` 的诊断，对每条诊断
/// 给出：归属文件与 1 基位置、诊断码与信息、以及（若诊断源自某 action / transition）
/// 该可调用体的 **action-rooted 语义闭包**作为「相关节点 / 文件」——正是 LLM 修复时
/// 需要看到的最小上下文。**不臆造修复建议**（具体改法是 LLM 的职责，脚手架只供事实）。
///
/// `error_code` 支持子串匹配（如 `CHECK-EFFECT` 命中 `CHECK-EFFECT-001`），便于按族筛选。
/// 无匹配诊断时打印提示并以成功退出（不是错误：可能项目本就干净）。
pub fn repair_context(root: &Path, error_code: &str) -> Result<ExitCode> {
    let project = Project::load(root)?;
    if report_syntax_errors(&project) {
        // 语法层错误优先暴露：repair-context 处理的是语义/名称解析层诊断。
        eprintln!("存在语法错误，请先修复语法（repair-context 处理语义层诊断）。");
        return Ok(ExitCode::FAILURE);
    }

    let (registry, lib_srcs) = library_context(root)?;
    let mut inputs = program_inputs(&project);
    inputs.extend(lib_srcs.program_inputs());
    let (index, _hir_diags) = resolve_program(&inputs, &registry).context("名称解析失败")?;
    let mut asts: Vec<&Ast> = project.files.iter().map(|f| &f.ast).collect();
    asts.extend(lib_srcs.asts());
    let model = SemanticModel::build(&asts, &index);

    let all = collect_diagnostics(&project, &index, &model, &asts);
    let needle = error_code.to_ascii_uppercase();
    let matched: Vec<&CollectedDiagnostic> = all
        .iter()
        .filter(|d| d.code.to_ascii_uppercase().contains(&needle))
        .collect();

    if matched.is_empty() {
        println!(
            "未找到匹配 `{error_code}` 的诊断（共扫描 {} 条诊断）。",
            all.len()
        );
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "修复上下文（匹配 `{error_code}`，{} 条诊断）：",
        matched.len()
    );
    for (i, d) in matched.iter().enumerate() {
        println!("───── 诊断 {} / {} ─────", i + 1, matched.len());
        println!("  诊断码：{}", d.code);
        println!(
            "  位置：  {}:{}:{}",
            d.rel_path,
            d.span.start.row + 1,
            d.span.start.column + 1
        );
        println!("  问题：  {}", d.message);

        // 相关节点 / 文件：诊断所在 action / transition 的 action-rooted 语义闭包。
        match &d.callable {
            Some(name) => match action_context(name, &asts, &index) {
                Ok(closure) => {
                    print_repair_closure(&closure);
                }
                Err(e) => {
                    println!("  相关节点：（计算 `{name}` 语义闭包失败：{e}）");
                }
            },
            None => {
                println!(
                    "  相关节点：（该诊断不在可调用体内，仅文件 {}）",
                    d.rel_path
                );
            }
        }
    }
    println!("（repair-context 只提供事实上下文，不臆造修复建议；具体改法由 LLM 修复循环决定。）");
    Ok(ExitCode::SUCCESS)
}

/// 打印修复上下文里一个语义闭包的「相关节点 / 文件」（稳定排序，无源码正文）。
fn print_repair_closure(closure: &ContextClosure) {
    if !closure.nodes.is_empty() {
        println!("  相关节点（{}）：", closure.nodes.len());
        for n in &closure.nodes {
            println!(
                "    {:<22} {:<10} {}",
                n.name,
                format!("{:?}", n.kind),
                n.path
            );
        }
    }
    if !closure.files.is_empty() {
        println!("  相关文件（{}）：", closure.files.len());
        for f in &closure.files {
            println!("    {f}");
        }
    }
}

// ---- 辅助 ----

fn program_inputs(project: &Project) -> Vec<ProgramInput<'_>> {
    project
        .files
        .iter()
        .map(|f| ProgramInput {
            domain: &f.domain,
            path: &f.rel_path,
            ast: &f.ast,
        })
        .collect()
}

/// 报告全部语法错误；返回是否存在错误。
fn report_syntax_errors(project: &Project) -> bool {
    if !project.has_syntax_errors() {
        return false;
    }
    for f in &project.files {
        for d in &f.syntax_diags {
            eprintln!("{}", render::syntax_line(&f.rel_path, d));
        }
    }
    true
}

/// 解析 `--arg` 实参，形如 `int:3` / `text:hello` / `bool:true`。
fn parse_args(raw: &[String]) -> Result<Vec<sophia_runtime::Value>> {
    use sophia_runtime::Value;
    raw.iter()
        .map(|s| {
            let (ty, val) = s
                .split_once(':')
                .with_context(|| format!("实参格式应为 `类型:值`，得到 `{s}`"))?;
            let v = match ty {
                "int" => Value::Int(
                    val.parse::<i64>()
                        .with_context(|| format!("非法整数 `{val}`"))?,
                ),
                "text" => Value::Text(val.to_string()),
                "bool" => Value::Bool(
                    val.parse::<bool>()
                        .with_context(|| format!("非法布尔 `{val}`"))?,
                ),
                "unit" => Value::Unit,
                other => anyhow::bail!("不支持的实参类型 `{other}`（支持 int/text/bool/unit）"),
            };
            Ok(v)
        })
        .collect()
}
