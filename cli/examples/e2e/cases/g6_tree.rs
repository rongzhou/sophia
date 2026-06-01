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

/// G6-01：温控面板的两个独立读数转换（根目标拆成两个具名 action 子目标）。
///
/// 业务：一个温控面板需要两个相互独立的纯逻辑换算 action：
/// ① CelsiusToScaled：把摄氏温度按固定倍率换算为内部刻度值；
/// ② FahrenheitOffset：把华氏读数减去固定基准偏移。
/// 两者互不调用、各自独立——天然适合 decompose 成两个子目标分别实现。
///
/// 入口取其中之一 `CelsiusToScaled(21)`：按倍率 2 换算 → 42。harness 合并两个子目标的候选
/// 文件为一个程序，再执行入口 action 对照期望。
fn g6_01() -> Case {
    Case {
        id: "G6-01",
        group: "g6",
        kind: CaseKind::Tree,
        title: "温控面板的两个独立读数换算",
        description: "在 climate 域内实现两个相互独立、互不调用的纯逻辑 action（适合拆成两个\
                      子目标分别实现）：\
                      ① 名为 CelsiusToScaled 的 action，输入一个整数 celsius（摄氏温度），\
                      输出 celsius 乘以固定倍率 2 后的内部刻度值（Int）；\
                      ② 名为 FahrenheitOffset 的 action，输入一个整数 fahrenheit（华氏读数），\
                      输出 fahrenheit 减去固定基准偏移 32 后的值（Int）。\
                      每个 action 放一个文件。",
        acceptance: &[
            "存在名为 CelsiusToScaled 的 action，输入 Int celsius，输出 celsius 乘以 2 的 Int",
            "存在名为 FahrenheitOffset 的 action，输入 Int fahrenheit，输出 fahrenheit 减 32 的 Int",
            "两个 action 相互独立、互不调用",
        ],
        entry_action: "CelsiusToScaled",
        // 21 × 2 = 42。
        args: vec![Value::Int(21)],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        // 子目标各自一次过；不设修复预算（spine 内部默认预算仍在）。
        max_repairs: 0,
        broken_seed: None,
    }
}
