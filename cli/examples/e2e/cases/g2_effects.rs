//! G2：effect + capability 用例（见 docs/e2e_test.md §4）。
//!
//! 考察 `Console.Write` effect 声明、capability allow 绑定、effect 检查路径（有副作用的
//! action 必须声明 effects 并绑定一个 allow 该 effect 的 capability）。均为"一次过"用例。
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
        description: "在 audit 域内：① 定义一个名为 AuditCapability 的 capability，允许 \
                      Console.Write 副作用；② 定义一个名为 LogNotice 的 action，绑定该 \
                      capability，输入一个 Sanitized<Text> 参数 message（已脱敏的审计消息），\
                      把 message 打印到控制台（产生 Console.Write 副作用），并返回该 message 的\
                      字符数（Int）。capability 与 action 各放一个文件。",
        acceptance: &[
            "存在名为 AuditCapability 的 capability，allow Console.Write",
            "存在名为 LogNotice 的 action，绑定 AuditCapability，声明 effects 含 Console.Write",
            "LogNotice 输入 Sanitized<Text> message，打印 message，返回其字符数 Int",
        ],
        entry_action: "LogNotice",
        // "hello" 长度 5；打印 "hello"。运行时 intent 被剥离，传 Text 即可。
        args: vec![Value::Text("hello".into())],
        expect: Expect::Returns(Value::Int(5)),
        expected_console: Some(vec!["hello".into()]),
        max_repairs: 0,
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
        description: "在 notify 域内：① 定义一个名为 NotifyCapability 的 capability，允许 \
                      Console.Write；② 定义一个名为 Broadcast 的 action，绑定该 capability，\
                      无输入参数，依次向控制台打印两行固定通知：第一行 \"hello\"，第二行 \
                      \"bye\"，然后返回打印的行数 2（Int）。capability 与 action 各放一个文件。",
        acceptance: &[
            "存在名为 NotifyCapability 的 capability，allow Console.Write",
            "存在名为 Broadcast 的 action，绑定 NotifyCapability，声明 effects 含 Console.Write",
            "Broadcast 依次打印 \"hello\" 与 \"bye\" 两行，返回 Int 2",
        ],
        entry_action: "Broadcast",
        args: vec![],
        expect: Expect::Returns(Value::Int(2)),
        expected_console: Some(vec!["hello".into(), "bye".into()]),
        max_repairs: 0,
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
/// **真实网络（e2e 禁 mock）**：harness 据入口声明的 `Http.Get` effect 注入真实 `CliHost`
/// （`reqwest`），真打稳定站点。断言取**稳定属性**——「取回的可信文本非空」而非精确长度
/// （真实响应体长度不稳定），避免脆弱断言。
fn g2_03() -> Case {
    Case {
        id: "G2-03",
        group: "g2",
        kind: CaseKind::DesignImplement,
        title: "网络获取 + intent 安全",
        description: "在 ingest 域内实现一个从网络取数据的动作：\
                      ① 定义一个名为 IngestCapability 的 capability，允许 Http.Get 副作用；\
                      ② 定义一个 intent_conversion 动作，把 Raw<Text> 转换为 Sanitized<Text>；\
                      ③ 定义一个名为 FetchNonEmpty 的 action，绑定 IngestCapability，输入一个文本 url，\
                      声明 effects 含 Http.Get，用 Http.Get(url) 取回响应体（类型为 Raw<Text>，\
                      不可信），经 intent 转换为可信文本后，判断其字符数是否大于 0，返回布尔。\
                      注意：Http.Get 返回的 Raw<Text> 是不可信数据，必须经 intent_conversion 动作\
                      转换为 Sanitized<Text> 才能当可信值使用——不可直接使用。每个 node 各放一个文件。",
        acceptance: &[
            "存在名为 IngestCapability 的 capability，allow Http.Get",
            "存在一个 intent_conversion 动作把 Raw<Text> 转为 Sanitized<Text>",
            "存在名为 FetchNonEmpty 的 action，绑定 IngestCapability，声明 effects 含 Http.Get",
            "FetchNonEmpty 用 Http.Get(url) 取回 Raw<Text>，经 intent 转换后判断非空，返回 Bool",
        ],
        entry_action: "FetchNonEmpty",
        // 真实稳定站点：example.com（IANA 维护的稳定示例域，响应体非空）。
        args: vec![Value::Text("https://example.com".into())],
        expect: Expect::Returns(Value::Bool(true)),
        expected_console: None,
        max_repairs: 0,
        broken_seed: None,
    }
}
