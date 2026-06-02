//! 基准题集（见 docs/benchmark_test.md §四）。
//!
//! 全新题目（不复制任何外部原型）。**选题策略 = 表现力阶梯**（与 e2e 的"覆盖面"选题不同，见
//! 设计 §一）：L1→L6 是**单调递增的难度阶梯**，每一级在低级能力之上**叠加**新机制 / 更深组合，
//! 目的是让 `sophia` 工作流与 `baseline`（直接写 Python）的**能力分叉点**随难度上升显现出来——
//! 而非像 e2e 那样按正交能力维度铺开做回归覆盖。
//!
//! 每题在 v0 起步子集内可表达、可解释执行，且 baseline(Python) 能实现同样的输入→输出语义，
//! 保证两 mode 可比。
//!
//! **防答案泄漏**：每题只写业务题面 + 入口契约 + 公开禁止事项；`hidden_cases` 是答案，只用于
//! 事后判定。题面命名的领域词汇（状态名 / 字段名 / 错误名）是需求规格，不是实现提示。
//!
//! 与 e2e 用例**刻意不重叠**：e2e 按能力维度做正确性覆盖（IncrementCounter / CompleteTodo 等），
//! benchmark 用另一批**难度递增**的题做横向对比，避免两套测试互相污染结论。

use std::collections::BTreeMap;

use sophia_runtime::{ExpectedOutcome, HiddenCase, Value};

use crate::problem::{EntrySig, Level, NeutralTy, Param, Problem};

/// 全部基准题（按难度阶梯 L1→L5 聚合；每级在低级能力之上叠加新机制）。
pub fn all_problems() -> Vec<Problem> {
    vec![
        // L1：纯标量地板。
        l1_abs_difference(),
        l1_within_budget(),
        // L2：+ 结构化数据建模。
        l2_rectangle_area(),
        l2_traffic_next(),
        // L3：+ 跨动作组合。
        l3_discounted_total(),
        // L4：+ 错误代数。
        l4_checked_subtract(),
        // L5：组合上述全部机制（表现力分叉最易显现的顶层阶梯）。
        l5_checkout_limit(),
        // L6：可失败返回 `one of`（失败是返回值，非 raise）——纯逻辑、无 IO、确定，是「可失败建模」
        // 维度上 Sophia vs Python 的阶梯顶题。网络 / 文件题不入 benchmark（禁 mock，真实 IO 不确定、
        // 不公平；其端到端验收在 e2e 用真实 IO 做，见 docs/e2e_test.md §四 G2-03 / G5-01）。
        l6_clamp_or_reject(),
    ]
}

/// 按分级过滤。
pub fn by_level(level: Level) -> Vec<Problem> {
    all_problems()
        .into_iter()
        .filter(|p| p.level == level)
        .collect()
}

/// 按 id 过滤。
pub fn by_id(id: &str) -> Vec<Problem> {
    all_problems().into_iter().filter(|p| p.id == id).collect()
}

fn hc(reff: &str, action: &str, args: Vec<Value>, expected: ExpectedOutcome) -> HiddenCase {
    HiddenCase {
        verifier_ref: reff.to_string(),
        entry_action: action.to_string(),
        args,
        expected,
    }
}

// ---- L1（地板）：纯标量逻辑——单 action、Int / Bool、比较与算术 ----

/// L1：两数之差的绝对值。
fn l1_abs_difference() -> Problem {
    Problem {
        id: "abs_difference",
        level: Level::L1,
        title: "两数之差的绝对值",
        prompt_goal: "给定两个整数 left 与 right，返回二者差值的绝对值；也就是当 left 减 \
                      right 的结果为负时返回其相反数，否则返回该差值。",
        entry: EntrySig {
            name: "AbsDifference",
            inputs: vec![
                Param {
                    name: "left",
                    ty: NeutralTy::Int,
                },
                Param {
                    name: "right",
                    ty: NeutralTy::Int,
                },
            ],
            output: NeutralTy::Int,
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            hc(
                "pos",
                "AbsDifference",
                vec![Value::Int(9), Value::Int(2)],
                ExpectedOutcome::Returns(Value::Int(7)),
            ),
            hc(
                "neg",
                "AbsDifference",
                vec![Value::Int(2), Value::Int(9)],
                ExpectedOutcome::Returns(Value::Int(7)),
            ),
            hc(
                "zero",
                "AbsDifference",
                vec![Value::Int(5), Value::Int(5)],
                ExpectedOutcome::Returns(Value::Int(0)),
            ),
        ],
    }
}

