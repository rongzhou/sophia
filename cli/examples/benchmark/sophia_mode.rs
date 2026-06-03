//! `sophia` mode：经 Sophia 工作流产出候选 `.sophia`，再用 v0 解释器逐 hidden case 判定
//! （见 docs/benchmark_test.md §一 / §五）。
//!
//! 设计纪律：**不复用 e2e harness**（设计 §七，先不抽象）。本文件自带一份精简闭环
//! （design → implement-loop → `runtime::verify`），与 e2e harness 在**纪律上一致**但代码独立：
//! - 防答案泄漏：design 不注入语法基线（semantics > format），implement / repair 注入共享
//!   语法基线（与 e2e 完全同一份 `prompt::preamble("sophia_syntax_baseline")` 资产）；
//!   hidden cases 绝不进 prompt（prompt 只由 `PublicBrief` 渲染）。
//! - 判定复用 `runtime::verify::run_hidden_cases`：零新增执行能力。
//! - 失败如实归因，绝不伪造通过。

use std::time::Instant;

use sophia_engine::{
    design_solution, run_implement_loop, GoalProgress, ImplementLoopConfig, ImplementLoopOutcome,
    LoopStepOutcome, StepPrompts,
};
use sophia_graph_db::{
    AcceptanceCriterionPayload, ActiveContext, DiagnosticItem, DiagnosticPayload, EdgeKind,
    GraphStore, NodeId, ObjectivePayload,
};
use sophia_llm::{CompletionRequest, LlmClient, StructuredConfig};
use sophia_prompt::PromptRegistry;
use sophia_runtime::{run_hidden_cases, VerificationResult};

use crate::problem::Problem;
use crate::report::{CaseOutcome, RunRecord};

/// sophia mode 的修复预算。复杂组合题偶尔需要多轮确定性诊断才能收敛；题面仍不注入语法。
const MAX_REPAIRS: u32 = 4;

/// 运行一道题的 sophia mode，返回结构化记录。计时口径见设计 §五：只计工作流（design +
/// implement + repair + check）的墙钟，**不计** hidden case 判定执行本身。
pub async fn run<C: LlmClient>(client: &C, model: &str, problem: &Problem) -> RunRecord {
    let started = Instant::now();
    let drive = drive_workflow(client, model, problem).await;
    let wall_time_ms = started.elapsed().as_millis();

    match drive {
        Ok(files) => {
            // 判定不计入 wall_time。
            verify_candidate(problem, &files, model, wall_time_ms)
        }
        Err(failure) => RunRecord {
            id: problem.id.to_string(),
            level: problem.level.as_str().to_string(),
            mode: crate::report::Mode::Sophia,
            language: None,
            model: model.to_string(),
            passed: false,
            wall_time_ms,
            failure: Some(failure),
            cases: Vec::new(),
        },
    }
}

/// 驱动 design → implement-loop，返回候选文件或失败简述。
async fn drive_workflow<C: LlmClient>(
    client: &C,
    model: &str,
    problem: &Problem,
) -> Result<Vec<(String, String)>, String> {
    let mut store = GraphStore::open_in_memory().map_err(|e| format!("建图失败：{e}"))?;
    let prompt = PromptRegistry::new();

    // 种下人类目标 + 验收条件（题面；provenance=human，隐式 bound）。
    let brief = problem.public_brief();
    let objective = store
        .as_human()
        .objective(
            problem.title,
            ObjectivePayload {
                title: problem.title.to_string(),
                description: problem.prompt_goal.to_string(),
            },
        )
        .map_err(|e| format!("建目标失败：{e}"))?;
    for (i, line) in brief.entry_contract_lines().iter().enumerate() {
        let ac = store
            .as_human()
            .acceptance_criterion(
                format!("ac{i}"),
                AcceptanceCriterionPayload {
                    statement: line.clone(),
                    verifier: None,
                },
            )
            .map_err(|e| format!("建验收条件失败：{e}"))?;
        store
            .append_edge(objective, ac, EdgeKind::ValidatedBy)
            .map_err(|e| format!("连验收边失败：{e}"))?;
    }

    let prompts = BenchPrompts::new(&prompt, model, problem);

    // design：请求据 active context 在调用时刻渲染（不注入语法基线）。
    let pseudo = match design_solution(
        &mut store,
        client,
        |ctx: &ActiveContext| prompts.design(ctx, objective),
        &StructuredConfig::default(),
        objective,
    )
    .await
    .map_err(|e| format!("design 调用失败：{e}"))?
    {
        LoopStepOutcome::Succeeded(art) => art,
        LoopStepOutcome::Failed { error, .. } => return Err(format!("design 失败：{error}")),
    };
    println!("    [design] 伪代码 {} 字节", pseudo.text.len());

    // implement-loop：注入共享语法基线（system prompt）+ 真实 code_check。
    let config = ImplementLoopConfig {
        max_repair_attempts: MAX_REPAIRS,
        structured: StructuredConfig::default(),
    };
    let outcome = run_implement_loop(
        &mut store,
        client,
        &prompts,
        &config,
        objective,
        pseudo.node,
        &pseudo.text,
        &pseudo.libraries,
        real_check,
    )
    .await
    .map_err(|e| format!("implement-loop 失败：{e}"))?;

    match outcome {
        ImplementLoopOutcome::Passed { files, .. } => Ok(files),
        ImplementLoopOutcome::BudgetExhausted { attempts, .. } => {
            Err(format!("check 在 {attempts} 次尝试内未收敛"))
        }
        ImplementLoopOutcome::Failed { error, .. } => Err(format!("implement 失败：{error}")),
    }
}

