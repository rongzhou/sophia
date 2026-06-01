//! e2e 用例注册表（见 docs/e2e_test.md §4）。
//!
//! 新增用例 = 在对应组文件加一个 [`crate::harness::Case`]，并在这里把它登记进 [`all_cases`]。

pub mod g1_basics;
pub mod g2_effects;
pub mod g3_heuristic;
pub mod g4_complex;
pub mod g5_file;
pub mod g6_tree;

use crate::harness::Case;

/// 全部已注册用例（按组聚合，组内按 ID）。
pub fn all_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    cases.extend(g1_basics::cases());
    cases.extend(g2_effects::cases());
    cases.extend(g3_heuristic::cases());
    cases.extend(g4_complex::cases());
    cases.extend(g5_file::cases());
    cases.extend(g6_tree::cases());
    cases
}

/// 按组名过滤（如 `g1`）。
pub fn by_group(group: &str) -> Vec<Case> {
    all_cases()
        .into_iter()
        .filter(|c| c.group == group)
        .collect()
}

/// 按用例 ID 过滤（如 `G1-01`）。
pub fn by_id(id: &str) -> Vec<Case> {
    all_cases().into_iter().filter(|c| c.id == id).collect()
}
