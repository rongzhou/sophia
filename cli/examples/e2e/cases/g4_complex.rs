//! G4：复杂程序用例（见 docs/e2e_test.md §4）。
//!
//! 考察更接近真实系统的能力组合：**多 node 协作 / 跨 action 调用**、**error algebra**
//! （error / raise / errors 传播）。仍在 v0 起步子集内，规模受限但能力真实。
//!
//! 与 G1/G2 不同点在"能力组合"而非难度：G4-01 一个 action 调用另一个 action（跨文件、
//! 经 Execution Graph 调用边路由）；G4-02 用 error algebra 在非法输入时 raise 领域错误。
//!
//! **防答案泄漏**：只写题目（业务需求 + 验收条件），不含任何 Sophia 源码答案。

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G4 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g4_01(), g4_02(), g4_03()]
}

/// G4-01：订单总价（跨 action 调用——OrderTotal 调用 LineSubtotal）。
///
/// 业务：先算单项小计（单价 × 数量），再加运费得订单总价。要求拆成两个 action：
/// 一个算小计、一个算总价并**调用**前者。考察跨 action 调用（经 Execution Graph 调用边）。
fn g4_01() -> Case {
    Case {
        id: "G4-01",
        group: "g4",
        kind: CaseKind::DesignImplement,
        title: "订单总价（跨 action 调用）",
        description: "在 order 域内实现两个 action：\
                      ① LineSubtotal：输入两个整数 unit_price（单价）与 quantity（数量），\
                      输出单项小计（单价 × 数量）的 Int；\
                      ② OrderTotal：输入三个整数 unit_price、quantity、shipping（运费），\
                      它必须**调用** LineSubtotal 得到小计，再加上 shipping，输出订单总价 Int。\
                      两个 action 各放一个文件。",
        acceptance: &[
            "存在 action LineSubtotal，输入 unit_price 与 quantity（Int），输出单价乘数量的 Int",
            "存在 action OrderTotal，输入 unit_price、quantity、shipping（Int）",
            "OrderTotal 调用 LineSubtotal 得到小计，再加 shipping，输出订单总价 Int",
        ],
        entry_action: "OrderTotal",
        // 单价 7 × 数量 5 = 35，加运费 7 = 42。
        args: vec![Value::Int(7), Value::Int(5), Value::Int(7)],
        expect: Expect::Returns(Value::Int(42)),
        expected_console: None,
        max_repairs: 0,
        broken_seed: None,
    }
}

/// G4-02：提现校验（error algebra——余额不足时 raise 领域错误）。
///
/// 业务：从账户余额提现一笔金额。若提现金额超过余额，应 raise 一个领域错误
/// （InsufficientFunds）；否则返回提现后的余额。考察 error 声明 / `raise` / `errors`。
/// 本用例的入参故意让金额超过余额，期望结果是 **raise InsufficientFunds**。
fn g4_02() -> Case {
    Case {
        id: "G4-02",
        group: "g4",
        kind: CaseKind::DesignImplement,
        title: "提现校验（error algebra）",
        description: "在 wallet 域内：\
                      ① 定义一个名为 WalletError 的 error，含一个 variant InsufficientFunds，\
                      该 variant 带一个 Int 字段 shortfall（缺口金额）；\
                      ② 定义一个名为 Withdraw 的 action，输入两个整数 balance（余额）与 \
                      amount（提现金额），在 errors 中声明 InsufficientFunds。\
                      若 amount 大于 balance，则 raise InsufficientFunds（shortfall = amount 减 \
                      balance）；否则返回提现后的余额（balance 减 amount）的 Int。\
                      error 与 action 各放一个文件。",
        acceptance: &[
            "存在名为 WalletError 的 error，含 variant InsufficientFunds，带 Int 字段 shortfall",
            "存在名为 Withdraw 的 action，输入 balance 与 amount（Int），errors 声明 InsufficientFunds",
            "amount > balance 时 raise InsufficientFunds，否则返回 balance 减 amount 的 Int",
        ],
        entry_action: "Withdraw",
        // 余额 30，提现 50 → 超额，期望 raise InsufficientFunds。
        args: vec![Value::Int(30), Value::Int(50)],
        expect: Expect::Raises("InsufficientFunds"),
        expected_console: None,
        max_repairs: 0,
        broken_seed: None,
    }
}

/// G4-03：受限取值（可失败返回 `one of`——失败是**返回值**，非 raise）。
///
/// 业务：把一个取值限制在 0..=limit；越界时**返回**一个错误结局 `OutOfRange`（而非 raise），
/// 区间内则返回取值本身。返回类型是 `one of { Int, OutOfRange }`，调用方须 match 两路。
/// 考察 F1 的核心增量：可恢复失败用**返回值**表达（区别于 G4-02 的 `raise` 不可恢复中断）。
/// 本用例入参故意越界，期望结果是 **返回 `OutOfRange { value = 15 }`**（不是 raise）。
fn g4_03() -> Case {
    use std::collections::BTreeMap;
    Case {
        id: "G4-03",
        group: "g4",
        kind: CaseKind::DesignImplement,
        title: "受限取值（可失败返回 one of）",
        description: "在 validation 域内：\
                      ① 定义一个名为 RangeError 的 error，含一个 variant OutOfRange，该 variant \
                      带一个 Int 字段 value（越界的取值）；\
                      ② 定义一个名为 ClampOrReject 的 action，输入两个整数 n 与 limit，\
                      返回 one of { Int, OutOfRange }：若 n 落在 0 到 limit（含两端）之间，\
                      返回 n 本身；否则**返回** OutOfRange（value = n）。\
                      注意：越界时把该 error 作为**返回结局**返回，**不要 raise**。\
                      error 与 action 各放一个文件。",
        acceptance: &[
            "存在名为 RangeError 的 error，含 variant OutOfRange，带 Int 字段 value",
            "存在名为 ClampOrReject 的 action，输入 n 与 limit（Int），返回 one of { Int, OutOfRange }",
            "n 在 0..=limit 内返回 n 本身；否则返回 OutOfRange（value = n），不得 raise",
        ],
        entry_action: "ClampOrReject",
        // n=15 > limit=10 → 越界，期望返回失败成员 OutOfRange{value:15}（非 raise）。
        args: vec![Value::Int(15), Value::Int(10)],
        expect: Expect::Returns(Value::ErrorValue {
            variant: "OutOfRange".into(),
            fields: BTreeMap::from([("value".to_string(), Value::Int(15))]),
        }),
        expected_console: None,
        max_repairs: 0,
        broken_seed: None,
    }
}