/// 候选语义检查通过后，用 v0 解释器逐 hidden case 判定（复用 `runtime::verify`）。
fn verify_candidate(
    problem: &Problem,
    files: &[(String, String)],
    model: &str,
    wall_time_ms: u128,
) -> RunRecord {
    use sophia_hir::{AsgIndex, IndexInput};
    use sophia_semantic::analyze_program;
    use sophia_syntax::{parse_ast, Ast};

    let mk = |passed: bool, failure: Option<String>, cases: Vec<CaseOutcome>| RunRecord {
        id: problem.id.to_string(),
        level: problem.level.as_str().to_string(),
        mode: crate::report::Mode::Sophia,
        language: None,
        model: model.to_string(),
        passed,
        wall_time_ms,
        failure,
        cases,
    };

    // 解析 + 建 index + 语义分析（候选已过 check，这里再建模型供解释器执行）。
    let asts: Result<Vec<Ast>, String> = files
        .iter()
        .map(|(_, c)| parse_ast(c).map_err(|e| format!("解析候选失败：{e}")))
        .collect();
    let asts = match asts {
        Ok(a) => a,
        Err(e) => return mk(false, Some(e), Vec::new()),
    };
    let inputs: Vec<IndexInput> = files
        .iter()
        .zip(&asts)
        .map(|((path, _), ast)| IndexInput {
            domain: Box::leak(sophia_engine::domain_of_path(path).into_boxed_str()),
            path: Box::leak(path.clone().into_boxed_str()),
            ast,
        })
        .collect();
    let index = match AsgIndex::build(inputs, &sophia_stdlib::standard_registry()) {
        Ok(i) => i,
        Err(e) => return mk(false, Some(format!("构建 index 失败：{e:?}")), Vec::new()),
    };
    let refs: Vec<&Ast> = asts.iter().collect();
    let analysis = analyze_program(&refs, &index);
    if !analysis.diagnostics.is_empty() {
        return mk(
            false,
            Some(format!("候选语义检查未过：{:?}", analysis.diagnostics)),
            Vec::new(),
        );
    }

    // 逐 hidden case 判定（复用 runtime::verify；绝不伪造通过）。benchmark 题集全为纯逻辑、
    // 无 IO、确定（网络 / 文件题不入 benchmark，见 problems.rs），故统一用默认执行路径。
    let results: Vec<VerificationResult> =
        run_hidden_cases(&analysis.model, &refs, &problem.hidden_cases);
    let cases: Vec<CaseOutcome> = results
        .iter()
        .map(|r| CaseOutcome {
            name: r.verifier_ref.clone(),
            passed: r.passed,
            detail: r.detail.clone(),
        })
        .collect();
    let all_passed = !cases.is_empty() && cases.iter().all(|c| c.passed);
    let failure = if all_passed {
        None
    } else {
        Some("部分 hidden case 未通过".to_string())
    };
    mk(all_passed, failure, cases)
}

// ---- 注入的确定性 code_check（委派 engine::code_check，与 e2e / CLI 共享单一实现）----