/// L1：预算判定（比较返回 Bool）。只用 `<=` 比较，落在 v0 起步子集内。
fn l1_within_budget() -> Problem {
    Problem {
        id: "within_budget",
        level: Level::L1,
        title: "预算判定",
        prompt_goal: "给定两个整数 spent（已花费）与 budget（预算上限），当 spent 不超过 budget 时\
             返回 true（未超支），否则返回 false。",
        entry: EntrySig {
            name: "WithinBudget",
            inputs: vec![
                Param {
                    name: "spent",
                    ty: NeutralTy::Int,
                },
                Param {
                    name: "budget",
                    ty: NeutralTy::Int,
                },
            ],
            output: NeutralTy::Bool,
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            hc(
                "under",
                "WithinBudget",
                vec![Value::Int(80), Value::Int(100)],
                ExpectedOutcome::Returns(Value::Bool(true)),
            ),
            hc(
                "exact",
                "WithinBudget",
                vec![Value::Int(100), Value::Int(100)],
                ExpectedOutcome::Returns(Value::Bool(true)),
            ),
            hc(
                "over",
                "WithinBudget",
                vec![Value::Int(120), Value::Int(100)],
                ExpectedOutcome::Returns(Value::Bool(false)),
            ),
        ],
    }
}

// ---- L2（+ 结构化建模）：在标量之上叠加 entity 字段访问 / state + match 穷尽 ----

/// L2：矩形面积（entity 多字段 + 字段访问）。
fn l2_rectangle_area() -> Problem {
    let rect = |w: i64, h: i64| Value::Entity {
        name: "Rectangle".into(),
        fields: BTreeMap::from([
            ("width".to_string(), Value::Int(w)),
            ("height".to_string(), Value::Int(h)),
        ]),
    };
    Problem {
        id: "rectangle_area",
        level: Level::L2,
        title: "矩形面积",
        prompt_goal: "在 geometry 域中，Rectangle 表示矩形，包含整数 width（宽）与 \
                      height（高）。入口 RectangleArea 接收一个 Rectangle，返回其面积：\
                      width 乘以 height。",
        entry: EntrySig {
            name: "RectangleArea",
            inputs: vec![Param {
                name: "rect",
                ty: NeutralTy::Record(vec![
                    ("width".to_string(), NeutralTy::Int),
                    ("height".to_string(), NeutralTy::Int),
                ]),
            }],
            output: NeutralTy::Int,
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            hc(
                "square",
                "RectangleArea",
                vec![rect(6, 6)],
                ExpectedOutcome::Returns(Value::Int(36)),
            ),
            hc(
                "wide",
                "RectangleArea",
                vec![rect(10, 3)],
                ExpectedOutcome::Returns(Value::Int(30)),
            ),
            hc(
                "unit",
                "RectangleArea",
                vec![rect(1, 1)],
                ExpectedOutcome::Returns(Value::Int(1)),
            ),
        ],
    }
}

/// L2：信号灯下一状态（state 多 value + match 穷尽）。
fn l2_traffic_next() -> Problem {
    let st = |v: &str| Value::State {
        state: "TrafficLight".into(),
        value: v.into(),
    };
    Problem {
        id: "traffic_next",
        level: Level::L2,
        title: "信号灯下一状态",
        prompt_goal: "在 traffic 域中，TrafficLight 有 Red、Green、Yellow 三个取值。入口 \
                      NextLight 接收当前信号灯取值，返回交通灯循环中的下一个取值：Red 之后是 \
                      Green，Green 之后是 Yellow，Yellow 之后回到 Red。",
        entry: EntrySig {
            name: "NextLight",
            inputs: vec![Param {
                name: "current",
                ty: NeutralTy::State {
                    name: "TrafficLight".to_string(),
                    values: vec!["Red".into(), "Green".into(), "Yellow".into()],
                },
            }],
            output: NeutralTy::State {
                name: "TrafficLight".to_string(),
                values: vec!["Red".into(), "Green".into(), "Yellow".into()],
            },
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            hc(
                "r2g",
                "NextLight",
                vec![st("Red")],
                ExpectedOutcome::Returns(st("Green")),
            ),
            hc(
                "g2y",
                "NextLight",
                vec![st("Green")],
                ExpectedOutcome::Returns(st("Yellow")),
            ),
            hc(
                "y2r",
                "NextLight",
                vec![st("Yellow")],
                ExpectedOutcome::Returns(st("Red")),
            ),
        ],
    }
}

// ---- L3（+ 跨动作组合）：在建模之上叠加一个 action 调用另一个 ----

