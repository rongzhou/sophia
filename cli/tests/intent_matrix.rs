//! intent accept/reject 矩阵（确定性，不需 LLM / 网络）。见 docs/unit_test.md §四（cli）、
//! docs/e2e_test.md §四 G2-03。
//!
//! 核心主张：经 `Http.Get` 取回的 `Raw<Text>` 是**不可信**数据，下游使用**必须**经显式
//! intent 转换（→ `Sanitized<Text>`）否则 Sophia 编译期**静态拒绝**；而等价 TypeScript（`fetch`
//! 字符串直接当可信值 + `tsc`）会**接受**。这条「静态拒绝不安全候选」本就该是**确定性回归**
//! （检查器对固定程序的裁定），故落 `cargo test`（单元测试）；它与 e2e 的网络获取用例 G2-03
//! 互补——**reject 半**（静态拒绝）在此确定性钉死，**accept 半**（真实取数据跑通）在 e2e 用
//! 真实 IO 验证。
//!
//! 判定直接复用 `sophia_engine::code_check`（集成层 gate，桥接 tools/check 语法 + HIR + 语义三层），
//! 与 graph implement-loop / e2e / benchmark 用的是**同一个** code_check——即此矩阵钉死的正是
//! 真实工作流会用的那道门。

use sophia_engine::code_check;

/// 不安全候选：`Http.Get` 的 `Raw<Text>` **不经转换**直接当 `Sanitized<Text>` 返回。
/// Sophia 必须静态拒绝（intent 严格相等被违反）。
const UNSAFE_DIRECT_RAW: &[(&str, &str)] = &[
    (
        "ingest/capabilities/NetCap.sophia",
        "capability NetCap { allow { Http.Get } }",
    ),
    (
        "ingest/actions/FetchTrusted.sophia",
        // body 直接把 Raw<Text> 当 Sanitized<Text> 返回——不安全。
        "action FetchTrusted {\n\
         capability: NetCap\n\
         input { url: Text }\n\
         output { body: Sanitized<Text> }\n\
         effects { Http.Get }\n\
         body { return Http.Get(url) }\n\
         }",
    ),
];

/// 安全候选：`Http.Get` 的 `Raw<Text>` 经 intent_conversion 动作转为 `Sanitized<Text>` 后返回。
/// Sophia 应接受（通过 code_check）。
const SAFE_VIA_CONVERSION: &[(&str, &str)] = &[
    (
        "ingest/capabilities/NetCap.sophia",
        "capability NetCap { allow { Http.Get } }",
    ),
    (
        "ingest/actions/Trust.sophia",
        "action Trust {\n\
         intent_conversion: true\n\
         input { raw: Raw<Text> }\n\
         output { clean: Sanitized<Text> }\n\
         effects { Pure }\n\
         body { return raw }\n\
         }",
    ),
    (
        "ingest/actions/FetchTrusted.sophia",
        "action FetchTrusted {\n\
         capability: NetCap\n\
         input { url: Text }\n\
         output { body: Sanitized<Text> }\n\
         effects { Http.Get }\n\
         body {\n\
         let raw = Http.Get(url)\n\
         return Trust(raw)\n\
         }\n\
         }",
    ),
];

fn owned(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(p, c)| (p.to_string(), c.to_string()))
        .collect()
}

#[test]
fn sophia_rejects_unsafe_raw_used_as_trusted() {
    // reject 半：不安全候选必须被 code_check 拒绝，且带 intent 诊断。
    let payload = code_check(&owned(UNSAFE_DIRECT_RAW));
    assert!(
        !payload.ok,
        "Raw<Text> 直接当 Sanitized<Text> 必须被静态拒绝：{:?}",
        payload.diagnostics
    );
    assert!(
        payload
            .diagnostics
            .iter()
            .any(|d| d.code == "CHECK-INTENT-001"),
        "应报 intent 不匹配（CHECK-INTENT-001）：{:?}",
        payload.diagnostics
    );
}

#[test]
fn sophia_accepts_raw_via_intent_conversion() {
    // accept 半（静态侧）：经 intent_conversion 转换后的安全候选应通过 code_check。
    let payload = code_check(&owned(SAFE_VIA_CONVERSION));
    assert!(
        payload.ok,
        "经 intent 转换的安全候选应通过 code_check：{:?}",
        payload.diagnostics
    );
}

/// 矩阵对照（文档化，不进测试门禁）：等价 TypeScript 在 `tsc --strict` 下**编译通过**，
/// 因为 TS 没有 intent 类型系统——`fetch` 回来的 `string` 与「可信 `string`」类型上无从区分。
///
/// ```ts
/// // tsc --strict 接受以下代码（string 即 string，无 intent 维度）：
/// async function fetchTrusted(url: string): Promise<string> {
///   const resp = await fetch(url);
///   const body: string = await resp.text(); // 不可信网络数据
///   return body;                            // 直接当可信值返回——tsc 不报错
/// }
/// ```
///
/// 这正是 intent 边界的**分叉点**：相同逻辑，Sophia 静态拒绝（见 `sophia_rejects_unsafe_raw_used_as_trusted`）、
/// TS 接受。`tsc` 是重外部工具链，**不引入测试门禁**（与「benchmark baseline 只做 Python」既有
/// 决策一致，见 docs/benchmark_test.md §三.1）；TS 侧以本文档矩阵条目 + 可复现片段呈现。
#[test]
fn ts_baseline_would_accept_documented() {
    // 本测试仅作矩阵条目的可见锚点：Sophia 拒绝（上）↔ TS 接受（本 doc 注释）。
    // 不执行 tsc；断言占位为 true，真实对照见上方 doc 注释的可复现 TS 片段。
    // 它存在的意义是让矩阵的「TS 接受」半在测试套件里有据可查（与 Sophia reject 半成对）。
    let sophia_rejects = !code_check(&owned(UNSAFE_DIRECT_RAW)).ok;
    assert!(
        sophia_rejects,
        "矩阵前提：Sophia 必须拒绝不安全候选（TS 接受半见 doc 注释）"
    );
}
