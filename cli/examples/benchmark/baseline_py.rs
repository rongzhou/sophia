//! `baseline` mode（Python）：LLM 直接写一个自包含 Python 模块，由外部 `python3` 子进程
//! 逐 hidden case 执行并对照（见 docs/benchmark_test.md §一 / §五 / §三.1）。
//!
//! **项目固有不对称（诚实标注）**：`sophia` mode 判定复用 `runtime::verify`（零新增执行
//! 能力），而本 mode 必须真正执行 LLM 生成的 Python——当前纯 Rust 工作区不具备，这是从零搭
//! 的子进程执行 + `Value↔JSON` 规约。`python3` 是**运行期外部工具**依赖，不进 Cargo 树；
//! 缺 `python3` 时由入口干净跳过。
//!
//! **安全（执行 LLM 生成的任意代码）**：受限临时工作目录 + 硬超时 + 用后清理。
//!
//! **防答案泄漏**：prompt 只由 `PublicBrief` 渲染；hidden cases 只在 benchmark 拥有的 runner
//! 夹具内部使用，绝不进 prompt。
//!
//! **判定协议**：runner 脚本 `import` 候选模块、对每个 case 调用入口函数 `run_action(input)`，
//! 把 `{"ok":true,"result":<JSON>}` 或（抛异常时）`{"ok":false,"error":"<类名>"}` 打到 stdout。
//! benchmark 读回，与 `ExpectedOutcome` 对照：`Returns(v)` 比 JSON 值相等；`Raises(variant)` 比
//! 抛出的异常类名等于 variant（baseline 契约要求"非法输入抛同名异常类"）。

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value as Json};
use sophia_llm::{complete_structured, CompletionRequest, LlmClient, StructuredConfig};
use sophia_runtime::{ExpectedOutcome, HiddenCase};

use crate::problem::{Problem, PublicBrief};
use crate::report::{CaseOutcome, Mode, RunRecord};
use crate::value_json::value_to_json;

/// 单个 hidden case 的子进程硬超时（秒）：防失控 / 死循环的 LLM 代码挂起整轮。
const CASE_TIMEOUT_SECS: u64 = 5;

/// baseline 结构化输出的 JSON schema：要求恰好一个 `code` 字段（完整 Python 模块源码）。
fn baseline_schema() -> Json {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["code"],
        "properties": {
            "code": { "type": "string" }
        }
    })
}

/// 运行一道题的 baseline(Python) mode，返回结构化记录。计时口径见设计 §五：只计 baseline
/// 单次 LLM 调用（含结构化重试）的墙钟，**不计** hidden case 子进程执行本身。
pub async fn run<C: LlmClient>(client: &C, model: &str, problem: &Problem) -> RunRecord {
    let mk = |passed: bool, failure: Option<String>, cases: Vec<CaseOutcome>, ms: u128| RunRecord {
        id: problem.id.to_string(),
        level: problem.level.as_str().to_string(),
        mode: Mode::Baseline,
        language: Some("python".to_string()),
        model: model.to_string(),
        passed,
        wall_time_ms: ms,
        failure,
        cases,
    };

    // 1) LLM 直接写一个自包含 Python 模块（计时只覆盖这一步）。
    //    反序列化到 serde_json::Value（避免在 cli 引 serde derive），再取 `code` 字段——
    //    schema 已保证恰好含 string 型 `code`。
    let req = baseline_request(model, &problem.public_brief());
    let started = Instant::now();
    let value: Json = match complete_structured(
        client,
        &req,
        &baseline_schema(),
        &StructuredConfig::default(),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            let ms = started.elapsed().as_millis();
            return mk(
                false,
                Some(format!("baseline 生成失败：{e}")),
                Vec::new(),
                ms,
            );
        }
    };
    let wall_time_ms = started.elapsed().as_millis();
    let code = value
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 2) 子进程逐 case 执行 + 对照（不计入 wall_time）。
    match execute_cases(&code, problem) {
        Ok(cases) => {
            let all_passed = !cases.is_empty() && cases.iter().all(|c| c.passed);
            let failure = if all_passed {
                None
            } else {
                Some("部分 hidden case 未通过".to_string())
            };
            mk(all_passed, failure, cases, wall_time_ms)
        }
        Err(e) => mk(false, Some(e), Vec::new(), wall_time_ms),
    }
}

