//! 基准题的单一表示（见 docs/benchmark_test.md §四 / §七）。
//!
//! 一道题是一个 **Rust 值** `Problem`，与 e2e 把用例表示为 Rust `Case` 同构——不引入外部
//! 配置文件格式。隐藏验证用例直接复用 `runtime::verify::HiddenCase`（实参与期望都用
//! `runtime::Value` 表达），一处定义、`sophia` 与 `baseline` 两 mode 共用。
//!
//! **防答案泄漏（最要紧，设计 §三）**：`hidden_cases` 是答案，**绝不进 prompt**。本模块把
//! prompt 可见的「公开题面」(`prompt_goal` / `entry` / `public_forbidden`) 与隐藏的
//! `hidden_cases` 放在同一结构里，但 prompt 组装函数**在类型上只接收公开字段**（见
//! `public_brief`），hidden cases 只流向判定层——靠函数签名而非自律隔离。

use serde_json::{Map, Value as Json};
use sophia_runtime::HiddenCase;

use crate::value_json::value_to_json;

/// 能力分级（设计 §一.6 / §四）：L1→L6 单调递增难度阶梯。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    L1,
    L2,
    L3,
    L4,
    L5,
    L6,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::L1 => "L1",
            Level::L2 => "L2",
            Level::L3 => "L3",
            Level::L4 => "L4",
            Level::L5 => "L5",
            Level::L6 => "L6",
        }
    }

    /// 从大小写不敏感的字符串解析（用于 `--level` 过滤）。
    pub fn parse(s: &str) -> Option<Level> {
        match s.to_uppercase().as_str() {
            "L1" => Some(Level::L1),
            "L2" => Some(Level::L2),
            "L3" => Some(Level::L3),
            "L4" => Some(Level::L4),
            "L5" => Some(Level::L5),
            "L6" => Some(Level::L6),
            _ => None,
        }
    }
}

/// 语言中立类型词汇表（设计 §一）：只用来向 LLM 描述入口形状、指导 `Value ↔ JSON` 规约。
/// 它**不是**新语言，只是 `runtime::Value` 结构的类型投影。当前题集用到的子集如下；
/// 新题若需 Text / `one of {...}` / `list of T` 等，按需补入（JIT，不预先声明）。
#[derive(Debug, Clone)]
pub enum NeutralTy {
    Int,
    Bool,
    /// 记录 / 实体：有序字段（字段名 → 类型）。
    Record(Vec<(String, NeutralTy)>),
    /// 状态：状态名 + 全部取值名（取值在 JSON 中规约为取值名字符串）。
    State {
        name: String,
        values: Vec<String>,
    },
    /// 可失败 / 互斥结局联合 `one of { 成员, ... }`（F1）：成员是「成功类型」与「失败结局」的
    /// 互斥集合。题面据此描述「返回以下结局之一」；调用方须 match 全部成员。`describe()` 只列
    /// 成员的中立描述，不暗示实现。
    OneOf(Vec<NeutralTy>),
    /// 领域错误结局成员（被返回的 `one of` 失败成员）：variant 名 + 有序字段。规约为
    /// `{variant, fields}` 对象（与 `runtime::Value::ErrorValue` / `value_to_json` 对齐）。
    ErrorVariant {
        variant: String,
        fields: Vec<(String, NeutralTy)>,
    },
}

impl NeutralTy {
    /// 业务化、语言中立的类型描述（题面 / 入口契约用，两 mode 共享同一文案，保证公平）。
    pub fn describe(&self) -> String {
        match self {
            NeutralTy::Int => "整数".to_string(),
            NeutralTy::Bool => "布尔值（true/false）".to_string(),
            NeutralTy::Record(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(n, t)| format!("{n}（{}）", t.describe()))
                    .collect();
                format!("含字段 [{}] 的对象", parts.join("、"))
            }
            NeutralTy::State { name, values } => {
                format!(
                    "{name} 状态（取值为以下名称之一的字符串：{}）",
                    values.join(" / ")
                )
            }
            NeutralTy::OneOf(members) => {
                let parts: Vec<String> = members.iter().map(|m| m.describe()).collect();
                format!("以下结局之一：{}", parts.join("；或 "))
            }
            NeutralTy::ErrorVariant { variant, fields } => {
                if fields.is_empty() {
                    format!("领域错误结局 {variant}")
                } else {
                    let parts: Vec<String> = fields
                        .iter()
                        .map(|(n, t)| format!("{n}（{}）", t.describe()))
                        .collect();
                    format!("领域错误结局 {variant}（含字段 [{}]）", parts.join("、"))
                }
            }
        }
    }
}

