//! HIR 名称解析与 scope 分析的集成测试。

use sophia_hir::{
    resolve_program, resolve_program_with_libraries, HirDiagnosticKind, HirError, LibraryContent,
    LibraryRegistry, NodeKind, ProgramInput,
};
use sophia_syntax::{parse_ast, Ast};

/// 内联 Http 库注册表（hir 测试不依赖 sophia-stdlib——core 层不反向依赖内容层，故就近建清单）。
fn http_registry() -> LibraryRegistry {
    LibraryRegistry::build(vec![LibraryContent {
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
    }])
    .expect("build http registry")
}

/// 构造规范 TodoDomain 的多文件程序输入（每个节点一个文件）。
fn todo_program_sources() -> Vec<(&'static str, &'static str, String)> {
    let entity = r#"entity Todo {
  fields {
    id { type: Uuid }
    title { type: Sanitized<Text> }
    status { type: TodoStatus }
    completed_at { type: one of { Time, Null } }
  }
}"#;
    let state = r#"state TodoStatus {
  value Pending { meaning: "未完成" }
  value Done { meaning: "已完成" }
}"#;
    let error = r#"error TodoError {
  variant TodoAlreadyDone { id: Uuid }
}"#;
    let capability = r#"capability TodoCapability {
  allow { Console.Write }
}"#;
    let action = r#"action CompleteTodo {
  capability: TodoCapability
  input { todo: Todo }
  output { todo: Todo }
  effects { Console.Write }
  errors { TodoAlreadyDone }
  body {
    match todo.status {
      TodoStatus.Done => raise TodoAlreadyDone { id = todo.id }
      TodoStatus.Pending => {
        print "completing"
        return todo
      }
    }
  }
}"#;
    vec![
        (
            "TodoDomain",
            "domains/TodoDomain/entities/Todo.sophia",
            entity.to_string(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/states/TodoStatus.sophia",
            state.to_string(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/errors/TodoError.sophia",
            error.to_string(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/capabilities/TodoCapability.sophia",
            capability.to_string(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/actions/CompleteTodo.sophia",
            action.to_string(),
        ),
    ]
}

/// 把源码字符串解析为 AST 集合，再构造 ProgramInput。
fn asts(sources: &[(&str, &str, String)]) -> Vec<(String, String, Ast)> {
    sources
        .iter()
        .map(|(domain, path, src)| {
            (
                domain.to_string(),
                path.to_string(),
                parse_ast(src.as_str()).expect("parse"),
            )
        })
        .collect()
}

fn inputs<'a>(parsed: &'a [(String, String, Ast)]) -> Vec<ProgramInput<'a>> {
    parsed
        .iter()
        .map(|(d, p, a)| ProgramInput {
            domain: d,
            path: p,
            ast: a,
        })
        .collect()
}

#[test]
fn well_formed_program_resolves_clean() {
    let sources = todo_program_sources();
    let parsed = asts(&sources);
    let (index, diags) = resolve_program(&inputs(&parsed)).expect("build");

    assert_eq!(index.kind_of("Todo"), Some(NodeKind::Entity));
    assert_eq!(index.kind_of("CompleteTodo"), Some(NodeKind::Action));
    assert_eq!(index.kind_of("TodoStatus"), Some(NodeKind::State));
    assert!(
        diags.is_empty(),
        "规范程序不应有 HIR 诊断，但得到：{diags:?}"
    );
}

#[test]
fn duplicate_node_name_is_hard_error() {
    let a = parse_ast("entity Todo { fields { id { type: Int } } }").unwrap();
    let b = parse_ast("entity Todo { fields { name { type: Text } } }").unwrap();
    let parsed = vec![
        (
            "D1".to_string(),
            "domains/D1/entities/Todo.sophia".to_string(),
            a,
        ),
        (
            "D2".to_string(),
            "domains/D2/entities/Todo.sophia".to_string(),
            b,
        ),
    ];
    let err = resolve_program(&inputs(&parsed)).unwrap_err();
    assert!(matches!(err, HirError::DuplicateNode { .. }));
}

#[test]
fn multiple_top_level_nodes_in_one_file_is_error() {
    let two = parse_ast(
        "entity A { fields { x { type: Int } } } entity B { fields { y { type: Int } } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/entities/AB.sophia".to_string(),
        two,
    )];
    let err = resolve_program(&inputs(&parsed)).unwrap_err();
    assert!(matches!(
        err,
        HirError::MultipleTopLevelNodes { count: 2, .. }
    ));
}

