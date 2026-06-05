//! Semantic IR 三层（type / effect / contract）的集成测试。

use sophia_hir::{AsgIndex, IndexInput, LibraryContent, LibraryRegistry};
use sophia_semantic::{analyze_program, SemanticDiagnosticKind as K};
use sophia_syntax::{parse_ast, Ast};

/// 把若干 (domain, path, source) 解析为 AST。
fn parse_all(sources: &[(&str, &str, &str)]) -> Vec<(String, String, Ast)> {
    sources
        .iter()
        .map(|(d, p, s)| (d.to_string(), p.to_string(), parse_ast(*s).expect("parse")))
        .collect()
}

/// 内联 File + Http 库注册表（semantic 测试不依赖 sophia-stdlib——core 层不反向依赖内容层）。
/// 类型层据此对 `File.Read`/`File.Write`/`Http.Get` 表驱动校验（签名 / intent 边界 / 返回类型）。
fn lib_registry() -> LibraryRegistry {
    LibraryRegistry::build(vec![
        LibraryContent {
            dir_name: "file".into(),
            manifest_toml: r#"
[library]
name = "file"
summary = "读写本地文件"
abi_version = 1
[[op]]
family = "File"
op = "Read"
params = ["Text"]
returns = "Raw<Text>"
host_fn = "file_read"
[[op]]
family = "File"
op = "Write"
params = ["Text", "Sanitized<Text>"]
returns = "Unit"
host_fn = "file_write"
[prompt]
asset = "file.md"
"#
            .into(),
            asset_text: "x".into(),
            sophia_sources: vec![],
            host_wasm: None,
        },
        LibraryContent {
            dir_name: "http".into(),
            manifest_toml: r#"
[library]
name = "http"
summary = "网络获取"
abi_version = 1
[[op]]
family = "Http"
op = "Get"
params = ["Text"]
returns = "Raw<Text>"
host_fn = "http_get"
[prompt]
asset = "http.md"
"#
            .into(),
            asset_text: "x".into(),
            sophia_sources: vec![],
            host_wasm: None,
        },
    ])
    .expect("build lib registry")
}

/// 从解析结果构建 AsgIndex（含 File/Http 库契约）并运行语义分析，返回诊断 kind 列表。
fn analyze(parsed: &[(String, String, Ast)]) -> Vec<K> {
    let index_inputs: Vec<IndexInput> = parsed
        .iter()
        .map(|(d, p, a)| IndexInput {
            domain: d,
            path: p,
            ast: a,
        })
        .collect();
    let registry = lib_registry();
    let index = AsgIndex::build(index_inputs, &registry).expect("index build");
    let asts: Vec<&Ast> = parsed.iter().map(|(_, _, a)| a).collect();
    let analysis = analyze_program(&asts, &index);
    analysis.diagnostics.iter().map(|d| d.kind).collect()
}

fn has(diags: &[K], kind: K) -> bool {
    diags.contains(&kind)
}

#[test]
fn text_parser_methods_typecheck() {
    let src = (
        "D",
        "domains/D/actions/TextOps.sophia",
        r#"action TextOps {
  input { text: Text; prefix: Text }
  output { ok: Bool }
  effects { Pure }
  body {
    let ch = text.char_at(1)
    let piece = text.slice(0, 2)
    return (ch + piece).starts_with(prefix)
  }
}"#,
    );
    let parsed = parse_all(&[src]);
    let diags = analyze(&parsed);
    assert!(
        diags.is_empty(),
        "Text parser methods should typecheck: {diags:?}"
    );
}

#[test]
fn text_parser_methods_reject_bad_arguments() {
    let src = (
        "D",
        "domains/D/actions/BadTextOps.sophia",
        r#"action BadTextOps {
  input { text: Text }
  output { out: Text }
  effects { Pure }
  body {
    let a = text.char_at("x")
    let b = text.slice(0)
    let c = text.starts_with(1)
    return a + b
  }
}"#,
    );
    let parsed = parse_all(&[src]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "bad Text method args should be rejected: {diags:?}"
    );
}

#[test]
fn text_parser_methods_reject_bad_receiver() {
    let src = (
        "D",
        "domains/D/actions/BadTextReceiver.sophia",
        r#"action BadTextReceiver {
  input { n: Int }
  output { out: Text }
  effects { Pure }
  body { return n.char_at(0) }
}"#,
    );
    let parsed = parse_all(&[src]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "Text parser methods should reject non-Text receiver: {diags:?}"
    );
}

