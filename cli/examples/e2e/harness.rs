//! e2e 用例共享 harness（见 docs/e2e_test.md §5）。
//!
//! 把"真实 LLM 后端构造 → design → implement → code_check → repair → v0 解释器执行"
//! 这条闭环固化为复用件，所有用例共享。新增用例只需在 `cases/` 加一个 [`Case`]，
//! 不必碰本模块。
//!
//! ## 防答案泄漏（最要紧，见设计文档 §2）
//!
//! - 语法基线取自**单一共享资产** `prompt::preamble("sophia_syntax_baseline")`，只含
//!   可泛化标准语法规则 + 中立示例，不含任何任务答案；由 prompt crate 的 snapshot +
//!   防泄漏断言测试守护。
//! - 语法基线只进 implement / repair 的 system prompt，**不进** design（伪代码 semantics
//!   > format）。
//! - 用例的"期望结果"与"待修坏候选"只存在于 harness 内部，**不喂给 LLM**。

use sophia_engine::{
    design_solution, repair_code, run_implement_loop, CodeArtifact, ImplementLoopConfig,
    ImplementLoopOutcome, LoopStepOutcome, StepPrompts,
};
use sophia_graph_db::{
    AcceptanceCriterionPayload, DiagnosticItem, DiagnosticPayload, EdgeKind, GraphStore, NodeId,
    ObjectivePayload,
};
use sophia_llm::{
    CompletionRequest, CompletionResponse, LlmClient, LlmError, LlmResult, StructuredConfig,
};
use sophia_prompt::PromptRegistry;
use sophia_runtime::Value;

/// 容忍后端请求失败的 LLM client 包装：仅对 `BackendUnavailable` 做有界
/// 重试 + 退避。
///
/// 设计说明：`complete_structured` 库层**故意不**重试 `BackendUnavailable`（避免放大后端
/// 不可用）。但 e2e harness 面对的是真实公网端点的偶发连接重置（实测 deepseek-flash 端点
/// 会偶发 "error sending request"），属瞬时抖动而非真实不可用，故在 harness 这一层做有界
/// 重试是合理的——它不改变库的语义，只是让 e2e 不被偶发抖动误判为失败。
struct RetryClient<C: LlmClient> {
    inner: C,
    max_attempts: u32,
}

#[async_trait::async_trait]
impl<C: LlmClient> LlmClient for RetryClient<C> {
    async fn complete(&self, req: &CompletionRequest) -> LlmResult<CompletionResponse> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match self.inner.complete(req).await {
                Ok(resp) => return Ok(resp),
                // 仅瞬时网络层不可用才重试；其它错误（解析/schema 等）立即上报。
                Err(LlmError::BackendUnavailable(msg)) if attempt < self.max_attempts => {
                    let backoff = std::time::Duration::from_millis(800 * attempt as u64);
                    eprintln!(
                        "    [retry] 第 {attempt} 次 LLM 请求失败（{msg}），{} ms 后重试…",
                        backoff.as_millis()
                    );
                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// 用有界重试包装一个后端 client。
pub fn with_retry<C: LlmClient>(inner: C, max_attempts: u32) -> impl LlmClient {
    RetryClient {
        inner,
        max_attempts: max_attempts.max(1),
    }
}

/// harness 的 prompt 提供者（实现 engine `StepPrompts`，见架构 §8.4）：在调用时刻据 active
/// context 渲染各步请求。design 阶段**不注入**语法基线（伪代码 semantics > format）；
/// implement / repair 阶段注入共享语法基线（决断性事实，无任务答案）。
struct HarnessPrompts<'a> {
    prompt: &'a PromptRegistry,
    model: String,
    /// 任务验收条件（题目；作为 design / implement 的 constraints 语境）。
    acceptance: &'static [&'static str],
    /// 任务的业务化目标（design 的 objective 语境；作为 focus 不在 active context 时的回退）。
    objective_text: String,
    /// 根焦点目标节点。只有根焦点（且尚无伪代码）才会被提供 `decompose` 候选动作——
    /// 避免子目标无限拆解。
    root_focus: NodeId,
    /// 是否允许根焦点选择 `decompose`（Tree 类用例为真；其它类型为假，保持既有行为）。
    allow_decompose: bool,
}

impl HarnessPrompts<'_> {
    /// 焦点目标的业务化文本：优先从 active context 的 bound_objectives 按 focus id 取
    /// （树遍历中子目标获得自己的题面）；focus 不在其中时回退到根目标文本。
    ///
    /// 对根焦点而言，root objective 本就在 bound_objectives 中，故产出与 `objective_text`
    /// 逐字相同——G1–G4 / R / G3（均以根为焦点）行为不变。
    fn focus_objective_text(&self, ctx: &sophia_graph_db::ActiveContext, focus: NodeId) -> String {
        ctx.bound_objectives
            .iter()
            .find(|o| o.id == focus)
            .map(|o| format!("{}：{}", o.title, o.description))
            .unwrap_or_else(|| self.objective_text.clone())
    }

