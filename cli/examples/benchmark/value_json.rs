//! `runtime::Value` ↔ 语言中立 JSON 的规约（见 docs/benchmark_test.md §二 / §五）。
//!
//! 单一值模型 `runtime::Value` 既供 `sophia` mode 直接喂解释器（原生比对，不经 JSON），
//! 也经本模块投影为**语言中立 JSON** 供 `baseline` mode 与外部 Python 子进程交换。
//! 三 mode 共用同一规约函数，保证对比公平。
//!
//! 规约约定（与 `Value` 结构同构，但去掉运行时专属标签）：
//! - `Int` / `Bool` / `Text` → JSON number / bool / string；
//! - `Unit` → `null`；
//! - `List` → JSON 数组；
//! - `Null` → `null`；
//! - `Entity` → 字段对象（**丢弃 entity 名**——Python 侧只有结构没有具名实体）；
//! - `State` → 状态**值名**字符串（如 `TodoStatus.Done` → `"Done"`）；
//! - `ErrorValue` → `{variant, fields}` 对象（被返回的 `one of` 错误成员）。

use serde_json::{json, Value as Json};
use sophia_runtime::Value;

/// 把 `runtime::Value` 投影为语言中立 JSON（用于发送输入、规约期望值）。
pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Unit => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Text(s) => json!(s),
        Value::List(items) => Json::Array(items.iter().map(value_to_json).collect()),
        Value::Null => Json::Null,
        // `one of` 成员被返回的 error variant：规约为 `{variant, fields}` 对象。
        Value::ErrorValue { variant, fields } => {
            let mut map = serde_json::Map::new();
            map.insert("variant".to_string(), json!(variant));
            let fmap = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            map.insert("fields".to_string(), Json::Object(fmap));
            Json::Object(map)
        }
        Value::Entity { fields, .. } => {
            let map = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            Json::Object(map)
        }
        Value::State { value, .. } => json!(value),
    }
}