#[test]
fn ordering_comparison_requires_int_operands() {
    let src = (
        "D",
        "domains/D/actions/BadTextCompare.sophia",
        r#"action BadTextCompare {
  input { text: Text }
  output { ok: Bool }
  body { return text.slice(0, 1) >= "0" }
}"#,
    );
    let parsed = parse_all(&[src]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "ordering comparisons should reject Text operands before runtime: {diags:?}"
    );
}

#[test]
fn while_condition_must_be_bool() {
    let src = (
        "D",
        "domains/D/actions/BadWhile.sophia",
        r#"action BadWhile {
  input { n: Int }
  output { out: Int }
  effects { Pure }
  body {
    while n {
      return n
    }
    return 0
  }
}"#,
    );
    let parsed = parse_all(&[src]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "while 条件必须拒绝非 Bool：{diags:?}"
    );
}

#[test]
fn ensures_output_record_access_is_clean() {
    // ensures 用 `output.<param>.<field>`（设计第五节示例形式），不应误报 NoSuchField。
    let state = (
        "D",
        "domains/D/states/S.sophia",
        "state S { value A { meaning: \"a\" } value B { meaning: \"b\" } }",
    );
    let ent = (
        "D",
        "domains/D/entities/T.sophia",
        "entity T { fields { status { type: S } } }",
    );
    let act = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: T }
  output { todo: T }
  body { return x }
  ensures { output.todo.status == S.B }
}"#,
    );
    let parsed = parse_all(&[state, ent, act]);
    let diags = analyze(&parsed);
    assert!(
        !has(&diags, K::NoSuchField),
        "output 记录字段访问不应误报：{diags:?}"
    );
}

#[test]
fn output_where_predicate_sees_output_param() {
    // output 的 where 谓词中，output 参数自身可见。
    let state = (
        "D",
        "domains/D/states/S.sophia",
        "state S { value A { meaning: \"a\" } value B { meaning: \"b\" } }",
    );
    let ent = (
        "D",
        "domains/D/entities/T.sophia",
        "entity T { fields { status { type: S } } }",
    );
    let act = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: T }
  output { t: T where t.status == S.B }
  body { return x }
}"#,
    );
    let parsed = parse_all(&[state, ent, act]);
    let diags = analyze(&parsed);
    assert!(
        diags.is_empty(),
        "output where 谓词应可见 output 参数：{diags:?}"
    );
}

#[test]
fn set_type_mismatch_reported() {
    // set 把 Text 赋给 Int mutable 变量，应报类型不匹配。
    let act = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  body {
    let mutable v = 0
    set v = "hi"
    return v
  }
}"#,
    );
    let parsed = parse_all(&[act]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "set 类型不匹配应报：{diags:?}"
    );
}

#[test]
fn well_typed_action_is_clean() {
    let entity = (
        "D",
        "domains/D/entities/Acc.sophia",
        r#"entity Acc {
  fields {
    balance { type: Int }
    label { type: Sanitized<Text> }
  }
}"#,
    );
    // action：构造 Acc，返回它；声明 Console.Write + capability。
    let cap = (
        "D",
        "domains/D/capabilities/C.sophia",
        r#"capability C {
  allow { Console.Write }
}"#,
    );
    let action = (
        "D",
        "domains/D/actions/Make.sophia",
        r#"action Make {
  capability: C
  input { amount: Int; name: Sanitized<Text> }
  output { acc: Acc }
  effects { Console.Write }
  body {
    print name
    return Acc { balance = amount, label = name }
  }
}"#,
    );
    let parsed = parse_all(&[entity, cap, action]);
    let diags = analyze(&parsed);
    assert!(diags.is_empty(), "良类型 action 不应有诊断：{diags:?}");
}

#[test]
fn missing_field_in_construction_reported() {
    let entity = (
        "D",
        "domains/D/entities/Acc.sophia",
        "entity Acc { fields { balance { type: Int } locked { type: Bool } } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { acc: Acc }
  body { return Acc { balance = x } }
}"#,
    );
    let parsed = parse_all(&[entity, action]);
    let diags = analyze(&parsed);
    assert!(has(&diags, K::MissingField), "应报缺字段：{diags:?}");
}

#[test]
fn unknown_field_in_construction_reported() {
    let entity = (
        "D",
        "domains/D/entities/Acc.sophia",
        "entity Acc { fields { balance { type: Int } } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { acc: Acc }
  body { return Acc { balance = x, ghost = x } }
}"#,
    );
    let parsed = parse_all(&[entity, action]);
    let diags = analyze(&parsed);
    assert!(has(&diags, K::UnknownField), "应报未知字段：{diags:?}");
}

