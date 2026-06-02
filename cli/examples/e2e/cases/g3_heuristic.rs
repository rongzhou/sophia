//! G3：启发式节点处理用例（见 docs/e2e_test.md §4）。
//!
//! 与 G1/G2 不同：G3 **不**由 harness 硬编码 design→implement 的顺序，而是经调度器
//! `run_goal_loop` 把动作选择权交给 **LLM**——每轮先产 DecisionNode（`considers→ 焦点`），
//! 再据 `selected_action` 自主推进。考察的是"LLM 据演进的图状态自主决策 decision→design→
//! implement，推进到可物化候选"（design §10.8），而非被脚本锁死的固定流程。
//!
//! 这依赖"prompt 调用时刻渲染"（engineering_architecture §8.4）：每轮 decision 的 prompt
//! 由当前 active context + 进度渲染——LLM 因此能看到"是否已有伪代码"并相应推进。
//!
//! **防答案泄漏**：同前组——只写题目（业务需求 + 验收条件），不含任何 Sophia 源码答案。

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G3 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g3_01()]
}

/// G3-01：库存扣减（经调度器自主推进 decision→design→implement）。
///
/// 业务：一个把库存数量按购买量扣减的纯逻辑 action。任务本身规模适中（single action），
/// 但驱动方式是 `Scheduler`——LLM 必须先决策 design_solution、产出伪代码，再决策
/// implement_design、产出候选。考察自主多步推进而非单步。
fn g3_01() -> Case {
    Case {
        id: "G3-01",
        group: "g3",
        kind: CaseKind::Scheduler,
        title: "库存扣减",
        description: "在 inventory 域内提供入口 DeductStock：\
                      输入两个整数 on_hand（当前库存）与 purchased（本次购买量），\
                      输出扣减后的剩余库存（on_hand 减 purchased）。",
        acceptance: &[
            "入口名为 DeductStock",
            "接收整数 on_hand 与 purchased",
            "返回 on_hand 减 purchased 的整数",
        ],
        entry_action: "DeductStock",
        args: vec![Value::Int(50), Value::Int(8)],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        expected_file_content: None,
        // 调度器场景：给修复预算（implement-loop 内部用），但仍期望顺利推进。
        max_repairs: 2,
        broken_seed: None,
    }
}
