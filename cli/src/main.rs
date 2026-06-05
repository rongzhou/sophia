//! Sophia CLI 入口（协调层）。
//!
//! 见 docs/engineering_architecture.md 第九节。CLI 是 IO 与呈现的归属层：
//! `core` / `tools` 保持无 IO，文件读取与诊断渲染都在这里完成。
//!
//! 命令分两类：
//! - **确定性命令**（不调用 LLM）：`init` / `parse` / `index` / `check` / `build`（emit WASM）/
//!   `run`（含 `--trace`）/ `context` / `smoke` / `repair-context` / `graph`（无子命令 = ASG 摘要）/
//!   `graph init`/`start`/`context`/`nodes`/`select`/`materialize` / `lsp`；
//! - **LLM 命令**：`graph design` / `graph implement-loop`（经 `--model`/`--mode` 构造后端）。

mod cli_modules {
    // main 作为二进制入口经库 crate 复用协调层构件（与 examples 共享同一份实现）。
    pub use sophia_cli::{commands, graph_cmd};
}
use cli_modules::{commands, graph_cmd};

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

/// Sophia 命令行工具。
#[derive(Debug, Parser)]
#[command(name = "sophia", version, about = "Sophia 语义执行平台 CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 创建标准目录结构和 sophia.toml。
    Init {
        /// 目标目录（默认当前目录）。
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// 项目名（默认取目录名）。
        #[arg(long)]
        name: Option<String>,
    },

    /// 解析单个 node 文件，报告语法诊断。
    Parse {
        /// `.sophia` 源文件路径。
        file: PathBuf,
    },

    /// 扫描 node 文件并生成 asg_index.json。
    Index {
        /// 项目根目录（默认当前目录）。
        #[arg(default_value = ".")]
        root: PathBuf,
    },

    /// 输出 ASG 摘要（节点与跨节点引用统计）。
    Graph {
        /// 项目根目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Development Graph 工作流子命令（缺省时输出 ASG 摘要）。
        #[command(subcommand)]
        sub: Option<GraphCmd>,
    },

    /// 执行静态检查（语法 + 名称解析 + 语义三层）。
    Check {
        #[arg(default_value = ".")]
        root: PathBuf,
    },

    /// 生成 action-rooted 语义上下文或 task closure（确定性，不调用 LLM）。
    Context {
        /// 从 action root 计算语义闭包。
        #[arg(long, conflicts_with = "task")]
        action: Option<String>,
        /// 从 task root 计算 task closure。
        #[arg(long)]
        task: Option<String>,
        /// 携带闭包内源码内容（§8.1 步骤 9 的 sources）。
        #[arg(long)]
        sources: bool,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 起步阶段为空操作；v1 起 emit WASM artifact（工作流 A）。
    Build {
        #[arg(default_value = ".")]
        root: PathBuf,
    },

    /// 执行 action。
    Run {
        /// action 名称。
        action: String,
        /// 项目根目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// 实参（按 input 顺序，形如 `int:3` / `text:hello` / `bool:true`）。
        #[arg(long = "arg")]
        args: Vec<String>,
        /// 打印 Execution Graph 执行 Trace 投影（§9.4：节点 / 调用边 / 结局）。
        #[arg(long)]
        trace: bool,
        /// 执行后端：默认解释器；`wasm` 执行 `sophia build` 产出的 program.wasm。
        #[arg(long, value_enum, default_value_t = CliRunBackend::Interpreter)]
        backend: CliRunBackend,
    },

    /// 一键烟雾测试（init → check → build → run），确定性、不调用 LLM。
    Smoke {
        /// 项目根目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// 可选：烟雾运行的 action 名称（省略则只做 check / build）。
        #[arg(long)]
        action: Option<String>,
        /// run 步骤的实参（形如 `int:3`；仅在指定 `--action` 时使用）。
        #[arg(long = "arg")]
        args: Vec<String>,
        /// run 步骤执行后端：默认解释器；`wasm` 会先 build 再执行产物。
        #[arg(long, value_enum, default_value_t = CliRunBackend::Interpreter)]
        backend: CliRunBackend,
    },

    /// 生成 LLM 修复上下文（结构化诊断 + 相关节点闭包），不调用 LLM。
    RepairContext {
        /// 要聚焦的诊断码（支持子串匹配，如 `CHECK-EFFECT` / `CHECK-TYPE-001`）。
        #[arg(long = "error")]
        error: String,
        /// 项目根目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 以 stdio 运行 Language Server（hover / diagnostics / goto definition）。
    Lsp,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliRunBackend {
    Interpreter,
    Wasm,
}

/// Development Graph 工作流子命令。
///
/// 见 docs/engineering_architecture.md 9.2。这些命令在 `sophia-runs/graph/dev_graph.sqlite`
/// 上以事件溯源方式 append 节点 / 边（仅增、不可变）。`init`/`start`/`context`/`nodes`/`select`/
/// `materialize` 是确定性命令；`design`/`implement-loop` 调用 LLM 后端。
#[derive(Debug, Subcommand)]
enum GraphCmd {
    /// 初始化 Development Graph 存储（创建空的事件溯源 SQLite）。
    Init {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 以一个人类目标开启工作流：创建 ObjectiveNode（provenance=human）。
    Start {
        /// 目标标题。
        title: String,
        /// 目标描述（缺省复用标题）。
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 计算并展示当前 active context（确定性推导，不写图）。
    Context {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 列出图中全部节点（按 ID 升序）。
    Nodes {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 为目标域生成结构化伪代码（design_solution，调用 LLM）。
    Design {
        /// 目标域节点 ID（如 `N0001`，须为 Objective | Milestone | FirstSlice）。
        node: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(flatten)]
        backend: BackendArgs,
    },

    /// 实现伪代码并在预算内修复（implement-loop：implement → code_check → repair）。
    ImplementLoop {
        /// 目标域节点 ID（须为 Objective | Milestone | FirstSlice）。
        node: String,
        /// 被实现的 Pseudocode 节点 ID。
        #[arg(long)]
        pseudo: String,
        /// 最大修复次数（design 10.9）。
        #[arg(long, default_value = "2")]
        max_repairs: u32,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(flatten)]
        backend: BackendArgs,
    },

    /// 由 LLM DecisionNode 驱动目标推进；可让位/执行 decompose，而非固定流程。
    Drive {
        /// 目标域节点 ID（须为 Objective | Milestone）。
        node: String,
        /// 最大 decision 轮数。
        #[arg(long, default_value = "6")]
        max_decisions: u32,
        /// 最大修复次数（传给 implement-loop）。
        #[arg(long, default_value = "2")]
        max_repairs: u32,
        /// 单目标最大伪代码版本数。
        #[arg(long, default_value = "3")]
        max_pseudocode_versions: u32,
        /// decompose 嵌套深度上限。
        #[arg(long, default_value = "3")]
        max_depth: u32,
        /// 目标树遍历的目标总数上限。
        #[arg(long, default_value = "16")]
        max_goals: u32,
        /// 自动接受 LLM 产生的拆解（调用方代表人类授权）；不设置则拆解会被拒绝。
        #[arg(long)]
        auto_accept_decompositions: bool,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(flatten)]
        backend: BackendArgs,
    },

    /// 选中一个通过 gate 的候选 CodeNode（建 SelectionNode）。
    Select {
        /// 候选 Code 节点 ID。
        node: String,
        /// 选择理由（SelectionNode payload）。
        #[arg(long, default_value = "确定性管线选中唯一通过 gate 的候选")]
        rationale: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// 物化一个已选中的候选到 `domains/`（重跑 gate + staging/rename 写盘）。
    Materialize {
        /// SelectionNode ID。
        node: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

/// LLM 后端配置参数（OpenAI 兼容 / Ollama 两种模式）。
#[derive(Debug, clap::Args)]
struct BackendArgs {
    /// 模型名（如 `gpt-4o-mini`、`qwen3`）。
    #[arg(long)]
    model: String,
    /// 后端模式：`openai`（OpenAI 兼容）或 `ollama`。
    #[arg(long, default_value = "ollama")]
    mode: String,
    /// 基地址（覆盖默认；如自建网关 / 远端 Ollama）。
    #[arg(long)]
    base_url: Option<String>,
    /// API key（OpenAI 兼容用；也可经 `SOPHIA_LLM_API_KEY` 环境变量提供）。
    #[arg(long)]
    api_key: Option<String>,
    /// 单次 LLM 调用墙钟上限秒数；0 表示关闭。也可用 `SOPHIA_LLM_CALL_TIMEOUT_SECS`。
    #[arg(long)]
    call_timeout_secs: Option<u64>,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("错误：{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Init { dir, name } => commands::init(&dir, name.as_deref()),
        Command::Parse { file } => commands::parse(&file),
        Command::Index { root } => commands::index(&root),
        Command::Graph { root, sub } => match sub {
            None => commands::graph(&root),
            Some(GraphCmd::Init { root }) => graph_cmd::init(&root),
            Some(GraphCmd::Start {
                title,
                description,
                root,
            }) => graph_cmd::start(&root, &title, description.as_deref()),
            Some(GraphCmd::Context { root }) => graph_cmd::context(&root),
            Some(GraphCmd::Nodes { root }) => graph_cmd::nodes(&root),
            Some(GraphCmd::Design {
                node,
                root,
                backend,
            }) => graph_cmd::design(
                &root,
                &node,
                &backend.model,
                &backend.mode,
                backend.base_url.as_deref(),
                backend.api_key.as_deref(),
                backend.call_timeout_secs,
            ),
            Some(GraphCmd::ImplementLoop {
                node,
                pseudo,
                max_repairs,
                root,
                backend,
            }) => graph_cmd::implement_loop(
                &root,
                &node,
                &pseudo,
                max_repairs,
                &backend.model,
                &backend.mode,
                backend.base_url.as_deref(),
                backend.api_key.as_deref(),
                backend.call_timeout_secs,
            ),
            Some(GraphCmd::Drive {
                node,
                max_decisions,
                max_repairs,
                max_pseudocode_versions,
                max_depth,
                max_goals,
                auto_accept_decompositions,
                root,
                backend,
            }) => graph_cmd::drive(
                &root,
                &node,
                max_decisions,
                max_repairs,
                max_pseudocode_versions,
                max_depth,
                max_goals,
                auto_accept_decompositions,
                &backend.model,
                &backend.mode,
                backend.base_url.as_deref(),
                backend.api_key.as_deref(),
                backend.call_timeout_secs,
            ),
            Some(GraphCmd::Select {
                node,
                rationale,
                root,
            }) => graph_cmd::select(&root, &node, &rationale),
            Some(GraphCmd::Materialize { node, root }) => graph_cmd::materialize(&root, &node),
        },
        Command::Check { root } => commands::check(&root),
        Command::Context {
            action,
            task,
            sources,
            root,
        } => commands::context(&root, action.as_deref(), task.as_deref(), sources),
        Command::Build { root } => commands::build(&root),
        Command::Run {
            action,
            root,
            args,
            trace,
            backend,
        } => commands::run_action(&root, &action, &args, trace, backend.into()),
        Command::Smoke {
            root,
            action,
            args,
            backend,
        } => commands::smoke(&root, action.as_deref(), &args, backend.into()),
        Command::RepairContext { error, root } => commands::repair_context(&root, &error),
        Command::Lsp => {
            // LSP 是长驻 stdio 服务，需要 tokio 运行时承载（与确定性命令的同步路径分开）。
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("构造 LSP tokio 运行时")?
                .block_on(sophia_lsp::run_stdio());
            Ok(ExitCode::SUCCESS)
        }
    }
}

impl From<CliRunBackend> for commands::RunBackend {
    fn from(value: CliRunBackend) -> Self {
        match value {
            CliRunBackend::Interpreter => commands::RunBackend::Interpreter,
            CliRunBackend::Wasm => commands::RunBackend::Wasm,
        }
    }
}
