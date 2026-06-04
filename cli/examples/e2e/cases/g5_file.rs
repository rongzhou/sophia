//! G5：标准库 `File`（本地文件读写 + intent 边界）用例（见 docs/e2e_test.md §G5、docs/file_lib.md）。
//!
//! 考察 `File.Read` / `File.Write` effect 声明、capability allow 绑定、以及 intent 边界：
//! `File.Read` 取回 `Raw<Text>`（不可信），必须经一个 `intent_conversion` 动作转为
//! `Sanitized<Text>` 才能使用；`File.Write` 的 content 必须是 `Sanitized<Text>`（写出边界）。
//!
//! **真实 IO（e2e 禁 mock）**：harness 据入口声明的 `File.*` effect 注入真实 native host
//! （`std::fs`），用例 write→read 往返打到 sandbox 根内的**真实临时文件**，不经任何内存桶 mock。
//!
//! **防答案泄漏**：只写题目（业务需求 + 验收条件）+ 入口 + 期望；不含任何 Sophia 源码答案。
//! `File` 库的语法 / intent 边界由按需库资产（`assets/stdlib/file.md`）承载，不进常驻基线。

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G5 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g5_01()]
}

/// 真实临时文件的 sandbox 相对路径。harness 把真实 host 的文件根设为 OS 临时目录。
fn temp_note_path() -> String {
    format!("sophia-e2e-g5-{}.txt", std::process::id())
}

/// G5-01：文件写入与回读（真实临时文件）。
///
/// 业务：把一条已脱敏的笔记写入某个路径，再以真实文件中保存的内容为准返回字符数。
fn g5_01() -> Case {
    Case {
        id: "G5-01",
        group: "g5",
        kind: CaseKind::DesignImplement,
        title: "笔记写入与回读",
        description: "在 vault 域内提供入口 StoreNote：接收文本 path（本地文件路径）与一条\
                      已脱敏、可安全保存的笔记 message。需要把 message 保存到 path 指向的\
                      真实本地文件；随后以该文件中实际保存的内容为准，返回内容字符数。",
        acceptance: &[
            "入口名为 StoreNote",
            "接收文本 path 与已脱敏的文本 message",
            "将 message 保存到 path 指向的真实本地文件",
            "返回该文件中实际保存内容的字符数",
        ],
        entry_action: "StoreNote",
        // 真实临时文件路径 + "hello"（长度 5）。运行时 intent 被剥离，message 传 Text 即可。
        args: vec![Value::Text(temp_note_path()), Value::Text("hello".into())],
        expect: Expect::Returns(Value::Int(5)),
        expected_console: None,
        expected_file_content: Some("hello".into()),
        max_repairs: 1,
        broken_seed: None,
    }
}
