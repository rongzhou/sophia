//! G1：基本语法 / 纯逻辑用例（见 docs/e2e_test.md §4）。
//!
//! 考察 action / entity / state、body 子语言、类型与 intent、纯函数（无 effect）。
//! 均为"一次过"用例（`max_repairs = 0`）。
//!
//! **防答案泄漏**：下面只写题目（业务需求 + 验收条件）、入口与期望返回值；
//! **不含任何 Sophia 源码答案**。任务命名其领域词汇（state/entity/字段名）是需求规格本身，
//! harness 据此构造输入并校验输出。

use std::collections::BTreeMap;

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G1 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g1_01(), g1_02(), g1_03(), g1_04(), r_01()]
}

/// G1-01：整数计数器加一（单 action、Int、算术、纯函数）。
fn g1_01() -> Case {
    Case {
        id: "G1-01",
        group: "g1",
        kind: CaseKind::DesignImplement,
        title: "整数计数器加一",
        description: "在 counter 域内提供入口 IncrementCounter：\
                      接收整数 current，返回 current 加一后的整数。",
        acceptance: &[
            "入口名为 IncrementCounter",
            "接收整数 current",
            "返回 current 加一后的整数",
        ],
        entry_action: "IncrementCounter",
        args: vec![Value::Int(41)],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        expected_file_content: None,
        max_repairs: 0,
        broken_seed: None,
    }
}

/// G1-02：待办状态置为完成（state 多 value + action、状态值返回）。
fn g1_02() -> Case {
    Case {
        id: "G1-02",
        group: "g1",
        kind: CaseKind::DesignImplement,
        title: "待办状态置为完成",
        description: "在 todo 域内提供入口 CompleteTodo：\
                      待办状态 TodoStatus 只有 Pending（未完成）与 Done（已完成）两种取值。\
                      接收一个 TodoStatus，返回表示已完成的 Done 状态。",
        acceptance: &[
            "保留状态名 TodoStatus 以及取值 Pending、Done",
            "入口名为 CompleteTodo，接收一个 TodoStatus",
            "返回 Done 状态",
        ],
        entry_action: "CompleteTodo",
        args: vec![Value::State {
            state: "TodoStatus".into(),
            value: "Pending".into(),
        }],
        expect: Expect::Returns(Value::State {
            state: "TodoStatus".into(),
            value: "Done".into(),
        }),
        expected_console: None,
        expected_file_content: None,
        max_repairs: 0,
        broken_seed: None,
    }
}

/// G1-03：购物车单项金额合计（entity 多字段、字段访问、整数乘法）。
fn g1_03() -> Case {
    // 输入实体：一项购物车条目（单价 7，数量 6）→ 期望合计 42。
    let item = Value::Entity {
        name: "CartItem".into(),
        fields: BTreeMap::from([
            ("unit_price".to_string(), Value::Int(7)),
            ("quantity".to_string(), Value::Int(6)),
        ]),
    };
    Case {
        id: "G1-03",
        group: "g1",
        kind: CaseKind::DesignImplement,
        title: "购物车单项金额合计",
        description: "在 cart 域内提供入口 LineTotal：\
                      购物车条目 CartItem 包含整数单价 unit_price 与整数数量 quantity。\
                      接收一个 CartItem，返回该条目的金额合计。",
        acceptance: &[
            "保留数据名 CartItem 以及字段 unit_price、quantity",
            "入口名为 LineTotal，接收一个 CartItem",
            "返回 unit_price 乘以 quantity 的整数",
        ],
        entry_action: "LineTotal",
        args: vec![item],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        expected_file_content: None,
        max_repairs: 0,
        broken_seed: None,
    }
}

/// G1-04：免邮资格判定（Bool 逻辑、比较）。
fn g1_04() -> Case {
    Case {
        id: "G1-04",
        group: "g1",
        kind: CaseKind::DesignImplement,
        title: "免邮资格判定",
        description: "在 shipping 域内提供入口 QualifiesForFreeShipping：\
                      输入一个整数 order_amount（订单金额），当金额达到或超过 100 时返回 \
                      true（享受免邮），否则返回 false。输出是一个 Bool。",
        acceptance: &[
            "入口名为 QualifiesForFreeShipping",
            "接收整数 order_amount",
            "金额达到或超过 100 返回 true，否则返回 false",
        ],
        entry_action: "QualifiesForFreeShipping",
        args: vec![Value::Int(150)],
        expect: Expect::Returns(Value::Bool(true)),
        expected_console: None,
        expected_file_content: None,
        max_repairs: 0,
        broken_seed: None,
    }
}

/// R-01：加一 action 的坏候选，验证真实诊断驱动 repair 收敛（修复闭环，横切）。
///
/// `broken_seed` 是"题目里待修的东西"——一份含常见真实缺陷的尝试，不暗示正确写法：
/// C 风格 `int`（应为 `Int`）、`output` 块漏花括号、body 引用未声明变量 `n`。
fn r_01() -> Case {
    Case {
        id: "R-01",
        group: "r",
        kind: CaseKind::RepairSeed,
        title: "修复加一坏候选",
        description:
            "在 counter 域内提供入口 IncrementCounter：输入整数 current，输出 current 加一。\
                      （起点候选有缺陷，需据诊断修复。）",
        acceptance: &[
            "入口名为 IncrementCounter",
            "接收整数 current",
            "返回 current 加一后的整数",
        ],
        entry_action: "IncrementCounter",
        args: vec![Value::Int(41)],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        expected_file_content: None,
        max_repairs: 3,
        broken_seed: Some((
            "counter/actions/IncrementCounter.sophia",
            "action IncrementCounter {\n  \
               input { current: int }\n  \
               output result: int\n  \
               body { return n + 1 }\n\
             }",
        )),
    }
}