/// 入口参数（名字 + 类型）。顺序必须与 `HiddenCase.args` 一致。
#[derive(Debug, Clone)]
pub struct Param {
    pub name: &'static str,
    pub ty: NeutralTy,
}

/// 入口签名契约：语言中立地描述「调用什么、传什么、返回什么」。两 mode 共用同一契约。
#[derive(Debug, Clone)]
pub struct EntrySig {
    /// `sophia` mode：入口 action 名；`baseline` mode：入口函数对应的语义。
    pub name: &'static str,
    pub inputs: Vec<Param>,
    pub output: NeutralTy,
}

/// 一道基准题（题目 + 入口契约 + 隐藏验证用例）。
pub struct Problem {
    /// 稳定题目 id。
    pub id: &'static str,
    pub level: Level,
    /// 业务化标题。
    pub title: &'static str,
    /// 自然语言题面（业务语言、语言中立，对 sophia 与 baseline 都成立）。
    pub prompt_goal: &'static str,
    /// 入口签名契约（公开，进 prompt）。
    pub entry: EntrySig,
    /// 公开禁止事项（进 prompt，是题面的一部分）。
    pub public_forbidden: Vec<&'static str>,
    /// 隐藏验证用例（复用 `runtime::verify::HiddenCase`）。**绝不进 prompt**，仅执行后对照。
    pub hidden_cases: Vec<HiddenCase>,
}

impl Problem {
    /// 提取**仅含公开字段**的题面摘要——prompt 组装函数只接收它，hidden cases 在类型上无法
    /// 流入 prompt（结构防线，设计 §三）。
    pub fn public_brief(&self) -> PublicBrief<'_> {
        PublicBrief {
            title: self.title,
            prompt_goal: self.prompt_goal,
            entry: &self.entry,
            public_forbidden: &self.public_forbidden,
        }
    }

    /// 把一个 hidden case 的实参规约为「参数名 → JSON 值」对象（供 baseline Python 子进程）。
    /// 依赖 `entry.inputs` 的名字与顺序与 `args` 对齐。
    pub fn named_input_json(&self, case: &HiddenCase) -> Json {
        let mut map = Map::new();
        for (param, arg) in self.entry.inputs.iter().zip(&case.args) {
            map.insert(param.name.to_string(), value_to_json(arg));
        }
        Json::Object(map)
    }
}

/// 题面的公开切片（prompt 可见部分）。**不含** `hidden_cases`——这是防泄漏的结构边界。
pub struct PublicBrief<'a> {
    pub title: &'a str,
    pub prompt_goal: &'a str,
    pub entry: &'a EntrySig,
    pub public_forbidden: &'a [&'static str],
}

impl PublicBrief<'_> {
    /// 入口契约的语言中立描述行（两 mode 共用，保证给两边的信息等价、对比公平）。
    pub fn entry_contract_lines(&self) -> Vec<String> {
        let inputs: Vec<String> = self
            .entry
            .inputs
            .iter()
            .map(|p| format!("{}（{}）", p.name, p.ty.describe()))
            .collect();
        let inputs_desc = if inputs.is_empty() {
            "无".to_string()
        } else {
            inputs.join("、")
        };
        vec![
            format!("入口名称：{}", self.entry.name),
            format!("输入参数（按此顺序）：{inputs_desc}"),
            format!("输出：{}", self.entry.output.describe()),
        ]
    }
}
