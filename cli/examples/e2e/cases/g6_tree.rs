//! G6：目标树遍历（decompose → 子目标各自推进）用例（见 docs/e2e_test.md §4）。
//!
//! 考察**非线性目标树推进**（design 10.8 动作 6 decompose / 10.9 遍历层）：根目标明显由
//! 多个相互独立的具名 action 需求组成，LLM 自主选择 `decompose` 把它拆成若干子目标，每个
//! 子目标经 spine 各自 design→implement 推进到候选；harness 合并所有候选为一个程序执行。
//!
//! 关键链路（本轮 A2 打通）：
//! - decompose 后，遍历层经**人类授权检查点**（`AutoAcceptReviewer`，harness 代表人类）建
//!   真实 `AcceptanceEvent accepts→ Decomposition`，子目标沿 `member_of` 继承 binding 进入
//!   各自 active context（design 5.3）；
//! - harness 的 prompt 提供者是 **focus-aware** 的：子目标的 design/implement 看到的是**自己**
//!   的目标题面（从 active context 按 focus id 取），而非根目标——这是子目标能被正确实现的前提。
//!
//! **防答案泄漏**：只写题目（业务需求 + 每个子目标要做什么）+ 入口 + 期望；不含任何 Sophia
//! 源码答案。子目标的"答案"由 LLM 自行设计，harness 只在执行后对照期望。

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G6 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g6_01()]
}

/// G6-01：温控面板的两个独立读数转换。
///
/// 业务：一个温控面板需要两个相互独立的纯逻辑换算入口：
/// ① CelsiusToScaled：把摄氏温度按固定倍率换算为内部刻度值；
/// ② FahrenheitOffset：把华氏读数减去固定基准偏移。
/// 两者互不依赖、各自独立。
///
/// 入口取其中之一 `CelsiusToScaled(21)`：按倍率 2 换算 → 42。
fn g6_01() -> Case {
    Case {
        id: "G6-01",
        group: "g6",
        kind: CaseKind::Tree,
        title: "温控面板的两个独立读数换算",
        description: "在 climate 域内提供两个彼此独立的换算入口：CelsiusToScaled 接收整数 \
                      celsius（摄氏温度），返回 celsius 乘以固定倍率 2 后的内部刻度值；\
                      FahrenheitOffset 接收整数 fahrenheit（华氏读数），返回 fahrenheit 减去\
                      固定基准偏移 32 后的值。两个换算互不依赖。",
        acceptance: &[
            "存在入口 CelsiusToScaled，接收整数 celsius，返回 celsius 乘以 2 的整数",
            "存在入口 FahrenheitOffset，接收整数 fahrenheit，返回 fahrenheit 减 32 的整数",
            "两个换算互不依赖",
        ],
        entry_action: "CelsiusToScaled",
        // 21 × 2 = 42。
        args: vec![Value::Int(21)],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        expected_file_content: None,
        // 子目标各自一次过；不设修复预算（spine 内部默认预算仍在）。
        max_repairs: 0,
        broken_seed: None,
    }
}
