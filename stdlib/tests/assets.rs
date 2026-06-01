//! 标准库提示词资产 / 目录 / 按需注入测试（见 docs/stdlib_design.md §三）。
//!
//! 自 prompt crate 迁来（库内容已归 sophia-stdlib）：库目录 / 资产 / preamble 经
//! `LibraryRegistry` 提供；防泄漏纪律（资产不含任务 token）同基线，由本测试守护。

use sophia_stdlib::standard_registry;

/// e2e / benchmark 任务相关 token（领域名 / 节点名 / 状态值 / 业务逻辑）。
/// 标准库资产 / 目录一律不得出现这些 token（防答案泄漏，架构 8.3；与 prompt crate 语法基线同纪律）。
const FORBIDDEN_TASK_TOKENS: &[&str] = &[
    "IncrementCounter",
    "CounterDomain",
    "TodoStatus",
    "TodoDomain",
    "CompleteTodo",
    "Pending",
    "Done",
    "current + 1",
    "AuditCapability",
    "LogNotice",
    "NotifyCapability",
    "Broadcast",
    "LineTotal",
    "CartItem",
    "QualifiesForFreeShipping",
    "IngestCapability",
    "FetchNonEmpty",
    "DeductStock",
    "OrderTotal",
    "LineSubtotal",
    "WalletError",
    "InsufficientFunds",
    "Withdraw",
    "ReadingStore",
    "MeterCapability",
    "RecordReading",
    "meter_id",
    "VaultCapability",
    "StoreNote",
    "CelsiusToScaled",
    "FahrenheitOffset",
    "climate",
    "AbsDifference",
    "WithinBudget",
    "RectangleArea",
    "TrafficLight",
    "NextLight",
    "NetTotal",
    "GrossTotal",
    "RemoveStock",
    "StockError",
    "Insufficient",
    "OrderLine",
    "LineAmount",
    "CreditError",
    "OverLimit",
    "Checkout",
    "ClampOrReject",
    "RangeError",
    "OutOfRange",
];

#[test]
fn catalog_lists_http_and_file_without_signatures() {
    // 库目录（design 阶段注入）：每库一行「名 — 用途」，含 file / http、不含操作签名 / 任务 token。
    let catalog = standard_registry().catalog();
    assert!(catalog.contains("`http`"), "目录应含 http 行：{catalog}");
    assert!(catalog.contains("`file`"), "目录应含 file 行：{catalog}");
    assert!(
        !catalog.contains("Http.Get") && !catalog.contains("File.Read"),
        "目录是极简介绍，不含操作签名（那是 implement 完整资产）：{catalog}"
    );
    for forbidden in FORBIDDEN_TASK_TOKENS {
        assert!(
            !catalog.contains(forbidden),
            "库目录泄漏了任务相关 token `{forbidden}`"
        );
    }
}

#[test]
fn catalog_is_deterministic_lexicographic() {
    // 库名字典序：file 在 http 前。
    let catalog = standard_registry().catalog();
    let file_pos = catalog.find("`file`").unwrap();
    let http_pos = catalog.find("`http`").unwrap();
    assert!(file_pos < http_pos, "目录应库名字典序：{catalog}");
}

#[test]
fn preamble_selects_on_demand() {
    let reg = standard_registry();
    // 声明 ["http"] → 含 Http 段；空集 → 空串（零注入）；未知库 → 忽略不 panic。
    let http = reg.preamble(&["http"]);
    assert!(http.contains("Http.Get"), "声明 http 应注入 Http 库资产");

    assert_eq!(
        reg.preamble(&[]),
        "",
        "空集应零注入（默认 = 纯语法基线，零回归）"
    );
    assert_eq!(
        reg.preamble(&["no_such_lib"]),
        "",
        "未知库应被忽略，返回空串"
    );

    // 去重 + 字典序：重复 http 不重复注入。
    assert_eq!(reg.preamble(&["http", "http"]), http, "重复库名应去重");
}

#[test]
fn assets_carry_no_task_answer_tokens() {
    // 库资产与常驻基线同防泄漏纪律：不含任何任务 token。
    let reg = standard_registry();
    for lib in reg.lib_names() {
        let asset = reg.prompt_asset(lib).unwrap();
        for forbidden in FORBIDDEN_TASK_TOKENS {
            assert!(
                !asset.asset_text.contains(forbidden),
                "标准库资产 `{lib}` 泄漏了任务相关 token `{forbidden}`"
            );
        }
    }
}