#[test]
fn field_type_mismatch_reported() {
    let entity = (
        "D",
        "domains/D/entities/Acc.sophia",
        "entity Acc { fields { balance { type: Int } } }",
    );
    // 把 Text 赋给 Int 字段。
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { s: Text }
  output { acc: Acc }
  body { return Acc { balance = s } }
}"#,
    );
    let parsed = parse_all(&[entity, action]);
    let diags = analyze(&parsed);
    assert!(has(&diags, K::TypeMismatch), "应报类型不匹配：{diags:?}");
}

#[test]
fn transition_construct_missing_input_reported() {
    let transition = (
        "D",
        "domains/D/transitions/Inc.sophia",
        r#"transition Inc {
  input { x: Int; delta: Int }
  output { y: Int }
  body { return x + delta }
}"#,
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  body { return Inc { x = x } }
}"#,
    );
    let parsed = parse_all(&[transition, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::MissingField),
        "transition 构造缺 input 应报 MissingField：{diags:?}"
    );
}

#[test]
fn transition_construct_unknown_input_reported() {
    let transition = (
        "D",
        "domains/D/transitions/Inc.sophia",
        r#"transition Inc {
  input { x: Int }
  output { y: Int }
  body { return x + 1 }
}"#,
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  body { return Inc { x = x, ghost = x } }
}"#,
    );
    let parsed = parse_all(&[transition, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::UnknownField),
        "transition 构造未知 input 应报 UnknownField：{diags:?}"
    );
}

#[test]
fn transition_construct_input_type_mismatch_reported() {
    let transition = (
        "D",
        "domains/D/transitions/Inc.sophia",
        r#"transition Inc {
  input { x: Int }
  output { y: Int }
  body { return x + 1 }
}"#,
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { s: Text }
  output { y: Int }
  body { return Inc { x = s } }
}"#,
    );
    let parsed = parse_all(&[transition, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "transition 构造 input 类型不匹配应报 TypeMismatch：{diags:?}"
    );
}

#[test]
fn intent_strict_equality_violation_reported() {
    // 字段要 Sanitized<Text>，传入 Raw<Text> —— intent 严格相等被违反。
    let entity = (
        "D",
        "domains/D/entities/Acc.sophia",
        "entity Acc { fields { label { type: Sanitized<Text> } } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { r: Raw<Text> }
  output { acc: Acc }
  body { return Acc { label = r } }
}"#,
    );
    let parsed = parse_all(&[entity, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IntentMismatch),
        "应报 intent 不匹配：{diags:?}"
    );
}

#[test]
fn missing_return_path_reported() {
    // if 只有 then 分支 return，else 缺失 → 存在 fallthrough 路径。
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { b: Bool }
  output { y: Int }
  body {
    if b {
      return 1
    }
  }
}"#,
    );
    let parsed = parse_all(&[action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::MissingReturn),
        "应报缺 return 路径：{diags:?}"
    );
}

#[test]
fn both_branches_return_is_complete() {
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { b: Bool }
  output { y: Int }
  body {
    if b {
      return 1
    } else {
      return 2
    }
  }
}"#,
    );
    let parsed = parse_all(&[action]);
    let diags = analyze(&parsed);
    assert!(
        !has(&diags, K::MissingReturn),
        "双分支 return 不应报缺 return：{diags:?}"
    );
}

#[test]
fn undeclared_effect_reported() {
    // body 用 print（Console.Write），但 effects 未声明。
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  body {
    print "hi"
    return x
  }
}"#,
    );
    let parsed = parse_all(&[action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::UndeclaredEffect),
        "应报未声明 effect：{diags:?}"
    );
    assert!(
        has(&diags, K::PureConflict),
        "声明为纯却有 effect 应报 Pure 冲突：{diags:?}"
    );
}

#[test]
fn capability_denies_effect() {
    // capability deny Console.Write，但 action 声明并使用它。
    let cap = (
        "D",
        "domains/D/capabilities/C.sophia",
        r#"capability C {
  allow { Console.Write }
  deny { Console.Write }
}"#,
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  capability: C
  input { x: Int }
  output { y: Int }
  effects { Console.Write }
  body {
    print "hi"
    return x
  }
}"#,
    );
    let parsed = parse_all(&[cap, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::CapabilityDenied),
        "deny 优先应报 capability 拒绝：{diags:?}"
    );
}

