//! G5：标准库 `File`（本地文件读写 + intent 边界）用例（见 docs/e2e_test.md §G5、docs/file_lib.md）。
//!
//! 考察 `File.Read` / `File.Write` effect 声明、capability allow 绑定、以及 intent 边界：
//! `File.Read` 取回 `Raw<Text>`（不可信），必须经一个 `intent_conversion` 动作转为
//! `Sanitized<Text>` 才能使用；`File.Write` 的 content 必须是 `Sanitized<Text>`（写出边界）。
//!
//! **真实 IO（e2e 禁 mock）**：harness 据入口声明的 `File.*` effect 注入真实 `CliHost`
//! （`std::fs`），用例 write→read 往返打到**真实临时文件**（OS 临时目录下的固定路径），不经任何
//! 内存桶 mock。
//!
//! **防答案泄漏**：只写题目（业务需求 + 验收条件）+ 入口 + 期望；不含任何 Sophia 源码答案。
//! `File` 库的语法 / intent 边界由按需库资产（`assets/stdlib/file.md`）承载，不进常驻基线。

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G5 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g5_01()]
}

/// 真实临时文件路径（OS 临时目录下，进程内固定名）。用例向它 write→read 真实文件 IO。
fn temp_note_path() -> String {
    std::env::temp_dir()
        .join(format!("sophia-e2e-g5-{}.txt", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

/// G5-01：文件写入与回读（File.Write + File.Read + intent 转换边界，真实临时文件）。
///
/// 业务：把一条已脱敏的笔记写入某个路径，再读回它并返回字符数（验证写出后能取回）。
/// 考察：① 有副作用 action 必须声明 `effects { File.Read; File.Write }` 且绑定一个 allow 它们的
/// capability；② `File.Read` 取回的 `Raw<Text>` 不可信，须经 `intent_conversion` 动作转为
/// `Sanitized<Text>` 才能取长度；③ `File.Write` 的 content 必须是 `Sanitized<Text>`。
///
/// 真实 IO：harness 注入 `CliHost`，`File.Write`/`File.Read` 打到真实临时文件（非 mock 桶）。
fn g5_01() -> Case {
    Case {
        id: "G5-01",
        group: "g5",
        kind: CaseKind::DesignImplement,
        title: "笔记写入与回读",
        description: "在 vault 域内：① 定义一个名为 VaultCapability 的 capability，允许 \
                      File.Read 与 File.Write 副作用；② 定义一个 intent_conversion 动作，把 \
                      Raw<Text> 转换为 Sanitized<Text>；③ 定义一个名为 StoreNote 的 action，\
                      绑定 VaultCapability，输入一个文本 path（路径）与一个 Sanitized<Text> \
                      message（已脱敏的笔记），声明 effects 含 File.Read 与 File.Write，先用 \
                      File.Write 把 message 写入 path，再用 File.Read(path) 读回内容、经 intent \
                      转换为可信文本后返回其字符数（Int）。每个 node 各放一个文件。",
        acceptance: &[
            "存在名为 VaultCapability 的 capability，allow File.Read 与 File.Write",
            "存在一个 intent_conversion 动作把 Raw<Text> 转为 Sanitized<Text>",
            "存在名为 StoreNote 的 action，绑定 VaultCapability，声明 effects 含 File.Read 与 File.Write",
            "StoreNote 先 File.Write(path, message) 再 File.Read(path)，经 intent 转换后返回内容字符数 Int",
        ],
        entry_action: "StoreNote",
        // 真实临时文件路径 + "hello"（长度 5）。运行时 intent 被剥离，message 传 Text 即可。
        args: vec![Value::Text(temp_note_path()), Value::Text("hello".into())],
        expect: Expect::Returns(Value::Int(5)),
        expected_console: None,
        max_repairs: 0,
        broken_seed: None,
    }
}