/// L3：折后总价（一个 action 调用另一个）。
fn l3_discounted_total() -> Problem {
    Problem {
        id: "discounted_total",
        level: Level::L3,
        title: "折后总价",
        prompt_goal: "在 pricing 域中，入口 NetTotal 接收整数 unit_price（单价）、quantity\
                      （数量）与 discount（折扣金额）。折后总价等于 unit_price 乘以 quantity \
                      得到的毛总价，再减去 discount。",
        entry: EntrySig {
            name: "NetTotal",
            inputs: vec![
                Param {
                    name: "unit_price",
                    ty: NeutralTy::Int,
                },
                Param {
                    name: "quantity",
                    ty: NeutralTy::Int,
                },
                Param {
                    name: "discount",
                    ty: NeutralTy::Int,
                },
            ],
            output: NeutralTy::Int,
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            // 单价 10 × 数量 5 = 50，减折扣 8 = 42。
            hc(
                "typical",
                "NetTotal",
                vec![Value::Int(10), Value::Int(5), Value::Int(8)],
                ExpectedOutcome::Returns(Value::Int(42)),
            ),
            hc(
                "no_discount",
                "NetTotal",
                vec![Value::Int(7), Value::Int(3), Value::Int(0)],
                ExpectedOutcome::Returns(Value::Int(21)),
            ),
            hc(
                "single",
                "NetTotal",
                vec![Value::Int(100), Value::Int(1), Value::Int(1)],
                ExpectedOutcome::Returns(Value::Int(99)),
            ),
        ],
    }
}

// ---- L4（+ 错误代数）：在组合之上叠加 error variant + raise + 条件分支 ----

/// L4：受限扣减（扣减额超过余量时 raise 领域错误）。
/// 只用减法 + 比较，落在 v0 起步子集内（解释器无除法 / 取模）。
fn l4_checked_subtract() -> Problem {
    Problem {
        id: "checked_subtract",
        level: Level::L4,
        title: "受限扣减",
        prompt_goal: "在 inventory 域中，入口 RemoveStock 接收整数 available（现有库存）与 \
                      requested（请求扣减量）。若 requested 大于 available，操作以中断式领域失败\
                      结束；对外可观察失败名称必须是 Insufficient，并携带 shortfall（缺口数量，等于 requested 减 \
                      available）；否则返回扣减后的剩余库存（available 减 requested）。",
        entry: EntrySig {
            name: "RemoveStock",
            inputs: vec![
                Param {
                    name: "available",
                    ty: NeutralTy::Int,
                },
                Param {
                    name: "requested",
                    ty: NeutralTy::Int,
                },
            ],
            output: NeutralTy::Int,
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            hc(
                "enough",
                "RemoveStock",
                vec![Value::Int(50), Value::Int(8)],
                ExpectedOutcome::Returns(Value::Int(42)),
            ),
            hc(
                "exact",
                "RemoveStock",
                vec![Value::Int(8), Value::Int(8)],
                ExpectedOutcome::Returns(Value::Int(0)),
            ),
            hc(
                "insufficient",
                "RemoveStock",
                vec![Value::Int(5), Value::Int(12)],
                ExpectedOutcome::Raises("Insufficient".to_string()),
            ),
        ],
    }
}

// ---- L5（组合全部机制）：entity 入参 + 跨动作调用 + 错误代数 + 标量算术 ----
//
// 这是阶梯顶层：把 L1–L4 的机制压进一道题——结构化输入（entity）、跨 action 调用、
// 条件分支与领域错误。表现力分叉（sophia 工作流 vs 直接写 Python）最可能在此显现。
// 仍严格落在 v0 起步子集内（只用 `+ - *`、比较、raise；无除法 / 取模）。