#[test]
fn effect_without_capability_reported() {
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  effects { Console.Write }
  body {
    print "hi"
    return x
  }
}"#,
    );
    let parsed = parse_all(&[action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::MissingCapability),
        "有 effect 无 capability 应报：{diags:?}"
    );
}

#[test]
fn undeclared_raise_reported() {
    let err = (
        "D",
        "domains/D/errors/E.sophia",
        "error E { variant Bad { reason: Text } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  body {
    raise Bad { reason = "no" }
  }
}"#,
    );
    let parsed = parse_all(&[err, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::UndeclaredError),
        "raise 未声明 variant 应报：{diags:?}"
    );
}

#[test]
fn declared_raise_is_ok() {
    let err = (
        "D",
        "domains/D/errors/E.sophia",
        "error E { variant Bad { reason: Text } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { x: Int }
  output { y: Int }
  errors { Bad }
  body {
    raise Bad { reason = "no" }
  }
}"#,
    );
    let parsed = parse_all(&[err, action]);
    let diags = analyze(&parsed);
    assert!(
        !has(&diags, K::UndeclaredError),
        "已声明 variant 的 raise 不应报：{diags:?}"
    );
}

#[test]
fn error_variant_missing_field_reported() {
    let err = (
        "D",
        "domains/D/errors/E.sophia",
        "error E { variant Bad { reason: Text } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  output { y: Int }
  errors { Bad }
  body {
    raise Bad { }
  }
}"#,
    );
    let parsed = parse_all(&[err, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::MissingField),
        "variant 构造缺字段应报 MissingField：{diags:?}"
    );
}

#[test]
fn error_variant_unknown_field_reported() {
    let err = (
        "D",
        "domains/D/errors/E.sophia",
        "error E { variant Bad { reason: Text } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  output { y: Int }
  errors { Bad }
  body {
    raise Bad { reason = "no", ghost = 1 }
  }
}"#,
    );
    let parsed = parse_all(&[err, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::UnknownField),
        "variant 构造未知字段应报 UnknownField：{diags:?}"
    );
}

#[test]
fn error_variant_field_type_mismatch_reported() {
    let err = (
        "D",
        "domains/D/errors/E.sophia",
        "error E { variant Bad { code: Int } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  output { y: Int }
  errors { Bad }
  body {
    raise Bad { code = "no" }
  }
}"#,
    );
    let parsed = parse_all(&[err, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "variant 构造字段类型不匹配应报 TypeMismatch：{diags:?}"
    );
}

#[test]
fn non_exhaustive_state_match_reported() {
    let state = (
        "D",
        "domains/D/states/S.sophia",
        "state S { value A { meaning: \"a\" } value B { meaning: \"b\" } }",
    );
    // match 只覆盖 A，缺 B。
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { s: S }
  output { y: Int }
  body {
    match s {
      S.A => return 1
    }
  }
}"#,
    );
    let parsed = parse_all(&[state, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::NonExhaustiveMatch),
        "state match 不穷尽应报：{diags:?}"
    );
}

#[test]
fn exhaustive_state_match_is_ok() {
    let state = (
        "D",
        "domains/D/states/S.sophia",
        "state S { value A { meaning: \"a\" } value B { meaning: \"b\" } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  input { s: S }
  output { y: Int }
  body {
    match s {
      S.A => return 1
      S.B => return 2
    }
  }
}"#,
    );
    let parsed = parse_all(&[state, action]);
    let diags = analyze(&parsed);
    assert!(
        !has(&diags, K::NonExhaustiveMatch),
        "穷尽 state match 不应报：{diags:?}"
    );
    assert!(
        !has(&diags, K::MissingReturn),
        "全 arm return 应视为完整：{diags:?}"
    );
}

#[test]
fn console_write_rejects_raw_intent() {
    // print 一个 Raw<Text> 变量 —— 违反 Console 输出边界。
    let cap = (
        "D",
        "domains/D/capabilities/C.sophia",
        "capability C { allow { Console.Write } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  capability: C
  input { r: Raw<Text> }
  output { y: Int }
  effects { Console.Write }
  body {
    print r
    return y
  }
}"#,
    );
    let parsed = parse_all(&[cap, action]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::ConsoleOutputIntent),
        "Console 输出 Raw 应报：{diags:?}"
    );
}