fn real_check(files: &[(String, String)]) -> DiagnosticPayload {
    let payload = sophia_engine::code_check(files);
    // 可观测性：打印每轮 check 结果，便于定位 sophia mode 的收敛过程（与 e2e 一致）。
    if payload.ok {
        println!("    [check] 通过（{} 个文件）", files.len());
    } else {
        println!("    [check] {} 条诊断：", payload.diagnostics.len());
        for d in &payload.diagnostics {
            println!(
                "      [{}] {}{}",
                d.code,
                d.problem,
                d.location
                    .as_ref()
                    .map(|l| format!("（{l}）"))
                    .unwrap_or_default()
            );
        }
    }
    payload
}

// ---- prompt 提供者（实现 engine StepPrompts；纪律与 e2e HarnessPrompts 一致）----

/// benchmark 的 prompt 提供者。题面只由 `PublicBrief` 渲染——hidden cases 在类型上无法
/// 流入（设计 §三 结构防线）。本 benchmark 不走 decision / decompose / revise 路径
/// （单步 design→implement，与 e2e 的 DesignImplement 类同），故那些方法以 unreachable 兜底。
struct BenchPrompts<'a> {
    prompt: &'a PromptRegistry,
    model: String,
    objective_text: String,
    /// 验收条件 = 入口契约行（题面，作为 constraints 语境）。
    acceptance: Vec<String>,
}

impl<'a> BenchPrompts<'a> {
    fn new(prompt: &'a PromptRegistry, model: &str, problem: &Problem) -> Self {
        let brief = problem.public_brief();
        let mut acceptance = brief.entry_contract_lines();
        acceptance.extend(brief.public_forbidden.iter().map(|s| s.to_string()));
        BenchPrompts {
            prompt,
            model: model.to_string(),
            objective_text: format!("{}：{}", problem.title, problem.prompt_goal),
            acceptance,
        }
    }
}

impl StepPrompts for BenchPrompts<'_> {
    fn decision(
        &self,
        _ctx: &ActiveContext,
        _focus: NodeId,
        _progress: GoalProgress,
    ) -> CompletionRequest {
        unreachable!("benchmark sophia mode 不经调度器决策路径")
    }

    fn decompose(&self, _ctx: &ActiveContext, _focus: NodeId) -> CompletionRequest {
        unreachable!("benchmark sophia mode 不经 decompose 路径")
    }

    fn design(&self, _ctx: &ActiveContext, _focus: NodeId) -> CompletionRequest {
        let rendered = self
            .prompt
            .render(
                "design_solution",
                serde_json::json!({
                    "objective": self.objective_text,
                    "constraints": self.acceptance,
                    "acceptance_criteria": self.acceptance,
                    "context_files": Vec::<String>::new(),
                    "stdlib_catalog": sophia_stdlib::standard_registry().catalog(),
                }),
            )
            .expect("渲染 design_solution 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(sophia_prompt::design_system_prompt());
        req
    }

    fn revise(
        &self,
        _ctx: &ActiveContext,
        _focus: NodeId,
        _pseudocode: &str,
        _diagnostics: &[DiagnosticItem],
    ) -> CompletionRequest {
        unreachable!("benchmark sophia mode 不经 revise 路径")
    }

    fn implement(
        &self,
        _ctx: &ActiveContext,
        _focus: NodeId,
        pseudocode: &str,
        libraries: &[String],
    ) -> CompletionRequest {
        let context_files = vec![format!("当前目标：{}", self.objective_text)];
        let rendered = self
            .prompt
            .render(
                "implement_design",
                serde_json::json!({
                    "pseudocode": pseudocode,
                    "context_files": context_files,
                    "constraints": self.acceptance,
                }),
            )
            .expect("渲染 implement_design 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        let stdlib_block = sophia_stdlib::standard_registry()
            .preamble(&libraries.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        req.system = Some(sophia_prompt::implement_system_prompt(&stdlib_block));
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
        let stdlib_block = sophia_stdlib::standard_registry()
            .preamble(&libraries.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        req.system = Some(sophia_prompt::implement_system_prompt(&stdlib_block));
        req
    }
}

// design / implement / repair 的 system prompt 文案统一在 `sophia_prompt`
// （`design_system_prompt` / `implement_system_prompt`，单一来源——纪律与 e2e 一致，见
// docs/benchmark_test.md §三：与 e2e 完全同一份 prompt 资产）。