#[test]
fn unresolved_type_reference_reported() {
    // Todo 引用了不存在的 state Ghost。
    let entity = parse_ast("entity Todo { fields { s { type: Ghost } } }").unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/entities/Todo.sophia".to_string(),
        entity,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::UnresolvedReference && d.name == "Ghost"));
}

#[test]
fn unknown_capability_binding_reported() {
    let action = parse_ast(
        "action A { capability: NoSuchCap input { x: Int } output { y: Int } body { return x } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        action,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::UnresolvedReference && d.name == "NoSuchCap"));
}

#[test]
fn shadowing_local_variable_is_forbidden() {
    let action = parse_ast(
        "action A { input { x: Int } output { y: Int } body { let v = 1 let v = 2 return y } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        action,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::Shadowing && d.name == "v"));
}

#[test]
fn shadowing_input_param_is_forbidden() {
    let action =
        parse_ast("action A { input { x: Int } output { y: Int } body { let x = 1 return y } }")
            .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        action,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::Shadowing && d.name == "x"));
}

#[test]
fn set_immutable_variable_reported() {
    let action = parse_ast(
        "action A { input { x: Int } output { y: Int } body { let v = 1 set v = 2 return y } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        action,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::AssignToImmutable && d.name == "v"));
}

#[test]
fn set_mutable_variable_is_ok() {
    let action = parse_ast(
        "action A { input { x: Int } output { y: Int } body { let mutable v = 1 set v = 2 return y } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        action,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(
        !diags
            .iter()
            .any(|d| d.kind == HirDiagnosticKind::AssignToImmutable),
        "对 mutable 变量 set 不应报错：{diags:?}"
    );
}

#[test]
fn set_undeclared_variable_reported() {
    let action = parse_ast(
        "action A { input { x: Int } output { y: Int } body { set ghost = 1 return y } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        action,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::UnresolvedVariable && d.name == "ghost"));
}

#[test]
fn match_binding_scoped_to_arm() {
    // found 在 arm body 内可见；离开 arm 后引用应未解析。
    let ok = parse_ast(
        "action A { input { x: one of { Int, Null } } output { y: Int } body { match x { Int found => return found Null => return 0 } } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        ok,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(
        !diags
            .iter()
            .any(|d| d.kind == HirDiagnosticKind::UnresolvedVariable),
        "arm 内引用绑定不应报错：{diags:?}"
    );
}

#[test]
fn unknown_type_in_match_pattern_reported() {
    // match 类型 pattern 的类型名未知（`Bogus v`）→ UnresolvedReference。
    let ok = parse_ast(
        "action A { input { x: one of { Int, Null } } output { y: Int } body { match x { Bogus v => return 0 Null => return 1 } } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/A.sophia".to_string(),
        ok,
    )];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(
        diags
            .iter()
            .any(|d| d.kind == HirDiagnosticKind::UnresolvedReference && d.name == "Bogus"),
        "match 类型 pattern 未知类型名应报 UnresolvedReference：{diags:?}"
    );
}

#[test]
fn cross_domain_implicit_reference_reported() {
    // D2 的 action 引用 D1 的 entity Foo，未通过 task include。
    let foo = parse_ast("entity Foo { fields { a { type: Int } } }").unwrap();
    let act = parse_ast("action UseFoo { input { f: Foo } output { y: Int } body { return y } }")
        .unwrap();
    let parsed = vec![
        (
            "D1".to_string(),
            "domains/D1/entities/Foo.sophia".to_string(),
            foo,
        ),
        (
            "D2".to_string(),
            "domains/D2/actions/UseFoo.sophia".to_string(),
            act,
        ),
    ];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::ImplicitCrossDomain && d.name == "Foo"));
}

#[test]
fn raise_unknown_variant_reported() {
    let action = parse_ast(
        "action A { input { x: Int } output { y: Int } errors { Known } body { raise Ghost { id = x } } }",
    )
    .unwrap();
    let err = parse_ast("error E { variant Known { id: Int } }").unwrap();
    let parsed = vec![
        (
            "D".to_string(),
            "domains/D/actions/A.sophia".to_string(),
            action,
        ),
        (
            "D".to_string(),
            "domains/D/errors/E.sophia".to_string(),
            err,
        ),
    ];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::UnresolvedReference && d.name == "Ghost"));
}

