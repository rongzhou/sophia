//! Sophia 编程能力基准测试入口（见 docs/benchmark_test.md）。
//!
//! 横向对比「LLM 直接写 Python」（`baseline` mode）与「Sophia 工作流」（`sophia` mode）在多组
//! 小规模编程题上的**成功率 + 耗时**两个核心指标。
//!
//! ## 运行
//!
//! ```bash
//! export SOPHIA_LLM_API_KEY=<key>          # OpenAI 兼容模式需要；不落盘 / 不进图 / 不打印
//! cargo run -p sophia-cli --example benchmark                      # 全部题 × 两 mode
//! cargo run -p sophia-cli --example benchmark -- --task abs_difference
//! cargo run -p sophia-cli --example benchmark -- --level l1
//! cargo run -p sophia-cli --example benchmark -- --mode sophia     # 只跑某 mode
//! cargo run -p sophia-cli --example benchmark -- --llm-mode ollama  # 本地 Ollama
//! cargo run -p sophia-cli --example benchmark -- --runs 3          # 每 (题,mode) 跑 3 次
//! cargo run -p sophia-cli --example benchmark -- --list            # 列出题目（不需 key）
//! ```
//!
//! 环境变量：`SOPHIA_LLM_MODE`（openai / ollama）、`SOPHIA_LLM_MODEL`、`SOPHIA_LLM_BASE_URL`。
//! OpenAI 兼容模式未设 `SOPHIA_LLM_API_KEY` 时干净跳过（CI 安全）；Ollama 默认本地
//! `http://localhost:11434`，默认模型 `qwen3.6:latest`，无需 API key。缺 `python3` 时 baseline
//! mode 干净跳过（只跑 sophia）。

mod baseline_py;
mod problem;
mod problems;
mod report;
mod retry;
mod sophia_mode;
mod value_json;

use std::path::PathBuf;
use std::process::ExitCode;

use report::{append_run, render_summary, Mode, RunRecord};
use sophia_llm::{BackendConfig, HttpLlmClient};

use crate::problem::{Level, Problem};

