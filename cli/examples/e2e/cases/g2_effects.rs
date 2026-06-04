//! G2：effect + capability 用例（见 docs/e2e_test.md §4）。
//!
//! 考察 `Console.Write` effect 声明、capability allow 绑定、effect 检查路径（有副作用的
//! action 必须声明 effects 并绑定一个 allow 该 effect 的 capability）。
//!
//! 校验点不止返回值，还包括 **console 输出**——验证 effect 真正经解释器的 effect host
//! 执行（而非仅通过静态检查）。
//!
//! **防答案泄漏**：只写题目（业务需求 + 验收条件）+ 入口 + 期望（返回值 / console）；
//! 不含任何 Sophia 源码答案。effect / capability 的语法是语言事实，由共享语法基线承载。

use crate::harness::{Case, CaseKind, Expect};
use sophia_runtime::Value;

/// G2 全部用例。
pub fn cases() -> Vec<Case> {
    vec![g2_01(), g2_02(), g2_03()]
}

/// G2-01：审计日志（Console.Write effect + capability allow，输出固定提示）。
///
/// 业务：把一条已脱敏的审计消息写到日志（console），并回传它的字符数作为记录长度。
/// 考察：有副作用 action 必须声明 `effects { Console.Write }` 且绑定一个 allow 它的
/// capability；`print` 在解释器经 effect host 真正输出。
///
/// 注意（语言语义）：`Console.Write` 只接受字面量 / `Sanitized<T>` / `Redacted<T>`
/// （intent 边界，§16.4），故审计消息建模为 `Sanitized<Text>`——这本身就是真实场景
/// （审计日志写出的是已脱敏内容）。这是**需求约束**，不是实现提示。
fn g2_01() -> Case {
    Case {
        id: "G2-01",
        group: "g2",
        kind: CaseKind::DesignImplement,
        title: "审计日志写入",
        description: "在 audit 域内提供入口 LogNotice：\
                      接收一条已脱敏、可安全写出的审计消息 message，将该消息写入控制台日志，\
                      并返回 message 的字符数。",
        acceptance: &[
            "入口名为 LogNotice",
            "接收已脱敏的文本 message",
            "控制台日志恰好写出 message，返回 message 的字符数",
        ],
        entry_action: "LogNotice",
        // "hello" 长度 5；打印 "hello"。运行时 intent 被剥离，传 Text 即可。
        args: vec![Value::Text("hello".into())],
        expect: Expect::Returns(Value::Int(5)),
        expected_console: Some(vec!["hello".into()]),
        expected_file_content: None,
        max_repairs: 1,
        broken_seed: None,
    }
}

/// G2-02：双行通知（多次 Console.Write，验证 effect 顺序执行）。
///
/// 业务：依次打印一条问候和一条结束语两行通知，返回总共打印的行数（2）。
/// 考察：多条 print 都经 effect host 顺序执行；effect 只需声明一次。
fn g2_02() -> Case {
    Case {
        id: "G2-02",
        group: "g2",
        kind: CaseKind::DesignImplement,
        title: "双行通知广播",
        description: "在 notify 域内提供入口 Broadcast：\
                      无输入，依次向控制台写出两行固定通知：第一行 \"hello\"，第二行 \"bye\"，\
                      然后返回写出的行数。",
        acceptance: &[
            "入口名为 Broadcast",
            "无输入",
            "控制台日志依次为 \"hello\" 与 \"bye\"，返回 2",
        ],
        entry_action: "Broadcast",
        args: vec![],
        expect: Expect::Returns(Value::Int(2)),
        expected_console: Some(vec!["hello".into(), "bye".into()]),
        expected_file_content: None,
        max_repairs: 1,
        broken_seed: None,
    }
}

/// G2-03：网络获取 + intent 安全（Http.Get + intent_conversion，旗舰 LLM-native 演示）。
///
/// 业务：从一个 URL 取回响应体，把它当**不可信**数据（`Raw<Text>`），经显式 intent 转换为
/// `Sanitized<Text>` 后判断是否非空，返回布尔。考察 F2/S1 + intent 边界：`Http.Get` 返回的
/// `Raw<Text>` **必须**经 `intent_conversion` 动作转换才能使用，否则静态拒绝（reject 半由确定性
/// 单测 `cli/tests/intent_matrix.rs` 钉死）。
///
/// **真实网络（e2e 禁 mock）**：harness 据入口声明的 `Http.Get` effect 注入真实 native host
/// （`reqwest`），真打稳定站点。断言取**稳定属性**——「取回的可信文本非空」而非精确长度
/// （真实响应体长度不稳定），避免脆弱断言。
fn g2_03() -> Case {
    Case {
        id: "G2-03",
        group: "g2",
        kind: CaseKind::DesignImplement,
        title: "网络获取并判空",
        description: "在 ingest 域内提供入口 FetchNonEmpty：\
                      接收一个 url，访问该网络地址取得响应文本。响应来自外部来源，必须按不可信\
                      外部数据处理，只有经过可信化边界后才能继续使用。若可信化后的文本非空，\
                      返回 true，否则返回 false。",
        acceptance: &[
            "入口名为 FetchNonEmpty",
            "接收文本 url",
            "访问 url 取得外部响应文本，并在可信化后判断是否非空",
            "返回布尔值",
        ],
        entry_action: "FetchNonEmpty",
        // 真实稳定站点：example.com（IANA 维护的稳定示例域，响应体非空）。
        args: vec![Value::Text("https://example.com".into())],
        expect: Expect::Returns(Value::Bool(true)),
        expected_console: None,
        expected_file_content: None,
        max_repairs: 0,
        broken_seed: None,
    }
}