    /// 当前 focus 的验收条件。
    ///
    /// 用例级 acceptance 是根目标的验收边界。Tree 用例拆出子目标后，子目标自己的
    /// Objective 描述就是局部规格；继续注入根 acceptance 会让每个子目标都实现整棵树。
    fn focus_acceptance(&self, focus: NodeId) -> Vec<&'static str> {
        if focus == self.root_focus {
            self.acceptance.to_vec()
        } else {
            Vec::new()
        }
    }
}

impl sophia_engine::StepPrompts for HarnessPrompts<'_> {
    fn decision(
        &self,
        ctx: &sophia_graph_db::ActiveContext,
        focus: NodeId,
        progress: sophia_engine::GoalProgress,
    ) -> CompletionRequest {
        // 根焦点且尚无伪代码、且本用例允许拆解时，把 decompose 也列入候选（Tree 类用例）。
        let offer_decompose =
            self.allow_decompose && focus == self.root_focus && !progress.has_pseudocode;
        let candidate_actions: Vec<&str> = if progress.last_implement_failed {
            vec!["revise_design", "implement_design"]
        } else if progress.has_pseudocode {
            vec!["implement_design", "design_solution"]
        } else if offer_decompose {
            vec!["decompose", "design_solution"]
        } else {
            vec!["design_solution"]
        };
        let rendered = self
            .prompt
            .render(
                "decision",
                serde_json::json!({
                    "focus_summary": self.focus_objective_text(ctx, focus),
                    "bound_objective_count": ctx.bound_objectives.len(),
                    "active_milestone": ctx.active_milestone.as_ref().map(|m| m.name.clone()),
                    "outstanding_questions": ctx.outstanding_questions.len(),
                    // 祖先链 / 诊断在本 spine 暂以进度摘要承载（演进状态可见，避免原地打转）。
                    "ancestors": vec![format!(
                        "已设计伪代码：{}；已用决策 {}/{}",
                        progress.has_pseudocode, progress.decisions_used, progress.max_decisions
                    )],
                    "diagnostics": Vec::<serde_json::Value>::new(),
                    "budget": {
                        "remaining_depth": progress.remaining_decisions(),
                        "repair_attempts": 0,
                    },
                    // 候选动作受当前进度约束，引导但不替 LLM 决策（design 10.8）。
                    "candidate_actions": candidate_actions,
                }),
            )
            .expect("渲染 decision 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(decision_system_prompt(offer_decompose));
        req
    }

    fn design(&self, ctx: &sophia_graph_db::ActiveContext, focus: NodeId) -> CompletionRequest {
        // design 产出语义伪代码：**不注入语法基线**（semantics > format）。
        let acceptance = self.focus_acceptance(focus);
        let rendered = self
            .prompt
            .render(
                "design_solution",
                serde_json::json!({
                    "objective": self.focus_objective_text(ctx, focus),
                    "constraints": Vec::<String>::new(),
                    "acceptance_criteria": acceptance,
                    "context_files": Vec::<String>::new(),
                    "stdlib_catalog": sophia_stdlib::standard_registry().catalog(),
                }),
            )
            .expect("渲染 design_solution 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(sophia_prompt::design_system_prompt());
        req
    }

    fn decompose(&self, ctx: &sophia_graph_db::ActiveContext, focus: NodeId) -> CompletionRequest {
        // decompose 属语义 / 目标层（非代码）：**不注入语法基线**（无关 .sophia 格式）。
        let constraints = self.focus_acceptance(focus);
        let rendered = self
            .prompt
            .render(
                "decompose",
                serde_json::json!({
                    "objective": self.focus_objective_text(ctx, focus),
                    "constraints": constraints,
                }),
            )
            .expect("渲染 decompose 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(
            "你是 Sophia 工作流的拆解者。只输出严格符合 decompose_result schema 的 JSON 对象，\
             含两个字段：rationale（字符串）、children（至少 2 个对象的数组，每个含 title 与 \
             description 两个字符串字段）。每个子目标应是一个可独立推进、可观察验收的业务目标。\
             不要提前指定 Sophia 语法、语言构件或文件布局。\
             不要输出 markdown 围栏或额外说明。"
                .to_string(),
        );
        req
    }

    fn revise(
        &self,
        ctx: &sophia_graph_db::ActiveContext,
        focus: NodeId,
        pseudocode: &str,
        diagnostics: &[DiagnosticItem],
    ) -> CompletionRequest {
        // revise 同属语义伪代码阶段：**不注入语法基线**；据概念性诊断重写伪代码。
        let constraints = self.focus_acceptance(focus);
        let rendered = self
            .prompt
            .render(
                "revise_design",
                serde_json::json!({
                    "pseudocode": pseudocode,
                    "diagnostics": diagnostics,
                    "objective": self.focus_objective_text(ctx, focus),
                    "constraints": constraints,
                }),
            )
            .expect("渲染 revise_design 模板失败");
        let mut req = CompletionRequest::new(&self.model, rendered);
        req.system = Some(sophia_prompt::design_system_prompt());
        req
    }

    fn implement(
        &self,
        ctx: &sophia_graph_db::ActiveContext,
        focus: NodeId,
        pseudocode: &str,
        libraries: &[String],
    ) -> CompletionRequest {
        let constraints = self.focus_acceptance(focus);
        let context_files = vec![format!(
            "当前目标：{}",
            self.focus_objective_text(ctx, focus)
        )];
        let rendered = self
            .prompt
            .render(
                "implement_design",
                serde_json::json!({
                    "pseudocode": pseudocode,
                    "context_files": context_files,
                    "constraints": constraints,
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
        _ctx: &sophia_graph_db::ActiveContext,
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

/// 用例的驱动方式（决定 harness 走哪条路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// G1/G2：直接 design → implement-loop（一次过或带修复预算）。
    DesignImplement,
    /// R：从坏候选起步跑 repair 闭环（由 `broken_seed` 提供起点）。
    RepairSeed,
    /// G3：经**调度器** `run_goal_loop`——LLM 自主决策 decision→design→implement 序列，
    /// 验证能自主推进到可物化候选（考察启发式编排，非单步）。
    Scheduler,
    /// G6：经**目标树遍历层** `run_goal_tree`——LLM 自主 `decompose` 把根目标拆成多个子目标，
    /// 每个子目标经 spine 各自推进到候选（含人类授权检查点 + binding 继承，design 5.3 / 10.9）。
    Tree,
}

/// 用例对执行结果的期望（执行后对照；**非 LLM 输入**）。
#[derive(Clone)]
pub enum Expect {
    /// 期望正常返回某值。
    Returns(Value),
    /// 期望 raise 某领域错误 variant（按 variant 名对照）。
    Raises(&'static str),
}

/// 一条 e2e 用例（题目 + 入口 + 期望；可选待修坏候选）。
///
/// **不含任何 Sophia 源码答案**——`expect` 仅用于执行后对照，不喂 LLM；`broken_seed`
/// 是"题目里待修的东西"（R 类用例），同样不构成对答案的提示。
pub struct Case {
    /// 稳定用例 ID（如 `G1-01`）。
    pub id: &'static str,
    /// 所属组（如 `g1`）。
    pub group: &'static str,
    /// 驱动方式。
    pub kind: CaseKind,
    /// 业务化的目标标题。
    pub title: &'static str,
    /// 业务化的需求描述（题目）。
    pub description: &'static str,
    /// 验收条件（题目）。
    pub acceptance: &'static [&'static str],
    /// 执行入口 action 名。
    pub entry_action: &'static str,
    /// 入口实参（按 input 顺序）。
    pub args: Vec<Value>,
    /// 期望的执行结果（返回值或 raise；执行后对照，非 LLM 输入）。
    pub expect: Expect,
    /// 可选：期望的 console 输出行（按顺序）。`None` 表示不校验 console。
    /// 用于 G2 等需要验证 effect 真正执行的用例（非 LLM 输入）。
    pub expected_console: Option<Vec<String>>,
    /// 可选：执行后检查第一个实参指向的真实文件内容。用于 File e2e 的 hidden verifier，
    /// 不喂给 LLM。
    pub expected_file_content: Option<String>,
    /// 修复预算：0 = 要求一次过（G1/G2）；>0 = 修复闭环（R）。
    pub max_repairs: u32,
    /// 可选：待修的坏候选（path, content）。给定则跳过 design/implement，直接从它起步跑
    /// repair 闭环（R 类用例）。这是"题目"，由 harness 内部构造，不喂任何答案提示。
    pub broken_seed: Option<(&'static str, &'static str)>,
}

/// 解释执行阶段所需的 owned 用例切片。
///
/// e2e 的 LLM 驱动运行在 Tokio async 外壳内；真实 `Http.Get` host 使用
/// `reqwest::blocking`，必须在 blocking 线程中完整创建、执行、析构，避免在 async runtime
/// 上下文中 drop 其内部 runtime。
struct ExecutionSpec {
    entry_action: String,
    args: Vec<Value>,
    expect: Expect,
    expected_console: Option<Vec<String>>,
    expected_file_content: Option<String>,
}

impl ExecutionSpec {
    fn from_case(case: &Case) -> Self {
        ExecutionSpec {
            entry_action: case.entry_action.to_string(),
            args: case.args.clone(),
            expect: case.expect.clone(),
            expected_console: case.expected_console.clone(),
            expected_file_content: case.expected_file_content.clone(),
        }
    }
}

/// 用例运行结果。
pub struct CaseReport {
    pub id: &'static str,
    pub passed: bool,
    pub detail: String,
}

/// 运行一条用例，返回报告（不 panic：失败也作为 `CaseReport` 返回，便于汇总）。
pub async fn run_case<C: LlmClient>(client: &C, model: &str, case: &Case) -> CaseReport {
    println!("\n──────────── [{}] {} ────────────", case.id, case.title);
    println!("题目：{}", case.description);
    match run_case_inner(client, model, case).await {
        Ok(true) => {
            println!("✓ [{}] PASS", case.id);
            CaseReport {
                id: case.id,
                passed: true,
                detail: "ok".into(),
            }
        }
        Ok(false) => {
            println!("✗ [{}] FAIL（未达成功判据）", case.id);
            CaseReport {
                id: case.id,
                passed: false,
                detail: "未达成功判据".into(),
            }
        }
        Err(e) => {
            println!("✗ [{}] ERROR：{e}", case.id);
            CaseReport {
                id: case.id,
                passed: false,
                detail: e.to_string(),
            }
        }
    }
}

async fn run_case_inner<C: LlmClient>(
    client: &C,
    model: &str,
    case: &Case,
) -> anyhow::Result<bool> {
    let mut store = GraphStore::open_in_memory()?;
    let prompt = PromptRegistry::new();

    // 种下人类目标 + 验收条件（provenance=human，隐式 bound）。
    let objective = store.as_human().objective(
        case.title,
        ObjectivePayload {
            title: case.title.to_string(),
            description: case.description.to_string(),
        },
    )?;
    for (i, a) in case.acceptance.iter().enumerate() {
        let ac = store.as_human().acceptance_criterion(
            format!("ac{i}"),
            AcceptanceCriterionPayload {
                statement: a.to_string(),
                verifier: None,
            },
        )?;
        store.append_edge(objective, ac, EdgeKind::ValidatedBy)?;
    }

    // R 类用例：从坏候选起步跑 repair 闭环；否则跑 design → implement-loop。
    let files = match case.kind {
        CaseKind::Scheduler => {
            scheduler_drive(&mut store, client, model, &prompt, objective, case).await?
        }
        CaseKind::Tree => tree_drive(&mut store, client, model, &prompt, objective, case).await?,
        CaseKind::RepairSeed => {
            let (path, content) = case
                .broken_seed
                .expect("RepairSeed 用例必须提供 broken_seed");
            repair_from_seed(
                &mut store,
                client,
                model,
                &prompt,
                objective,
                case,
                path,
                content,
                case.max_repairs,
            )
            .await?
        }
        CaseKind::DesignImplement => {
            design_then_implement(&mut store, client, model, &prompt, objective, case).await?
        }
    };

    let Some(files) = files else {
        return Ok(false);
    };

    // check 通过后用 v0 解释器执行，对照期望。真实 File/Http host 是同步 blocking
    // host；放到 Tokio blocking 线程里运行，避免 `reqwest::blocking` 在 async 上下文析构。
    let spec = ExecutionSpec::from_case(case);
    tokio::task::spawn_blocking(move || execute_and_check(&files, &spec))
        .await
        .map_err(|e| anyhow::anyhow!("解释执行线程失败：{e}"))?
}

/// 构造 harness prompt 提供者。
fn harness_prompts<'a>(
    prompt: &'a PromptRegistry,
    model: &str,
    case: &Case,
    root_focus: NodeId,
    allow_decompose: bool,
) -> HarnessPrompts<'a> {
    HarnessPrompts {
        prompt,
        model: model.to_string(),
        acceptance: case.acceptance,
        objective_text: format!("{}：{}", case.title, case.description),
        root_focus,
        allow_decompose,
    }
}

/// G3 路径：经调度器 `run_goal_loop` 让 **LLM 自主决策** decision→design→implement 序列，
/// 验证能自主推进到可物化候选（考察启发式编排，非单步）。
///
/// 与 G1/G2 不同：不由 harness 硬编码 design→implement 的顺序，而是把决策权交给 LLM——
/// 每轮先产 DecisionNode（`considers→ 焦点`），再据 `selected_action` 分派。每步请求都由
/// `HarnessPrompts` 在调用时刻据当前 active context + 进度渲染（§8.4）。
async fn scheduler_drive<C: LlmClient>(
    store: &mut GraphStore,
    client: &C,
    model: &str,
    prompt: &PromptRegistry,
    objective: NodeId,
    case: &Case,
) -> anyhow::Result<Option<Vec<(String, String)>>> {
    use sophia_engine::{run_goal_loop, LibrarySelectionPolicy, Outcome, SchedulerBudget};

    let prompts = harness_prompts(prompt, model, case, objective, false);
    let budget = SchedulerBudget::default();
    let library_policy =
        LibrarySelectionPolicy::from_names(sophia_stdlib::standard_registry().lib_names());

    let outcome = run_goal_loop(
        store,
        client,
        &prompts,
        &budget,
        &library_policy,
        objective,
        real_check,
    )
    .await?;

    match outcome {
        Outcome::CandidateReady {
            code,
            files,
            decisions,
        } => {
            println!("[scheduler] {decisions} 轮决策后产出可物化候选 {code}");
            print_files(&files);
            Ok(Some(files))
        }
        Outcome::Yielded {
            action, decisions, ..
        } => {
            println!(
                "[scheduler] {decisions} 轮后让位高层动作 {action:?}（本 spine 不实现其语义）"
            );
            Ok(None)
        }
        Outcome::BudgetExhausted { reason, decisions } => {
            println!("[scheduler] 预算耗尽（{decisions} 轮）：{reason}");
            Ok(None)
        }
        Outcome::Failed { raw_llm, error } => {
            anyhow::bail!("scheduler 决策/执行失败（RawLlmNode {raw_llm}）：{error}");
        }
    }
}

/// G6 路径：经目标树遍历层 `run_goal_tree` 让 **LLM 自主 decompose** 把根目标拆成多个子目标，
/// 每个子目标经 spine 各自推进到候选，最后**合并所有候选文件**作为整程序执行。
///
/// 与 G3（单目标 spine）不同：这里考察非线性树推进 + 人类授权检查点（design 5.3 / N4）——
/// decompose 落图后由 `AutoAcceptReviewer` 代表人类接受（真实 AcceptanceEvent 落图），子目标
/// 沿 `member_of` 继承 binding 进入各自的 active context，从而 design/implement 能看到自己的
/// 子目标题面（这正是本轮 harness focus-aware 改造解决的缺口）。harness 即代表人类操作员，
/// 故用 auto-accept；真人 CLI 应交互式审查。
async fn tree_drive<C: LlmClient>(
    store: &mut GraphStore,
    client: &C,
    model: &str,
    prompt: &PromptRegistry,
    objective: NodeId,
    case: &Case,
) -> anyhow::Result<Option<Vec<(String, String)>>> {
    use sophia_engine::{
        run_goal_tree, AutoAcceptReviewer, GoalTreeConfig, LibrarySelectionPolicy, TreeBudget,
    };

    // Tree 用例：允许根焦点选择 decompose。
    let prompts = harness_prompts(prompt, model, case, objective, true);
    let config = GoalTreeConfig {
        budget: TreeBudget::default(),
        library_policy: LibrarySelectionPolicy::from_names(
            sophia_stdlib::standard_registry().lib_names(),
        ),
    };
    let mut reviewer = AutoAcceptReviewer;

    let resolution = run_goal_tree(
        store,
        client,
        &prompts,
        &mut reviewer,
        &config,
        objective,
        real_check,
    )
    .await?;

    // 收集整棵树的候选（深度优先）。完全归结才合并执行；否则如实报告未达。
    let candidates = resolution.candidates();
    println!(
        "[tree] 归结完成：{} 个候选（fully_resolved={}）",
        candidates.len(),
        resolution.is_fully_resolved()
    );
    if !resolution.is_fully_resolved() || candidates.is_empty() {
        describe_unresolved(&resolution);
        return Ok(None);
    }

    // 合并所有子目标候选文件为一个程序（各子目标产出不同 domain/action 的文件）。
    let mut merged: Vec<(String, String)> = Vec::new();
    for (focus, code, files) in candidates {
        println!(
            "[tree] 焦点 {focus} → 候选 {code}（{} 个文件）",
            files.len()
        );
        for (path, content) in files {
            if merged.iter().any(|(p, _)| p == path) {
                anyhow::bail!("子目标候选文件路径冲突：{path}（拆解应产出互不重叠的 action）");
            }
            merged.push((path.clone(), content.clone()));
        }
    }
    print_files(&merged);
    Ok(Some(merged))
}

/// 打印未完全归结的树归结摘要（便于定位失败子目标）。
fn describe_unresolved(res: &sophia_engine::GoalResolution) {
    use sophia_engine::GoalResolution as G;
    match res {
        G::Candidate { focus, .. } => println!("    候选：焦点 {focus}"),
        G::Decomposed { children, .. } => {
            for c in children {
                describe_unresolved(c);
            }
        }
        G::DecompositionRejected { focus, reason, .. } => {
            println!("    焦点 {focus} 拆解被拒：{reason}")
        }
        G::Backtracked { focus } => println!("    焦点 {focus} 被 backtrack 放弃"),
        G::Yielded { focus, action, .. } => println!("    焦点 {focus} 让位高层动作 {action:?}"),
        G::BudgetExhausted { focus, reason } => println!("    焦点 {focus} 预算耗尽：{reason}"),
        G::Failed { focus, error, .. } => println!("    焦点 {focus} LLM 失败：{error}"),
    }
}

/// design → implement-loop 路径（G1/G2 一次过用例）。
async fn design_then_implement<C: LlmClient>(
    store: &mut GraphStore,
    client: &C,
    model: &str,
    prompt: &PromptRegistry,
    objective: NodeId,
    case: &Case,
) -> anyhow::Result<Option<Vec<(String, String)>>> {
    let prompts = harness_prompts(prompt, model, case, objective, false);

    // design：请求在调用时刻据 active context 渲染（不注入语法基线）。
    let pseudo = match design_solution(
        store,
        client,
        |ctx: &sophia_graph_db::ActiveContext| prompts.design(ctx, objective),
        &StructuredConfig::default(),
        &sophia_engine::LibrarySelectionPolicy::from_names(
            sophia_stdlib::standard_registry().lib_names(),
        ),
        objective,
    )
    .await?
    {
        LoopStepOutcome::Succeeded(art) => {
            println!("[design] → {}（{} 字节伪代码）", art.node, art.text.len());
            art
        }
        LoopStepOutcome::Failed { raw_llm, error } => {
            anyhow::bail!("design 失败（RawLlmNode {raw_llm}）：{error}");
        }
    };

    // implement-loop：注入语法基线（system prompt）+ 真实 code_check。
    let config = ImplementLoopConfig {
        max_repair_attempts: case.max_repairs,
        structured: StructuredConfig::default(),
    };
    let outcome = run_implement_loop(
        store,
        client,
        &prompts,
        &config,
        objective,
        pseudo.node,
        &pseudo.text,
        &pseudo.libraries,
        real_check,
    )
    .await?;

    match outcome {
        ImplementLoopOutcome::Passed {
            code,
            files,
            attempts,
            ..
        } => {
            println!("[implement] 通过（{attempts} 次尝试）→ {code}");
            if case.max_repairs == 0 && attempts > 1 {
                println!("  注意：要求一次过，但用了 {attempts} 次尝试");
                return Ok(None);
            }
            print_files(&files);
            Ok(Some(files))
        }
        ImplementLoopOutcome::BudgetExhausted {
            last_code,
            attempts,
            ..
        } => {
            println!("[implement] 预算耗尽（{attempts} 次尝试未通过 check）；最后候选 {last_code}");
            Ok(None)
        }
        ImplementLoopOutcome::Failed { raw_llm, error } => {
            anyhow::bail!("implement 失败（RawLlmNode {raw_llm}）：{error}");
        }
    }
}

/// 从坏候选起步的 repair 闭环（R 类用例）。
#[allow(clippy::too_many_arguments)]
async fn repair_from_seed<C: LlmClient>(
    store: &mut GraphStore,
    client: &C,
    model: &str,
    prompt: &PromptRegistry,
    objective: NodeId,
    case: &Case,
    path: &str,
    content: &str,
    max_repairs: u32,
) -> anyhow::Result<Option<Vec<(String, String)>>> {
    use sophia_graph_db::{snapshot_payload, CodePayload};

    let prompts = harness_prompts(prompt, model, case, objective, false);
    let mut files = vec![(path.to_string(), content.to_string())];
    let snapshot = {
        let ctx = sophia_graph_db::derive_active_context(store);
        store
            .as_deterministic()
            .context_snapshot("snap:seed", snapshot_payload(&ctx))?
    };
    let mut current_code = store.as_llm().code(
        "code:broken-seed",
        CodePayload {
            files: files.iter().map(|(p, _)| p.clone()).collect(),
        },
    )?;
    store.append_edge(current_code, snapshot, EdgeKind::Consumed)?;
    store.append_edge(current_code, objective, EdgeKind::Addresses)?;
    println!("[seed] 坏候选 {current_code}：");
    print_files(&files);

    let mut repairs_done = 0u32;
    loop {
        let payload = real_check(&files);
        if payload.ok {
            return Ok(Some(files));
        }
        if repairs_done >= max_repairs {
            println!("[repair] 预算耗尽（已修 {repairs_done} 次）");
            return Ok(None);
        }
        let diagnostics = payload.diagnostics.clone();
        let prev_files = files.clone();
        repairs_done += 1;
        let artifact: CodeArtifact = match repair_code(
            store,
            client,
            |ctx: &sophia_graph_db::ActiveContext| {
                prompts.repair(ctx, objective, &prev_files, &diagnostics, &[])
            },
            &StructuredConfig::default(),
            objective,
            current_code,
        )
        .await?
        {
            LoopStepOutcome::Succeeded(a) => a,
            LoopStepOutcome::Failed { raw_llm, error } => {
                anyhow::bail!("repair 失败（RawLlmNode {raw_llm}）：{error}");
            }
        };
        current_code = artifact.node;
        files = artifact.files;
        println!("[repair {repairs_done}] → 新候选 {current_code}：");
        print_files(&files);
    }
}

/// check 通过后用 v0 解释器执行，对照期望返回值。
fn execute_and_check(files: &[(String, String)], spec: &ExecutionSpec) -> anyhow::Result<bool> {
    use sophia_hir::{AsgIndex, IndexInput};
    use sophia_semantic::analyze_program;
    use sophia_syntax::{parse_ast, Ast};

    let asts: Vec<Ast> = files
        .iter()
        .map(|(_, c)| parse_ast(c).map_err(|e| anyhow::anyhow!("解析候选失败：{e}")))
        .collect::<anyhow::Result<_>>()?;
    let inputs: Vec<IndexInput> = files
        .iter()
        .zip(&asts)
        .map(|((path, _), ast)| IndexInput {
            domain: Box::leak(sophia_engine::domain_of_path(path).into_boxed_str()),
            path: Box::leak(path.clone().into_boxed_str()),
            ast,
        })
        .collect();
    let index = AsgIndex::build(inputs, &sophia_stdlib::standard_registry())
        .map_err(|e| anyhow::anyhow!("构建 index 失败：{e:?}"))?;
    let refs: Vec<&Ast> = asts.iter().collect();
    let analysis = analyze_program(&refs, &index);
    if !analysis.diagnostics.is_empty() {
        anyhow::bail!("候选语义检查未过（不应发生）：{:?}", analysis.diagnostics);
    }
    println!(
        "[run] 执行 `{}`（{} 个实参）…",
        spec.entry_action,
        spec.args.len()
    );
    // e2e 禁 mock（见 docs/e2e_test.md）：入口若声明 File/Http effect，注入**真实**库 host
    //（`sophia-stdlib` 的 register_native_hosts：真实 `reqwest` / sandboxed `std::fs`），与 CLI `run` 同口径；
    // 否则用默认 host 注册表（纯逻辑 / Console，无库 host）。
    let entry_uses_real_io = analysis
        .model
        .callables
        .get(&spec.entry_action)
        .map(|d| {
            d.declared_effects.iter().any(|e| {
                (e.family == "Http" && e.op == "Get")
                    || (e.family == "File" && (e.op == "Read" || e.op == "Write"))
            })
        })
        .unwrap_or(false);

    let (outcome, console): (sophia_runtime::Outcome, Vec<String>) = if entry_uses_real_io {
        let mut host = sophia_runtime::HostRegistry::new();
        sophia_stdlib::register_native_hosts(&mut host, std::env::temp_dir())
            .map_err(|e| anyhow::anyhow!("注册真实库 host 失败：{e}"))?;
        let (outcome, _trace) = sophia_runtime::run_action(
            &analysis.model,
            &refs,
            &spec.entry_action,
            spec.args.clone(),
            &mut host,
        )
        .map_err(|e| anyhow::anyhow!("解释执行失败：{e}"))?;
        let console = host.console.clone();
        (outcome, console)
    } else {
        let mut host = sophia_runtime::HostRegistry::new();
        let (outcome, _trace) = sophia_runtime::run_action(
            &analysis.model,
            &refs,
            &spec.entry_action,
            spec.args.clone(),
            &mut host,
        )
        .map_err(|e| anyhow::anyhow!("解释执行失败：{e}"))?;
        (outcome, host.console)
    };
    for line in &console {
        println!("    [console] {line}");
    }
    match outcome {
        sophia_runtime::Outcome::Returned(v) => match &spec.expect {
            Expect::Returns(expected) => {
                let value_ok = &v == expected;
                println!(
                    "    返回值：{v}（期望 {expected}）→ {}",
                    if value_ok { "匹配" } else { "不匹配" }
                );
                let console_ok = match &spec.expected_console {
                    None => true,
                    Some(expected) => {
                        let ok = &console == expected;
                        println!(
                            "    console：{:?}（期望 {:?}）→ {}",
                            console,
                            expected,
                            if ok { "匹配" } else { "不匹配" }
                        );
                        ok
                    }
                };
                let file_ok = match &spec.expected_file_content {
                    None => true,
                    Some(expected) => {
                        let Some(Value::Text(path)) = spec.args.first() else {
                            println!("    文件内容：缺少 Text 路径实参 → 不匹配");
                            return Ok(false);
                        };
                        let file_path = std::env::temp_dir().join(path);
                        match std::fs::read_to_string(&file_path) {
                            Ok(actual) => {
                                let ok = &actual == expected;
                                println!(
                                    "    文件内容：{:?}（期望 {:?}）→ {}",
                                    actual,
                                    expected,
                                    if ok { "匹配" } else { "不匹配" }
                                );
                                ok
                            }
                            Err(e) => {
                                println!("    文件内容：读取失败 {e} → 不匹配");
                                false
                            }
                        }
                    }
                };
                Ok(value_ok && console_ok && file_ok)
            }
            Expect::Raises(variant) => {
                println!("    返回值：{v}（但期望 raise `{variant}`）→ 不匹配");
                Ok(false)
            }
        },
        sophia_runtime::Outcome::Raised(e) => match &spec.expect {
            Expect::Raises(variant) => {
                let ok = e.variant == *variant;
                println!(
                    "    raise：`{}`（期望 raise `{variant}`）→ {}",
                    e.variant,
                    if ok { "匹配" } else { "不匹配" }
                );
                Ok(ok)
            }
            Expect::Returns(expected) => {
                println!("    raise 领域错误：{e}（但期望返回 {expected}）→ 不匹配");
                Ok(false)
            }
        },
    }
}

// ---- 真实 code_check（桥接 tools/check）----

/// 注入的确定性 code_check：委派 `sophia_engine::code_check`（语法 → HIR + 语义三层 +
/// strip-assist；三处共享的单一实现）+ 逐轮打印（harness 可观测性，便于定位收敛过程）。
pub fn real_check(files: &[(String, String)]) -> DiagnosticPayload {
    let payload = sophia_engine::code_check(files);
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

// ---- prompt system 文案 ----

/// decision 阶段 system prompt。`offer_decompose` 为真时（仅根焦点、尚无伪代码的 Tree 类
/// 用例）额外说明可选 decompose——引导而不替 LLM 决策（design 10.8）。
fn decision_system_prompt(offer_decompose: bool) -> String {
    let extra = if offer_decompose {
        "若目标明显由多个相互独立的业务子目标组成、单步实现过大，可选 decompose 把它\
         拆成若干子目标（每个子目标随后各自 design→implement）；否则直接 design_solution。"
    } else {
        "若尚无伪代码，selected_action 选 design_solution；"
    };
    format!(
        "你是 Sophia 工作流的决策者。只输出严格符合 decision schema 的 JSON 对象，含四个字段：\
         selected_action（字符串）、confidence（0..1 数字）、rationale（字符串）、\
         state_assessment（对象）。\
         state_assessment 必须是 goal 类型，恰好这些字段：\
         {{\"kind\":\"goal\",\"goal_size\":\"tiny|small|medium|large\",\
         \"decomposition_pressure\":\"low|medium|high\",\"active_milestone_present\":false,\
         \"outstanding_clarifications\":0}}。\
         不要添加任何其它字段。决策规则：{extra}\
         已有伪代码则选 implement_design；若上次实现未通过检查（概念性问题），可选 revise_design \
         重写伪代码。selected_action 只能取 candidate_actions 中列出的值。\
         不要输出 markdown 围栏或额外说明。"
    )
}

// ---- 工具 ----

fn print_files(files: &[(String, String)]) {
    for (path, content) in files {
        println!(
            "----- {path} -----\n{}\n-------------------",
            content.trim()
        );
    }
}