const DEFAULT_OPENAI_MODEL: &str = "deepseek-ai/deepseek-v4-flash";
const DEFAULT_OPENAI_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
const DEFAULT_OLLAMA_MODEL: &str = "qwen3.6:latest";
const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_OPENAI_TIMEOUT_SECS: u64 = 120;
const DEFAULT_OLLAMA_TIMEOUT_SECS: u64 = 300;

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("参数错误：{e}");
            return ExitCode::FAILURE;
        }
    };

    let selected = select_problems(&opts);
    if matches!(opts.action, Action::List) {
        for p in &selected {
            println!("{} [{}] {}", p.id, p.level.as_str(), p.title);
        }
        return ExitCode::SUCCESS;
    }
    if selected.is_empty() {
        eprintln!("没有匹配的题目");
        return ExitCode::FAILURE;
    }

    let llm = match opts.llm.resolve() {
        Ok(llm) => llm,
        Err(LlmConfigError::MissingApiKey) => {
            eprintln!(
                "OpenAI 兼容模式未设置 SOPHIA_LLM_API_KEY，跳过真实 LLM 基准测试。\n\
                 用法：\n  export SOPHIA_LLM_API_KEY=<key>\n  \
                 cargo run -p sophia-cli --example benchmark [-- --llm-mode openai | --llm-mode ollama]"
            );
            return ExitCode::SUCCESS;
        }
        Err(LlmConfigError::InvalidMode(m)) => {
            eprintln!("参数错误：不支持的 LLM 后端 `{m}`（支持 openai / ollama）");
            return ExitCode::FAILURE;
        }
        Err(LlmConfigError::InvalidTimeout(raw)) => {
            eprintln!("参数错误：SOPHIA_LLM_TIMEOUT_SECS 非法：{raw}");
            return ExitCode::FAILURE;
        }
    };

    // 确定要跑的 mode 集合：尊重 --mode；baseline 需要 python3，缺则干净跳过。
    let mut modes: Vec<Mode> = match opts.mode {
        Some(m) => vec![m],
        None => vec![Mode::Sophia, Mode::Baseline],
    };
    if modes.contains(&Mode::Baseline) && !python3_available() {
        eprintln!("未检测到 python3，baseline mode 跳过（只跑 sophia）。");
        modes.retain(|m| *m != Mode::Baseline);
    }
    if modes.is_empty() {
        eprintln!("没有可运行的 mode。");
        return ExitCode::SUCCESS;
    }

    let model = llm.model.clone();
    let retry_attempts = llm.retry_attempts;
    let client = match HttpLlmClient::new(llm.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("构造 LLM 后端失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    // 有界重试。OpenAI 兼容远端容忍偶发抖动；Ollama 默认不重试，避免本地生成超时后重复请求。
    let client = retry::with_retry(client, retry_attempts);

    let out_dir = PathBuf::from("sophia-runs/benchmark").join(&opts.label);

    println!("== Sophia 编程能力基准测试 ==");
    println!("后端：{}", llm.mode_label);
    println!("模型：{model}");
    println!("端点：{}", llm.base_url);
    println!(
        "超时：{}s；请求重试：{} 次",
        llm.timeout_secs, retry_attempts
    );
    println!(
        "题目：{} 道；mode：{:?}；每组运行：{} 次；产物：{}",
        selected.len(),
        modes.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
        opts.runs,
        out_dir.join("runs.jsonl").display()
    );

    let records = tokio_block_on(async {
        let mut records: Vec<RunRecord> = Vec::new();
        for problem in &selected {
            for &mode in &modes {
                for run_index in 0..opts.runs {
                    println!(
                        "\n──────── [{}] {} · {} · 第 {}/{} 次 ────────",
                        problem.id,
                        problem.title,
                        mode.as_str(),
                        run_index + 1,
                        opts.runs
                    );
                    let record = match mode {
                        Mode::Sophia => sophia_mode::run(&client, &model, problem).await,
                        Mode::Baseline => baseline_py::run(&client, &model, problem).await,
                    };
                    print_record(&record);
                    if let Err(e) = append_run(&out_dir, &record) {
                        eprintln!("写 runs.jsonl 失败：{e}");
                    }
                    records.push(record);
                }
            }
        }
        records
    });

    // 聚合表（核心两指标）。
    let summary = render_summary(&records);
    println!("\n════════ 汇总（成功率 / 平均耗时） ════════\n{summary}");
    if let Err(e) = std::fs::write(out_dir.join("summary.md"), &summary) {
        eprintln!("写 summary.md 失败：{e}");
    }

    // 退出码：只要有任一运行通过即视为成功执行（benchmark 是度量而非门禁；失败题如实记录）。
    ExitCode::SUCCESS
}

fn print_record(r: &RunRecord) {
    let mark = if r.passed { "✓ PASS" } else { "✗ FAIL" };
    println!(
        "{mark} · {} ms · {}",
        r.wall_time_ms,
        r.failure.as_deref().unwrap_or("通过全部 hidden case")
    );
    for c in &r.cases {
        println!(
            "    {} {} — {}",
            if c.passed { "✓" } else { "✗" },
            c.name,
            c.detail
        );
    }
}

/// 命令行选项。
struct Opts {
    action: Action,
    /// 题目过滤（None = 全部）。
    filter: Filter,
    /// mode 过滤（None = 两个都跑）。
    mode: Option<Mode>,
    /// 每 (题, mode) 运行次数。
    runs: u32,
    /// 产物子目录标签。
    label: String,
    /// LLM 后端配置。
    llm: LlmArgs,
}

enum Action {
    Run,
    List,
}

enum Filter {
    All,
    Task(String),
    Level(Level),
}

#[derive(Default)]
struct LlmArgs {
    mode: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    timeout_secs: Option<u64>,
}

struct ResolvedLlm {
    mode_label: &'static str,
    model: String,
    base_url: String,
    timeout_secs: u64,
    retry_attempts: u32,
    config: BackendConfig,
}

enum LlmConfigError {
    MissingApiKey,
    InvalidMode(String),
    InvalidTimeout(String),
}

impl LlmArgs {
    fn resolve(self) -> Result<ResolvedLlm, LlmConfigError> {
        let mode = self
            .mode
            .or_else(|| std::env::var("SOPHIA_LLM_MODE").ok())
            .unwrap_or_else(|| "openai".to_string())
            .to_lowercase();
        let env_model = || std::env::var("SOPHIA_LLM_MODEL").ok();
        let env_base = || std::env::var("SOPHIA_LLM_BASE_URL").ok();
        let env_timeout = parse_env_timeout_secs()?;
        let key = self
            .api_key
            .or_else(|| std::env::var("SOPHIA_LLM_API_KEY").ok());

        match mode.as_str() {
            "openai" => {
                let Some(api_key) = key else {
                    return Err(LlmConfigError::MissingApiKey);
                };
                let base_url = self
                    .base_url
                    .or_else(env_base)
                    .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());
                let timeout_secs = self
                    .timeout_secs
                    .or(env_timeout)
                    .unwrap_or(DEFAULT_OPENAI_TIMEOUT_SECS);
                let mut config = BackendConfig::openai(api_key);
                config.base_url = base_url.clone();
                config.timeout_secs = timeout_secs;
                Ok(ResolvedLlm {
                    mode_label: "openai",
                    model: self
                        .model
                        .or_else(env_model)
                        .unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string()),
                    base_url: base_url.clone(),
                    timeout_secs,
                    retry_attempts: 6,
                    config,
                })
            }
            "ollama" => {
                let base_url = self
                    .base_url
                    .or_else(env_base)
                    .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_string());
                let timeout_secs = self
                    .timeout_secs
                    .or(env_timeout)
                    .unwrap_or(DEFAULT_OLLAMA_TIMEOUT_SECS);
                let mut config = BackendConfig::ollama();
                config.base_url = base_url.clone();
                config.api_key = key;
                config.timeout_secs = timeout_secs;
                Ok(ResolvedLlm {
                    mode_label: "ollama",
                    model: self
                        .model
                        .or_else(env_model)
                        .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string()),
                    base_url: base_url.clone(),
                    timeout_secs,
                    retry_attempts: 1,
                    config,
                })
            }
            other => Err(LlmConfigError::InvalidMode(other.to_string())),
        }
    }
}

