//! 多候选评分排序（见 docs/language_design.md 10.9 score 块）。
//!
//! **评分不是图节点**（spec 第二节 20 类 role 无 `Score`）：score 是确定性管线的**内存
//! 选择启发式**，用于在多个**已通过全部 gate** 的候选间排序，**不持久化进图**。选择结果
//! 仍只由 `SelectionNode { rationale }` 表达（编排层据排名选出 winner 后建一个 SelectionNode）。
//!
//! 维度（design 10.9）：compile / tests / constraints / simplicity / locality /
//! capability_minimality / pseudocode_clarity，`overall = weighted_sum`。
//!
//! **硬约束**：`compile == 0` 时 `overall` 不得超过 `0.49`——防止"语义合理但不可编译"的
//! 候选被选中（design 10.9）。
//!
//! 诚实性：compile / constraints / tests 取自确定性 gate 报告（真实信号，非臆造）；
//! simplicity / locality / capability_minimality 由候选源码的可度量结构性属性计算
//! （非启发式黑箱——给出明确公式）；pseudocode_clarity 关乎伪代码而非代码，故作为**调用方
//! 显式提供**的输入（无信号时取中性 0.5），不在代码侧伪造。

/// 七个评分维度的权重（design 10.9 `overall: weighted_sum`）。
///
/// 默认权重把**决断性正确性维度**（compile / tests / constraints）置于主导，结构性偏好
/// （simplicity / locality / capability_minimality / pseudocode_clarity）为次要平局打破因子。
#[derive(Debug, Clone, Copy)]
pub struct ScoreWeights {
    pub compile: f64,
    pub tests: f64,
    pub constraints: f64,
    pub simplicity: f64,
    pub locality: f64,
    pub capability_minimality: f64,
    pub pseudocode_clarity: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        // 正确性维度合计 0.75（主导），结构性维度合计 0.25（次要）。归一化在 `overall` 内做。
        ScoreWeights {
            compile: 0.35,
            tests: 0.20,
            constraints: 0.20,
            simplicity: 0.08,
            locality: 0.07,
            capability_minimality: 0.05,
            pseudocode_clarity: 0.05,
        }
    }
}

impl ScoreWeights {
    fn sum(&self) -> f64 {
        self.compile
            + self.tests
            + self.constraints
            + self.simplicity
            + self.locality
            + self.capability_minimality
            + self.pseudocode_clarity
    }
}

/// 一个候选的评分输入：决断性信号（来自 gate 报告）+ 候选源码 + 可选伪代码清晰度。
#[derive(Debug, Clone)]
pub struct ScoreInputs<'a> {
    /// code_check 是否通过（compile 维度的真实信号）。
    pub compile_pass: bool,
    /// runtime validation 是否通过（tests 维度的 v0 代理信号）。
    pub tests_pass: bool,
    /// constraint_audit 是否通过（constraints 维度的真实信号）。
    pub constraints_pass: bool,
    /// 候选文件（path → content），用于计算结构性维度。
    pub files: &'a [(String, String)],
    /// 伪代码清晰度 [0,1]（关乎伪代码而非代码；调用方有信号才提供，否则 `None` → 中性 0.5）。
    pub pseudocode_clarity: Option<f64>,
}

/// 一个候选的评分结果：七个维度子分 + 加权总分。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub compile: f64,
    pub tests: f64,
    pub constraints: f64,
    pub simplicity: f64,
    pub locality: f64,
    pub capability_minimality: f64,
    pub pseudocode_clarity: f64,
    pub overall: f64,
}

/// 对单个候选评分。
///
/// 结构性维度公式（均归一化到 [0,1]，越大越好）：
/// - `simplicity`：源码越短越简单——`1 / (1 + total_chars / 400)`；
/// - `locality`：文件越少越局部——`1 / file_count`（至少 1 个文件）；
/// - `capability_minimality`：声明的 effect / capability 越少越小权限——
///   `1 / (1 + effect_capability_decls)`。
///
/// `overall = (Σ weight_i * dim_i) / Σ weight_i`，再施加硬约束：`compile == 0 → overall ≤ 0.49`。
pub fn score_candidate(inputs: &ScoreInputs, weights: &ScoreWeights) -> Score {
    let compile = bool_score(inputs.compile_pass);
    let tests = bool_score(inputs.tests_pass);
    let constraints = bool_score(inputs.constraints_pass);

    let total_chars: usize = inputs.files.iter().map(|(_, c)| c.chars().count()).sum();
    let simplicity = 1.0 / (1.0 + total_chars as f64 / 400.0);

    let file_count = inputs.files.len().max(1) as f64;
    let locality = 1.0 / file_count;

    let decls = effect_capability_decls(inputs.files) as f64;
    let capability_minimality = 1.0 / (1.0 + decls);

    // 伪代码清晰度：无信号取中性 0.5（不伪造）。
    let pseudocode_clarity = inputs.pseudocode_clarity.unwrap_or(0.5).clamp(0.0, 1.0);

    let weighted = weights.compile * compile
        + weights.tests * tests
        + weights.constraints * constraints
        + weights.simplicity * simplicity
        + weights.locality * locality
        + weights.capability_minimality * capability_minimality
        + weights.pseudocode_clarity * pseudocode_clarity;
    let mut overall = weighted / weights.sum().max(f64::EPSILON);

    // 硬约束（design 10.9）：不可编译的候选 overall 封顶 0.49。
    if compile == 0.0 {
        overall = overall.min(0.49);
    }

    Score {
        compile,
        tests,
        constraints,
        simplicity,
        locality,
        capability_minimality,
        pseudocode_clarity,
        overall,
    }
}

/// 对一组候选评分并按 `overall` 降序排名。返回 `(原始下标, Score)` 列表（高分在前）。
///
/// **确定性平局打破**：`overall` 相等时按原始下标升序（稳定、可复现）。返回原始下标使
/// 调用方能定位胜出候选；空输入返回空列表。
pub fn rank_candidates(inputs: &[ScoreInputs], weights: &ScoreWeights) -> Vec<(usize, Score)> {
    let mut scored: Vec<(usize, Score)> = inputs
        .iter()
        .enumerate()
        .map(|(i, inp)| (i, score_candidate(inp, weights)))
        .collect();
    // 降序 overall；平局按下标升序（确定性）。
    scored.sort_by(|a, b| {
        b.1.overall
            .partial_cmp(&a.1.overall)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    scored
}

/// 布尔信号 → 0.0 / 1.0。
fn bool_score(pass: bool) -> f64 {
    if pass {
        1.0
    } else {
        0.0
    }
}

/// 统计候选源码中声明的 effect / capability 数（capability_minimality 的可度量信号）。
///
/// 起步阶段用轻量文本计数：`effects {` 块与 `capability:` 绑定的出现次数。这是确定性的
/// 结构性度量（非语义分析）——权限声明越多，最小权限分越低。
fn effect_capability_decls(files: &[(String, String)]) -> usize {
    files
        .iter()
        .map(|(_, content)| {
            content.matches("effects {").count() + content.matches("capability:").count()
        })
        .sum()
}