/// L5：结账额度校验（组合：entity 入参 + 调用 LineAmount + 错误代数）。
fn l5_checkout_limit() -> Problem {
    let line = |unit: i64, qty: i64| Value::Entity {
        name: "OrderLine".into(),
        fields: BTreeMap::from([
            ("unit_price".to_string(), Value::Int(unit)),
            ("quantity".to_string(), Value::Int(qty)),
        ]),
    };
    Problem {
        id: "checkout_limit",
        level: Level::L5,
        title: "结账额度校验（组合）",
        prompt_goal: "在 checkout 域中，OrderLine 表示订单行，包含整数 unit_price（单价）与 \
             quantity（数量）。入口 Checkout 接收一个 OrderLine 与整数 credit_limit（信用额度）。\
             行金额等于 unit_price 乘以 quantity；若行金额大于 credit_limit，操作以中断式领域失败\
             结束；对外可观察失败名称必须是 OverLimit，并携带 excess（超出额度的金额，等于行金额减 credit_limit）；否则\
             返回行金额。",
        entry: EntrySig {
            name: "Checkout",
            inputs: vec![
                Param {
                    name: "line",
                    ty: NeutralTy::Record(vec![
                        ("unit_price".to_string(), NeutralTy::Int),
                        ("quantity".to_string(), NeutralTy::Int),
                    ]),
                },
                Param {
                    name: "credit_limit",
                    ty: NeutralTy::Int,
                },
            ],
            output: NeutralTy::Int,
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            // 单价 6 × 数量 7 = 42，额度 100 → 未超，返回 42。
            hc(
                "within_limit",
                "Checkout",
                vec![line(6, 7), Value::Int(100)],
                ExpectedOutcome::Returns(Value::Int(42)),
            ),
            // 单价 5 × 数量 5 = 25，恰好等于额度 25 → 未超（边界），返回 25。
            hc(
                "exactly_limit",
                "Checkout",
                vec![line(5, 5), Value::Int(25)],
                ExpectedOutcome::Returns(Value::Int(25)),
            ),
            // 单价 9 × 数量 4 = 36，额度 30 → 超额，raise OverLimit。
            hc(
                "over_limit",
                "Checkout",
                vec![line(9, 4), Value::Int(30)],
                ExpectedOutcome::Raises("OverLimit".to_string()),
            ),
        ],
    }
}

// ---- L6（可失败返回 `one of`）：在阶梯顶端叠加「失败是返回值」机制 ----
//
// L6 不是「第六类能力」，而是 v1 语言扩充 F1（`one of`）落地后，在阶梯顶端验收「可失败建模」
// 维度上 Sophia 工作流 vs 直接写 Python 的表现力分叉。纯逻辑、无 IO、确定，可解释执行。
// 网络 / 文件题不入 benchmark（禁 mock，真实 IO 不确定、不公平），其端到端验收在 e2e 用真实
// IO 做（见 docs/e2e_test.md G2-03 / G5-01）。

/// 构造一个被返回的 error variant 值（`one of` 的失败成员）。
fn err(variant: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::ErrorValue {
        variant: variant.to_string(),
        fields: fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    }
}

/// L6：受限取值——可失败返回 `one of { Int, OutOfRange }`。
///
/// 验收 F1：失败是**返回值**（可恢复结局），区别于 L4 `checked_subtract` 的 `raise`（不可恢复
/// 中断）。调用方消费须 match 成功成员（裸 Int）与失败成员（OutOfRange）两路。
fn l6_clamp_or_reject() -> Problem {
    Problem {
        id: "clamp_or_reject",
        level: Level::L6,
        title: "受限取值",
        prompt_goal: "在 validation 域中，入口 ClampOrReject 接收两个整数 n 与 limit。若 n \
             落在 0 到 limit（含两端）之间，返回 n 本身；否则返回可恢复失败结局 OutOfRange，\
             并携带 value（越界的取值，等于 n）。越界是返回结局，不是中断式失败。",
        entry: EntrySig {
            name: "ClampOrReject",
            inputs: vec![
                Param {
                    name: "n",
                    ty: NeutralTy::Int,
                },
                Param {
                    name: "limit",
                    ty: NeutralTy::Int,
                },
            ],
            output: NeutralTy::OneOf(vec![
                NeutralTy::Int,
                NeutralTy::ErrorVariant {
                    variant: "OutOfRange".to_string(),
                    fields: vec![("value".to_string(), NeutralTy::Int)],
                },
            ]),
        },
        public_forbidden: vec![
            "不得使用存储 / 网络 / 时间 / 随机 / 文件系统 / 外部服务。",
            "越界必须作为返回结局，不得用中断式失败结束。",
            "不得针对具体输入特判或硬编码答案。",
        ],
        hidden_cases: vec![
            // 区间内 → 返回 n 本身。
            hc(
                "in_range",
                "ClampOrReject",
                vec![Value::Int(3), Value::Int(10)],
                ExpectedOutcome::Returns(Value::Int(3)),
            ),
            // 下边界 0 → 返回自身。
            hc(
                "low_edge",
                "ClampOrReject",
                vec![Value::Int(0), Value::Int(10)],
                ExpectedOutcome::Returns(Value::Int(0)),
            ),
            // 上边界 limit → 返回自身。
            hc(
                "high_edge",
                "ClampOrReject",
                vec![Value::Int(10), Value::Int(10)],
                ExpectedOutcome::Returns(Value::Int(10)),
            ),
            // 越界 → 返回失败成员 OutOfRange{value:15}（非 raise）。
            hc(
                "out_of_range",
                "ClampOrReject",
                vec![Value::Int(15), Value::Int(10)],
                ExpectedOutcome::Returns(err("OutOfRange", vec![("value", Value::Int(15))])),
            ),
        ],
    }
}