/// 组装 baseline prompt（**只用公开题面**；含 anti-cheat 子句，设计 §三）。
fn baseline_request(model: &str, brief: &PublicBrief<'_>) -> CompletionRequest {
    let contract = brief.entry_contract_lines().join("\n- ");
    let forbidden = if brief.public_forbidden.is_empty() {
        "（无额外禁止事项）".to_string()
    } else {
        brief
            .public_forbidden
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let user = format!(
        "任务：{}\n\n题面：{}\n\n入口契约：\n- {contract}\n\n禁止事项：\n{forbidden}",
        brief.title, brief.prompt_goal
    );
    // 可恢复失败返回的输出对齐：成功返回值本身；失败返回约定形状的 dict。
    let recoverable_failure_clause =
        "- 只有题面明确要求「可恢复失败结局」或「返回结局」时，才按返回值处理：成功时直接 return 成功值本身；\
         失败时 return 一个 dict {\"variant\": \"<失败名称>\", \"fields\": {<字段名>: <值>}}。\
         这是返回结局而非抛异常。若题面说「中断式领域失败」，不要返回该 dict。\n";
    let system = format!(
        "你是一个 Python 基线实现者。只输出一个 JSON 对象，含恰好一个字段 code（完整的 \
         Python 模块源码字符串）。要求：\n\
         - 模块导出**恰好一个**函数 `def run_action(input):`，参数 input 是一个 dict，\
           键为入口契约里列出的参数名；函数直接 return 结果值。\n\
         - 结果值用 Python 原生类型表达（int / bool / str / list / dict）。状态类输出返回\
           其取值名字符串。\n\
         {recoverable_failure_clause}\
         - 若题面要求在非法输入时以中断式领域失败结束，则**抛出一个异常**。异常类名取题面给出的具体失败名称。\
           可临时定义同名 Exception 子类。这样判定按你抛出的具体失败身份比对。\n\
         - 禁止读取文件 / 环境变量 / 时间 / 随机 / 进程状态 / 测试数据；\
           禁止针对具体输入特判或硬编码答案（只实现题面要求的通用逻辑）。\n\
         - 不要输出 markdown 围栏或额外说明，只输出 JSON 对象。"
    );
    let mut req = CompletionRequest::new(model, user);
    req.system = Some(system);
    req
}

/// 在受限临时目录里逐 case 执行候选 Python 模块，返回每个 case 的判定。
fn execute_cases(code: &str, problem: &Problem) -> Result<Vec<CaseOutcome>, String> {
    // 受限临时工作目录（用后清理）。用进程 id + 题 id 命名，避免并发碰撞。
    let dir = std::env::temp_dir().join(format!(
        "sophia-bench-{}-{}",
        std::process::id(),
        problem.id
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("建临时目录失败：{e}"))?;
    let guard = DirGuard(dir.clone());

    // 写候选模块 + benchmark 拥有的 runner 夹具（runner 内部才接触 hidden case 输入）。
    std::fs::write(dir.join("candidate.py"), code).map_err(|e| format!("写候选失败：{e}"))?;
    std::fs::write(dir.join("runner.py"), RUNNER_PY).map_err(|e| format!("写 runner 失败：{e}"))?;

    let mut cases = Vec::new();
    for case in &problem.hidden_cases {
        cases.push(run_one_case(&dir, problem, case)?);
    }
    drop(guard); // 显式触发清理（即便上面出错，DirGuard 的 Drop 也会清）。
    Ok(cases)
}

/// 跑单个 case：把输入 JSON 经 stdin 喂给 runner，读回实际结局 JSON，与期望对照。
fn run_one_case(
    dir: &std::path::Path,
    problem: &Problem,
    case: &HiddenCase,
) -> Result<CaseOutcome, String> {
    let input_json = problem.named_input_json(case);
    let payload = json!({
        "entry": problem.entry.name,
        "input": input_json,
    })
    .to_string();

    let mut child = Command::new("python3")
        .arg(dir.join("runner.py"))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 python3 失败：{e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.as_bytes())
            .map_err(|e| format!("写子进程 stdin 失败：{e}"))?;
    }

    // 硬超时：超时即 kill，该 case 判失败（不挂起整轮）。
    let output = match wait_with_timeout(&mut child, Duration::from_secs(CASE_TIMEOUT_SECS)) {
        Some(o) => o,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(CaseOutcome {
                name: case.verifier_ref.clone(),
                passed: false,
                detail: format!("执行超时（>{CASE_TIMEOUT_SECS}s）"),
            });
        }
    };

    if !output.status.success() {
        // 子进程非 0 退出：runner 内部异常（如 import 候选失败 = 语法错误）。
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(CaseOutcome {
            name: case.verifier_ref.clone(),
            passed: false,
            detail: format!("子进程错误：{}", first_line(&stderr)),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let actual: Json = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("解析 runner 输出失败：{e}（输出：{}）", first_line(&stdout)))?;

    Ok(judge(case, &actual))
}

/// 把 runner 输出的实际结局与 `ExpectedOutcome` 对照（绝不伪造通过）。
fn judge(case: &HiddenCase, actual: &Json) -> CaseOutcome {
    let name = case.verifier_ref.clone();
    let ok = actual.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    match &case.expected {
        ExpectedOutcome::Returns(want) => {
            if !ok {
                let err = actual.get("error").and_then(|v| v.as_str()).unwrap_or("?");
                return CaseOutcome {
                    name,
                    passed: false,
                    detail: format!("期望返回值，实际抛异常 {err}"),
                };
            }
            let want_json = value_to_json(want);
            let got = actual.get("result").cloned().unwrap_or(Json::Null);
            let passed = json_eq(&want_json, &got);
            CaseOutcome {
                name,
                passed,
                detail: if passed {
                    format!("返回值匹配：{got}")
                } else {
                    format!("返回值不匹配：实际 {got}，期望 {want_json}")
                },
            }
        }
        ExpectedOutcome::Raises(want_variant) => {
            if ok {
                let got = actual.get("result").cloned().unwrap_or(Json::Null);
                return CaseOutcome {
                    name,
                    passed: false,
                    detail: format!("期望抛 {want_variant}，实际返回 {got}"),
                };
            }
            let err = actual.get("error").and_then(|v| v.as_str()).unwrap_or("");
            let passed = err == want_variant;
            CaseOutcome {
                name,
                passed,
                detail: if passed {
                    format!("异常类名匹配：{err}")
                } else {
                    format!("异常类名不匹配：实际 {err}，期望 {want_variant}")
                },
            }
        }
    }
}

/// 数值/结构相等比较。JSON 数字可能是 i64 / f64，统一按 f64 比（题集只用整数，安全）。
fn json_eq(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(x), Json::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(xf), Some(yf)) => (xf - yf).abs() < f64::EPSILON,
            _ => x == y,
        },
        (Json::Array(xs), Json::Array(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| json_eq(x, y))
        }
        (Json::Object(xs), Json::Object(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .all(|(k, xv)| ys.get(k).is_some_and(|yv| json_eq(xv, yv)))
        }
        _ => a == b,
    }
}