#[test]
fn console_write_accepts_sanitized() {
    let cap = (
        "D",
        "domains/D/capabilities/C.sophia",
        "capability C { allow { Console.Write } }",
    );
    let action = (
        "D",
        "domains/D/actions/M.sophia",
        r#"action M {
  capability: C
  input { s: Sanitized<Text> }
  output { y: Int }
  effects { Console.Write }
  body {
    print s
    return y
  }
}"#,
    );
    let parsed = parse_all(&[cap, action]);
    let diags = analyze(&parsed);
    assert!(
        !has(&diags, K::ConsoleOutputIntent),
        "Console 输出 Sanitized 不应报：{diags:?}"
    );
}

#[test]
fn called_action_errors_must_propagate() {
    let err = (
        "D",
        "domains/D/errors/E.sophia",
        "error E { variant Bad { reason: Text } }",
    );
    let callee = (
        "D",
        "domains/D/actions/Inner.sophia",
        r#"action Inner {
  input { x: Int }
  output { y: Int }
  errors { Bad }
  body {
    raise Bad { reason = "x" }
  }
}"#,
    );
    // Outer 调用 Inner 但未声明 Bad。
    let caller = (
        "D",
        "domains/D/actions/Outer.sophia",
        r#"action Outer {
  input { x: Int }
  output { y: Int }
  body {
    let r = Inner(x)
    return r
  }
}"#,
    );
    let parsed = parse_all(&[err, callee, caller]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::ErrorNotPropagated),
        "被调用方 error 未传播应报：{diags:?}"
    );
}

#[test]
fn canonical_todo_domain_analyzes_clean() {
    // 文档规范示例（TodoDomain）的核心子集：entity / state / error / capability /
    // storage / action，应通过全部三层检查（CompleteTodo 用 transition 调用与
    // storage 操作，属于扩展子集，这里用起步子集内的等价 action 表达）。
    let entity = (
        "TodoDomain",
        "domains/TodoDomain/entities/Todo.sophia",
        r#"entity Todo {
  meaning: "A Todo."
  fields {
    title { type: Sanitized<Text> }
    status { type: TodoStatus }
  }
}"#,
    );
    let state = (
        "TodoDomain",
        "domains/TodoDomain/states/TodoStatus.sophia",
        r#"state TodoStatus {
  value Pending { meaning: "p" }
  value Done { meaning: "d" }
}"#,
    );
    let error = (
        "TodoDomain",
        "domains/TodoDomain/errors/TodoError.sophia",
        r#"error TodoError {
  variant AlreadyDone { reason: Text }
}"#,
    );
    let cap = (
        "TodoDomain",
        "domains/TodoDomain/capabilities/TodoCapability.sophia",
        r#"capability TodoCapability {
  allow { Console.Write }
}"#,
    );
    let action = (
        "TodoDomain",
        "domains/TodoDomain/actions/Complete.sophia",
        r#"action Complete {
  meaning: "Complete a todo."
  capability: TodoCapability
  input { todo: Todo }
  output { todo: Todo }
  effects { Console.Write }
  errors { AlreadyDone }
  body {
    match todo.status {
      TodoStatus.Done => raise AlreadyDone { reason = "done" }
      TodoStatus.Pending => return Todo { title = todo.title, status = TodoStatus.Done }
    }
  }
}"#,
    );
    let parsed = parse_all(&[entity, state, error, cap, action]);
    let diags = analyze(&parsed);
    assert!(
        diags.is_empty(),
        "规范 TodoDomain 子集应通过三层检查：{diags:?}"
    );
}

// ---- intent_conversion 结构约束（设计 7.2） ----

#[test]
fn valid_intent_conversion_is_clean() {
    let act = (
        "D",
        "domains/D/actions/Sanitize.sophia",
        r#"action Sanitize {
  intent_conversion: true
  input  { raw: Raw<Text> }
  output { clean: Sanitized<Text> }
  effects { Pure }
  body { return raw }
}"#,
    );
    let parsed = parse_all(&[act]);
    let diags = analyze(&parsed);
    assert!(
        diags.is_empty(),
        "合法 intent_conversion 不应有诊断：{diags:?}"
    );
}

#[test]
fn intent_conversion_same_intent_reported() {
    // 输入输出 intent 相同 → 未发生转换。
    let act = (
        "D",
        "domains/D/actions/NoOp.sophia",
        r#"action NoOp {
  intent_conversion: true
  input  { x: Raw<Text> }
  output { y: Raw<Text> }
  effects { Pure }
  body { return x }
}"#,
    );
    let parsed = parse_all(&[act]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::InvalidIntentConversion),
        "相同 intent 应报 InvalidIntentConversion：{diags:?}"
    );
}

