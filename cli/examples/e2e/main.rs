//! Sophia v0 端到端（e2e）真实 LLM 测试入口（见 docs/e2e_test.md）。
//!
//! 单一 harness + 用例注册表，按组组织。验证完整 v0 闭环
//! design → implement → check → repair → v0 解释器执行 在真实 LLM 下端到端可用。
//!
//! ## 运行
//!
//! ```bash
//! export SOPHIA_LLM_API_KEY=<key>          # OpenAI 兼容模式需要；不落盘 / 不进图 / 不打印
//! cargo run -p sophia-cli --example e2e                            # 全部用例
//! cargo run -p sophia-cli --example e2e -- --group g1              # 只跑某组
//! cargo run -p sophia-cli --example e2e -- --case G1-02            # 只跑某用例
//! cargo run -p sophia-cli --example e2e -- --llm-mode ollama       # 本地 Ollama
//! ```
//!
//! 环境变量：`SOPHIA_LLM_MODE`（openai / ollama）、`SOPHIA_LLM_MODEL`、`SOPHIA_LLM_BASE_URL`。
//! OpenAI 兼容模式未设置 `SOPHIA_LLM_API_KEY` 时干净跳过（CI 安全）；Ollama 默认本地
//! `http://localhost:11434`，默认模型 `qwen3.6:latest`，无需 API key。

mod cases;
mod harness;

use std::process::ExitCode;

use sophia_llm::{BackendConfig, HttpLlmClient};

const DEFAULT_OPENAI_MODEL: &str = "deepseek-ai/deepseek-v4-flash";
const DEFAULT_OPENAI_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
const DEFAULT_OLLAMA_MODEL: &str = "qwen3.6:latest";
const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_OPENAI_TIMEOUT_SECS: u64 = 120;
const DEFAULT_OLLAMA_TIMEOUT_SECS: u64 = 300;

fn main() -> ExitCode {
    // 解析过滤参数（--group <g> / --case <ID> / --list）与 LLM 后端参数。
    let opts = match parse_args() {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("参数错误：{e}");
            return ExitCode::FAILURE;
        }
    };

    // --list：仅列出用例 ID（供批量脚本枚举），不需要 API key。
    if matches!(opts.selection, Selection::List) {
        for c in cases::all_cases() {
            println!("{} {}", c.id, c.title);
        }
        return ExitCode::SUCCESS;
    }

    let llm = match opts.llm.resolve() {
        Ok(llm) => llm,
        Err(LlmConfigError::MissingApiKey) => {
            eprintln!(
                "OpenAI 兼容模式未设置 SOPHIA_LLM_API_KEY，跳过真实 LLM e2e 测试。\n\
                 用法：\n  export SOPHIA_LLM_API_KEY=<key>\n  \
                 cargo run -p sophia-cli --example e2e [-- --llm-mode openai | --llm-mode ollama]"
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

    let cases = match &opts.selection {
        Selection::All => cases::all_cases(),
        Selection::Group(g) => cases::by_group(g),
        Selection::Case(id) => cases::by_id(id),
        Selection::List => unreachable!("已在上面处理"),
    };
    if cases.is_empty() {
        eprintln!("没有匹配的用例：{:?}", opts.selection);
        return ExitCode::FAILURE;
    }

    println!("== Sophia v0 e2e 真实 LLM 测试 ==");
    println!("后端：{}", llm.mode_label);
    println!("模型：{}", llm.model);
    println!("端点：{}", llm.config.base_url);
    println!(
        "超时：{}s；请求重试：{} 次",
        llm.config.timeout_secs, llm.retry_attempts
    );
    println!("选择：{:?}（{} 个用例）", opts.selection, cases.len());

    let model = llm.model.clone();
    let retry_attempts = llm.retry_attempts;
    let client = match HttpLlmClient::new(llm.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("构造 LLM 后端失败：{e}");
            return ExitCode::FAILURE;
        }
    };
    // 包一层有界重试。OpenAI 兼容远端容忍偶发抖动；Ollama 默认不重试，避免本地生成超时后重复请求。
    let client = harness::with_retry(client, retry_attempts);

    let reports = tokio_block_on(async {
        let mut reports = Vec::new();
        for case in &cases {
            reports.push(harness::run_case(&client, &model, case).await);
        }
        reports
    });

    // 汇总。
    let passed = reports.iter().filter(|r| r.passed).count();
    let total = reports.len();
    println!("\n════════ 汇总：{passed}/{total} 通过 ════════");
    for r in &reports {
        if r.passed {
            println!("  ✓ {}", r.id);
        } else {
            println!("  ✗ {} — {}", r.id, r.detail);
        }
    }

    if passed == total {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 用例选择。
#[derive(Debug)]
enum Selection {
    All,
    Group(String),
    Case(String),
    /// 仅列出用例 ID（供批量脚本枚举）。
    List,
}

struct Opts {
    selection: Selection,
    llm: LlmArgs,
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
                let mut config = BackendConfig::openai(api_key);
                config.base_url = self
                    .base_url
                    .or_else(env_base)
                    .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());
                config.timeout_secs = self
                    .timeout_secs
                    .or(env_timeout)
                    .unwrap_or(DEFAULT_OPENAI_TIMEOUT_SECS);
                Ok(ResolvedLlm {
                    mode_label: "openai",
                    model: self
                        .model
                        .or_else(env_model)
                        .unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string()),
                    retry_attempts: 6,
                    config,
                })
            }
            "ollama" => {
                let mut config = BackendConfig::ollama();
                config.base_url = self
                    .base_url
                    .or_else(env_base)
                    .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_string());
                config.api_key = key;
                config.timeout_secs = self
                    .timeout_secs
                    .or(env_timeout)
                    .unwrap_or(DEFAULT_OLLAMA_TIMEOUT_SECS);
                Ok(ResolvedLlm {
                    mode_label: "ollama",
                    model: self
                        .model
                        .or_else(env_model)
                        .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string()),
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

/// 解析 `--group <g>` / `--case <ID>` / `--list` 与 LLM 后端参数。
fn parse_args() -> Result<Opts, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut selection = Selection::All;
    let mut llm = LlmArgs::default();
    while i < args.len() {
        match args[i].as_str() {
            "--list" => {
                selection = Selection::List;
                i += 1;
            }
            "--group" => {
                let g = args.get(i + 1).ok_or("--group 需要一个组名")?;
                selection = Selection::Group(g.to_lowercase());
                i += 2;
            }
            "--case" => {
                let id = args.get(i + 1).ok_or("--case 需要一个用例 ID")?;
                selection = Selection::Case(id.to_uppercase());
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
    Ok(Opts { selection, llm })
}

fn tokio_block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("构造 tokio 运行时")
        .block_on(fut)
}
