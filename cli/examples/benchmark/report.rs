//! 基准产物：逐次记录（JSON Lines）+ 聚合表（见 docs/benchmark_test.md §六）。
//!
//! 只记**核心两指标**——成功率（passed）与耗时（wall_time_ms）——加最少归因（`failure`）。
//! 不引入封闭的失败枚举（设计 §六）：`failure` 是自由文本简述，若日后归因确有共性再固化。
//! 用 `serde_json` 手工构造记录（cli 已依赖 `serde_json`，不为此引 `serde` derive）。

use std::collections::BTreeMap;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde_json::{json, Value as Json};

/// 解法路径 mode。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Sophia 工作流（判定经 v0 解释器，复用 `runtime::verify`）。
    Sophia,
    /// LLM 直接写主流语言模块（当前只做 Python，判定经外部子进程）。
    Baseline,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Sophia => "sophia",
            Mode::Baseline => "baseline",
        }
    }
}

/// 单个 hidden case 的判定明细。
pub struct CaseOutcome {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// 一次 (题, mode, 运行序号) 的结构化记录。
pub struct RunRecord {
    pub id: String,
    pub level: String,
    pub mode: Mode,
    /// baseline 用的语言（当前恒为 `python`）；sophia 为 `None`。
    pub language: Option<String>,
    pub model: String,
    pub passed: bool,
    pub wall_time_ms: u128,
    /// 失败简述（成功为 `None`）。
    pub failure: Option<String>,
    pub cases: Vec<CaseOutcome>,
}

impl RunRecord {
    fn to_json(&self) -> Json {
        json!({
            "id": self.id,
            "level": self.level,
            "mode": self.mode.as_str(),
            "language": self.language,
            "model": self.model,
            "passed": self.passed,
            "wall_time_ms": self.wall_time_ms,
            "failure": self.failure,
            "cases": self.cases.iter().map(|c| json!({
                "name": c.name,
                "passed": c.passed,
                "detail": c.detail,
            })).collect::<Vec<_>>(),
        })
    }
}

/// 把记录以 JSON Lines 追加写入 `runs.jsonl`（append-only）。
pub fn append_run(out_dir: &Path, record: &RunRecord) -> std::io::Result<()> {
    create_dir_all(out_dir)?;
    let path = out_dir.join("runs.jsonl");
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", record.to_json())?;
    Ok(())
}

/// 聚合一组记录，渲染逐 (题 × mode) 的成功率 / 平均耗时表（设计 §六）。
///
/// 列即核心指标：`level | task | mode | runs | passed | success_rate | avg_wall_time_ms`。
/// 同一 (id, mode) 的多次运行聚合为一行。
pub fn render_summary(records: &[RunRecord]) -> String {
    // 稳定排序键：(level, id, mode)。
    let mut agg: BTreeMap<(String, String, &'static str), (u32, u32, u128)> = BTreeMap::new();
    for r in records {
        let key = (r.level.clone(), r.id.clone(), r.mode.as_str());
        let e = agg.entry(key).or_insert((0, 0, 0));
        e.0 += 1; // runs
        if r.passed {
            e.1 += 1; // passed
        }
        e.2 += r.wall_time_ms; // 累计耗时
    }

    let mut out = String::new();
    out.push_str("| level | task | mode | runs | passed | success_rate | avg_wall_time_ms |\n");
    out.push_str("|-------|------|------|-----:|-------:|-------------:|-----------------:|\n");
    for ((level, id, mode), (runs, passed, total_ms)) in &agg {
        let rate = if *runs > 0 {
            *passed as f64 / *runs as f64
        } else {
            0.0
        };
        let avg_ms = if *runs > 0 {
            *total_ms as f64 / *runs as f64
        } else {
            0.0
        };
        out.push_str(&format!(
            "| {level} | {id} | {mode} | {runs} | {passed} | {rate:.3} | {avg_ms:.0} |\n"
        ));
    }
    out
}