#[test]
fn errors_referencing_error_node_instead_of_variant_reported() {
    // errors 列表引用了 error 节点名 E，而非其 variant。
    let action =
        parse_ast("action A { input { x: Int } output { y: Int } errors { E } body { return y } }")
            .unwrap();
    let err = parse_ast("error E { variant Known { id: Int } }").unwrap();
    let parsed = vec![
        (
            "D".to_string(),
            "domains/D/actions/A.sophia".to_string(),
            action,
        ),
        (
            "D".to_string(),
            "domains/D/errors/E.sophia".to_string(),
            err,
        ),
    ];
    let (_idx, diags) = resolve_program(&inputs(&parsed)).unwrap();
    assert!(diags
        .iter()
        .any(|d| d.kind == HirDiagnosticKind::UnresolvedReference && d.name == "E"));
}

#[test]
fn asg_index_json_is_stable_and_sorted() {
    let sources = todo_program_sources();
    let parsed = asts(&sources);
    let (index, _diags) = resolve_program(&inputs(&parsed)).expect("build");
    let json = index.to_json().expect("json");
    // BTreeMap 保证 key 升序；CompleteTodo 应排在 Todo 前。
    let pos_complete = json.find("CompleteTodo").unwrap();
    let pos_todo = json.find("\"Todo\"").unwrap();
    assert!(pos_complete < pos_todo, "JSON key 应按字典序稳定排序");
}

/// ASG index 的确定性快照（守护 HIR 名称解析产物 / 17.2 规范不被静默改动）。
///
/// 用规范 TodoDomain 程序，把 `index.to_json()`（BTreeMap 稳定排序）`insta` 快照。
#[test]
fn asg_index_json_snapshot() {
    let sources = todo_program_sources();
    let parsed = asts(&sources);
    let (index, _diags) = resolve_program(&inputs(&parsed)).expect("build");
    let json = index.to_json().expect("json");
    insta::assert_snapshot!("asg_index_todo_domain", json);
}

#[test]
fn http_special_root_resolves_clean() {
    // `Http.Get(url)` 的特殊根 `Http` 应被名称解析放行（不报未声明变量），且 `effects { Http.Get }`
    // 声明（0 参）不应被误判 UnresolvedEffect——effect 身份不带 URL arg（见 docs/http_lib.md §2.6）。
    let ok = parse_ast(
        "action Fetch { input { url: Text } output { body: Raw<Text> } effects { Http.Get } body { return Http.Get(url) } }",
    )
    .unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/actions/Fetch.sophia".to_string(),
        ok,
    )];
    let (_idx, diags) = resolve_program_with_libraries(&inputs(&parsed), &http_registry()).unwrap();
    assert!(
        !diags
            .iter()
            .any(|d| d.kind == HirDiagnosticKind::UnresolvedVariable
                || d.kind == HirDiagnosticKind::UnresolvedReference),
        "Http 特殊根 + Http.Get effect 应解析干净：{diags:?}"
    );
    // 回归（2026-05-31 D2 暴露的潜伏缺陷）：`effects { Http.Get }` 是 0 参声明，arity 表若误设
    // 为 1 会报 UnresolvedEffect。钉死 arity=0。
    assert!(
        !diags
            .iter()
            .any(|d| d.kind == HirDiagnosticKind::UnresolvedEffect),
        "`effects {{ Http.Get }}`（0 参）不应报 UnresolvedEffect：{diags:?}"
    );
}

#[test]
fn http_get_capability_allow_resolves_clean() {
    // `capability { allow { Http.Get } }`（0 参 effect 引用）应解析干净——同上 arity=0 回归。
    let cap = parse_ast("capability NetCap { allow { Http.Get } }").unwrap();
    let parsed = vec![(
        "D".to_string(),
        "domains/D/capabilities/NetCap.sophia".to_string(),
        cap,
    )];
    let (_idx, diags) = resolve_program_with_libraries(&inputs(&parsed), &http_registry()).unwrap();
    assert!(
        !diags
            .iter()
            .any(|d| d.kind == HirDiagnosticKind::UnresolvedEffect),
        "`allow {{ Http.Get }}`（0 参）不应报 UnresolvedEffect：{diags:?}"
    );
}