#[test]
fn intent_conversion_different_inner_reported() {
    let act = (
        "D",
        "domains/D/actions/Bad.sophia",
        r#"action Bad {
  intent_conversion: true
  input  { x: Raw<Text> }
  output { y: Sanitized<Int> }
  effects { Pure }
  body { return x }
}"#,
    );
    let parsed = parse_all(&[act]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::InvalidIntentConversion),
        "inner 类型不同应报：{diags:?}"
    );
}

#[test]
fn intent_conversion_with_effect_reported() {
    // intent_conversion 不得有 effect。
    let cap = (
        "D",
        "domains/D/capabilities/C.sophia",
        r#"capability C { allow { Console.Write } deny { } }"#,
    );
    let act = (
        "D",
        "domains/D/actions/Bad.sophia",
        r#"action Bad {
  intent_conversion: true
  capability: C
  input  { x: Raw<Text> }
  output { y: Sanitized<Text> }
  effects { Console.Write }
  body { print x return x }
}"#,
    );
    let parsed = parse_all(&[cap, act]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::InvalidIntentConversion),
        "有 effect 应报：{diags:?}"
    );
}

#[test]
fn intent_conversion_body_not_passthrough_reported() {
    // body 必须直接 return 输入值。
    let act = (
        "D",
        "domains/D/actions/Bad.sophia",
        r#"action Bad {
  intent_conversion: true
  input  { x: Raw<Text> }
  output { y: Sanitized<Text> }
  effects { Pure }
  body { let z = x return z }
}"#,
    );
    let parsed = parse_all(&[act]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::InvalidIntentConversion),
        "body 非直通应报：{diags:?}"
    );
}

// ---- body 级 storage 操作（§16.6 扩展子集） ----
// ---- 标准库 File：File.Read / File.Write effect 族（见 docs/file_lib.md） ----

#[test]
fn file_read_write_typechecks_clean() {
    // File.Read(path) -> Raw<Text>（经 intent 转换后用）；File.Write(path, Sanitized<Text>) -> Unit；
    // 声明 File.Read/Write effect + capability allow 即通过。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/C.sophia",
            r#"capability C { allow { File.Read; File.Write } deny { } }"#,
        ),
        (
            "D",
            "domains/D/actions/Trust.sophia",
            r#"action Trust { intent_conversion: true input { raw: Raw<Text> } output { clean: Sanitized<Text> } effects { Pure } body { return raw } }"#,
        ),
        (
            "D",
            "domains/D/actions/Op.sophia",
            r#"action Op {
  capability: C
  input { path: Text; content: Sanitized<Text> }
  output { len: Int }
  effects { File.Read; File.Write }
  body {
    File.Write(path, content)
    let raw = File.Read(path)
    let clean = Trust(raw)
    return clean.length
  }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(diags.is_empty(), "合法 File 操作不应有诊断：{diags:?}");
}

#[test]
fn file_op_without_declared_effect_reported() {
    // 用了 File.Read 但 effects 没声明 File.Read → UndeclaredEffect。
    let parsed = parse_all(&[(
        "D",
        "domains/D/actions/Op.sophia",
        r#"action Op {
  input { path: Text }
  output { r: Raw<Text> }
  body { return File.Read(path) }
}"#,
    )]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::UndeclaredEffect),
        "未声明 File.Read 应报 UndeclaredEffect：{diags:?}"
    );
}

#[test]
fn file_read_raw_used_directly_reported() {
    // File.Read 的 Raw<Text> 不经转换直接当 Sanitized<Text> 输出 → IntentMismatch。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/C.sophia",
            r#"capability C { allow { File.Read } deny { } }"#,
        ),
        (
            "D",
            "domains/D/actions/Op.sophia",
            r#"action Op {
  capability: C
  input { path: Text }
  output { content: Sanitized<Text> }
  effects { File.Read }
  body { return File.Read(path) }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IntentMismatch),
        "Raw 直接当可信值应报 IntentMismatch：{diags:?}"
    );
}

#[test]
fn file_write_raw_content_reported() {
    // File.Write 的 content 须为 Sanitized<Text>，传 Raw<Text> → IntentMismatch。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/C.sophia",
            r#"capability C { allow { File.Write } deny { } }"#,
        ),
        (
            "D",
            "domains/D/actions/Op.sophia",
            r#"action Op {
  capability: C
  input { path: Text; raw: Raw<Text> }
  output { done: Bool }
  effects { File.Write }
  body { File.Write(path, raw) return true }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IntentMismatch),
        "File.Write 收 Raw content 应报 IntentMismatch：{diags:?}"
    );
}