/// 有界等待子进程：超时返回 `None`（调用方负责 kill）。轮询而非阻塞 wait，保持可移植。
fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<std::process::Output> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                // 已退出：收集输出（stdout/stderr 已被 piped）。
                return child_output(child);
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
}

/// 退出后收集 piped 输出（take 走管道读尽）。
fn child_output(child: &mut std::process::Child) -> Option<std::process::Output> {
    use std::io::Read;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_end(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_end(&mut stderr);
    }
    let status = child.wait().ok()?;
    Some(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// 临时目录守卫：Drop 时递归清理（用后即清，设计 §二 卫生）。
struct DirGuard(std::path::PathBuf);
impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// benchmark 拥有的 runner 夹具（**非 LLM 产出**）。从 stdin 读 `{entry,input}`，`import`
/// 候选模块、调用 `run_action(input)`，把结局以 JSON 打到 stdout：成功
/// `{"ok":true,"result":...}`、抛异常 `{"ok":false,"error":"<异常类名>"}`。
/// hidden case 的**期望**绝不进此脚本（只传输入）——判定在 Rust 侧事后比对。
const RUNNER_PY: &str = r#"import sys, json

def main():
    req = json.load(sys.stdin)
    inp = req.get("input", {})
    try:
        import candidate
    except Exception as e:
        # 候选无法导入（语法错误等）：作为子进程非 0 退出，由 Rust 侧归因。
        sys.stderr.write("import candidate failed: %s\n" % e)
        sys.exit(2)
    try:
        result = candidate.run_action(inp)
        sys.stdout.write(json.dumps({"ok": True, "result": result}))
    except Exception as e:
        sys.stdout.write(json.dumps({"ok": False, "error": type(e).__name__}))

if __name__ == "__main__":
    main()
"#;