fn parse_env_timeout_secs() -> Result<Option<u64>, LlmConfigError> {
    match std::env::var("SOPHIA_LLM_TIMEOUT_SECS") {
        Ok(raw) => raw
            .parse::<u64>()
            .map(Some)
            .map_err(|_| LlmConfigError::InvalidTimeout(raw)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(raw)) => Err(LlmConfigError::InvalidTimeout(
            raw.to_string_lossy().into_owned(),
        )),
    }
}

fn select_problems(opts: &Opts) -> Vec<Problem> {
    match &opts.filter {
        Filter::All => problems::all_problems(),
        Filter::Task(id) => problems::by_id(id),
        Filter::Level(l) => problems::by_level(*l),
    }
}

fn parse_args() -> Result<Opts, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut action = Action::Run;
    let mut filter = Filter::All;
    let mut mode = None;
    let mut runs = 1u32;
    let mut label = "default".to_string();
    let mut llm = LlmArgs::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--list" => {
                action = Action::List;
                i += 1;
            }
            "--task" => {
                let id = args.get(i + 1).ok_or("--task 需要一个题目 id")?;
                filter = Filter::Task(id.clone());
                i += 2;
            }
            "--level" => {
                let l = args.get(i + 1).ok_or("--level 需要一个分级（如 l1）")?;
                let level = Level::parse(l).ok_or_else(|| format!("未知分级 `{l}`"))?;
                filter = Filter::Level(level);
                i += 2;
            }
            "--mode" => {
                let m = args.get(i + 1).ok_or("--mode 需要 sophia 或 baseline")?;
                mode = Some(match m.to_lowercase().as_str() {
                    "sophia" => Mode::Sophia,
                    "baseline" => Mode::Baseline,
                    other => return Err(format!("未知 mode `{other}`（应为 sophia | baseline）")),
                });
                i += 2;
            }
            "--runs" => {
                let n = args.get(i + 1).ok_or("--runs 需要一个正整数")?;
                runs = n.parse().map_err(|_| format!("--runs 非法：{n}"))?;
                if runs == 0 {
                    return Err("--runs 必须 >= 1".to_string());
                }
                i += 2;
            }
            "--label" => {
                let l = args.get(i + 1).ok_or("--label 需要一个名字")?;
                label = l.clone();
                i += 2;
            }
            "--llm-mode" => {
                llm.mode = Some(
                    args.get(i + 1)
                        .ok_or("--llm-mode 需要 openai 或 ollama")?
                        .clone(),
                );
                i += 2;
            }
            "--llm-model" => {
                llm.model = Some(args.get(i + 1).ok_or("--llm-model 需要模型名")?.clone());
                i += 2;
            }
            "--llm-base-url" => {
                llm.base_url = Some(args.get(i + 1).ok_or("--llm-base-url 需要 URL")?.clone());
                i += 2;
            }
            "--llm-api-key" => {
                llm.api_key = Some(args.get(i + 1).ok_or("--llm-api-key 需要 API key")?.clone());
                i += 2;
            }
            "--llm-timeout-secs" => {
                let raw = args.get(i + 1).ok_or("--llm-timeout-secs 需要秒数")?;
                llm.timeout_secs = Some(
                    raw.parse()
                        .map_err(|_| format!("--llm-timeout-secs 非法：{raw}"))?,
                );
                i += 2;
            }
            other => return Err(format!("未知参数 `{other}`")),
        }
    }
    Ok(Opts {
        action,
        filter,
        mode,
        runs,
        label,
        llm,
    })
}

/// 探测 `python3` 是否可用（`python3 --version` 能成功执行）。
fn python3_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn tokio_block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("构造 tokio 运行时")
        .block_on(fut)
}