/// Semantic IR 声明模型形式核心指纹的确定性快照。
///
/// `formal_fingerprint`（确定性 Debug、无 span、无 assist、BTreeMap 稳定排序）是 strip-assist
/// 门禁与等价比对的形式核心。用一个含 entity / state / error / action 的程序快照它，守护
/// 声明模型的规范化结构不被静默改动。
#[test]
fn semantic_model_fingerprint_snapshot() {
    use sophia_semantic::SemanticModel;
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/states/Status.sophia",
            "state Status { value Open { meaning: \"开\" } value Closed { meaning: \"关\" } }",
        ),
        (
            "D",
            "domains/D/entities/Item.sophia",
            "entity Item { fields { id { type: Int } label { type: Text } state { type: Status } } }",
        ),
        (
            "D",
            "domains/D/errors/ItemError.sophia",
            "error ItemError { variant NotFound { id: Int } }",
        ),
        (
            "D",
            "domains/D/actions/CloseItem.sophia",
            "action CloseItem { input { item: Item } output { result: Status } errors { NotFound } \
             body { return Status.Closed } }",
        ),
    ]);
    let index_inputs: Vec<IndexInput> = parsed
        .iter()
        .map(|(d, p, a)| IndexInput {
            domain: d,
            path: p,
            ast: a,
        })
        .collect();
    let index = AsgIndex::build(index_inputs, &LibraryRegistry::empty()).expect("index build");
    let asts: Vec<&Ast> = parsed.iter().map(|(_, _, a)| a).collect();
    let model = SemanticModel::build(&asts, &index);
    insta::assert_snapshot!("semantic_model_fingerprint", model.formal_fingerprint());
}

#[test]
fn one_of_duplicate_scalar_members_reported() {
    // one of { Int, Int }：两个同类型标量，按 tag 不可区分 → IndistinguishableUnion。
    let parsed = parse_all(&[(
        "D",
        "domains/D/actions/Op.sophia",
        r#"action Op {
  input { x: Int }
  output { y: one of { Int, Int } }
  body { return x }
}"#,
    )]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IndistinguishableUnion),
        "one of {{ Int, Int }} 应报 IndistinguishableUnion：{diags:?}"
    );
}

#[test]
fn one_of_intent_erasure_collision_reported() {
    // one of { Raw<Text>, Text }：intent 运行时擦除，底层同为 Text → 不可区分。
    let parsed = parse_all(&[(
        "D",
        "domains/D/actions/Op.sophia",
        r#"action Op {
  input { x: Text }
  output { y: one of { Raw<Text>, Text } }
  body { return x }
}"#,
    )]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IndistinguishableUnion),
        "one of {{ Raw<Text>, Text }} 应报 IndistinguishableUnion：{diags:?}"
    );
}

#[test]
fn one_of_distinguishable_members_clean() {
    // one of { Int, Null }、one of { Int, SomeVariant } 都两两可区分 → 无诊断。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/errors/E.sophia",
            "error E { variant Bad { reason: Text } }",
        ),
        (
            "D",
            "domains/D/actions/Op.sophia",
            r#"action Op {
  input { x: Int }
  output { y: one of { Int, Bad } }
  body { return x }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        !has(&diags, K::IndistinguishableUnion),
        "可区分的 one of 不应报：{diags:?}"
    );
}

#[test]
fn one_of_duplicate_entity_members_reported() {
    // one of { Item, Item }：两个同名 entity，不可区分。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/entities/Item.sophia",
            "entity Item { fields { id { type: Int } } }",
        ),
        (
            "D",
            "domains/D/actions/Op.sophia",
            r#"action Op {
  input { x: Int }
  output { y: one of { Item, Item } }
  body { return Item { id = x } }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IndistinguishableUnion),
        "one of {{ Item, Item }} 应报 IndistinguishableUnion：{diags:?}"
    );
}

// ---- F2：Http.Get effect 族（见 docs/http_lib.md） ----

#[test]
fn http_get_returns_raw_text_clean() {
    // Http.Get(url) -> Raw<Text>，声明 effects + capability + 经 intent 转换 → 无诊断。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/Net.sophia",
            r#"capability Net { allow { Http.Get } }"#,
        ),
        (
            "D",
            "domains/D/actions/Sanitize.sophia",
            r#"action Sanitize {
  intent_conversion: true
  input { raw: Raw<Text> }
  output { clean: Sanitized<Text> }
  effects { Pure }
  body { return raw }
}"#,
        ),
        (
            "D",
            "domains/D/actions/Fetch.sophia",
            r#"action Fetch {
  capability: Net
  input { url: Text }
  output { body: Sanitized<Text> }
  effects { Http.Get }
  body {
    let raw = Http.Get(url)
    return Sanitize(raw)
  }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(diags.is_empty(), "合法 Http.Get 流程不应有诊断：{diags:?}");
}

#[test]
fn http_get_raw_used_directly_reported() {
    // D2 reject：Http.Get 的 Raw<Text> 直接当 Sanitized<Text> 输出 → IntentMismatch。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/Net.sophia",
            r#"capability Net { allow { Http.Get } }"#,
        ),
        (
            "D",
            "domains/D/actions/FetchBad.sophia",
            r#"action FetchBad {
  capability: Net
  input { url: Text }
  output { body: Sanitized<Text> }
  effects { Http.Get }
  body {
    let raw = Http.Get(url)
    return raw
  }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::IntentMismatch),
        "Raw 直接当可信值应报 IntentMismatch：{diags:?}"
    );
}

#[test]
fn http_get_undeclared_effect_reported() {
    // 用了 Http.Get 但 effects 没声明 → UndeclaredEffect。
    let parsed = parse_all(&[(
        "D",
        "domains/D/actions/Fetch.sophia",
        r#"action Fetch {
  input { url: Text }
  output { body: Raw<Text> }
  body { return Http.Get(url) }
}"#,
    )]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::UndeclaredEffect),
        "未声明 Http.Get 应报 UndeclaredEffect：{diags:?}"
    );
}

#[test]
fn http_get_without_capability_reported() {
    // 声明了 effect 但无 capability 绑定 → MissingCapability。
    let parsed = parse_all(&[(
        "D",
        "domains/D/actions/Fetch.sophia",
        r#"action Fetch {
  input { url: Text }
  output { body: Raw<Text> }
  effects { Http.Get }
  body { return Http.Get(url) }
}"#,
    )]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::MissingCapability),
        "无 capability 应报 MissingCapability：{diags:?}"
    );
}

#[test]
fn http_get_capability_denied_reported() {
    // capability 未 allow Http.Get → CapabilityDenied。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/Empty.sophia",
            r#"capability Empty { allow { Console.Write } }"#,
        ),
        (
            "D",
            "domains/D/actions/Fetch.sophia",
            r#"action Fetch {
  capability: Empty
  input { url: Text }
  output { body: Raw<Text> }
  effects { Http.Get }
  body { return Http.Get(url) }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::CapabilityDenied),
        "capability 未 allow Http.Get 应报 CapabilityDenied：{diags:?}"
    );
}

#[test]
fn http_get_non_text_url_reported() {
    // url 实参非 Text（传 Int）→ TypeMismatch。
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/capabilities/Net.sophia",
            r#"capability Net { allow { Http.Get } }"#,
        ),
        (
            "D",
            "domains/D/actions/Fetch.sophia",
            r#"action Fetch {
  capability: Net
  input { n: Int }
  output { body: Raw<Text> }
  effects { Http.Get }
  body { return Http.Get(n) }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "Http.Get(Int) 应报 TypeMismatch：{diags:?}"
    );
}

#[test]
fn callable_missing_argument_reported() {
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/actions/Add.sophia",
            r#"action Add {
  input { a: Int; b: Int }
  output { total: Int }
  body { return a + b }
}"#,
        ),
        (
            "D",
            "domains/D/actions/Main.sophia",
            r#"action Main {
  output { total: Int }
  body { return Add(1) }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "少传 callable 参数应报 TypeMismatch：{diags:?}"
    );
}

#[test]
fn callable_extra_argument_reported() {
    let parsed = parse_all(&[
        (
            "D",
            "domains/D/actions/Id.sophia",
            r#"action Id {
  input { a: Int }
  output { value: Int }
  body { return a }
}"#,
        ),
        (
            "D",
            "domains/D/actions/Main.sophia",
            r#"action Main {
  output { value: Int }
  body { return Id(1, 2) }
}"#,
        ),
    ]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "多传 callable 参数应报 TypeMismatch：{diags:?}"
    );
}

#[test]
fn builtin_to_text_extra_argument_reported() {
    let parsed = parse_all(&[(
        "D",
        "domains/D/actions/Textify.sophia",
        r#"action Textify {
  output { text: Text }
  body { return to_text(1, 2) }
}"#,
    )]);
    let diags = analyze(&parsed);
    assert!(
        has(&diags, K::TypeMismatch),
        "to_text 多传参数应报 TypeMismatch：{diags:?}"
    );
}
