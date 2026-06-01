//! Task Closure / action-rooted semantic context 的集成测试（语言设计第八节）。

use sophia_hir::{
    action_context, resolve_program, task_context, ClosureError, ContextEdgeKind, NodeKind,
    ProgramInput,
};
use sophia_syntax::{parse_ast, Ast};

/// 规范 TodoDomain 多文件程序（每节点一文件）。
fn sources() -> Vec<(&'static str, &'static str, String)> {
    let domain = r#"domain TodoDomain { meaning: "todo" }"#;
    let entity = r#"entity Todo {
  fields {
    id { type: Uuid }
    title { type: Sanitized<Text> }
    status { type: TodoStatus }
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
      TodoStatus.Pending => return todo
    }
  }
}"#;
    let task = r#"task ImplementCompleteTodo {
  goal: "implement"
  include {
    entity Todo; state TodoStatus; error TodoError
    capability TodoCapability; action CompleteTodo
  }
  exclude { Http.Get }
}"#;
    vec![
        (
            "TodoDomain",
            "domains/TodoDomain/domain.sophia",
            domain.into(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/entities/Todo.sophia",
            entity.into(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/states/TodoStatus.sophia",
            state.into(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/errors/TodoError.sophia",
            error.into(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/capabilities/TodoCapability.sophia",
            capability.into(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/actions/CompleteTodo.sophia",
            action.into(),
        ),
        (
            "TodoDomain",
            "domains/TodoDomain/tasks/ImplementCompleteTodo.sophia",
            task.into(),
        ),
    ]
}

fn build() -> (Vec<Ast>, sophia_hir::AsgIndex) {
    let srcs = sources();
    let asts: Vec<Ast> = srcs
        .iter()
        .map(|(_, _, s)| parse_ast(s.clone()).unwrap())
        .collect();
    let inputs: Vec<ProgramInput> = srcs
        .iter()
        .zip(&asts)
        .map(|((domain, path, _), ast)| ProgramInput { domain, path, ast })
        .collect();
    let (index, diags) = resolve_program(&inputs).unwrap();
    assert!(diags.is_empty(), "规范程序应无诊断：{diags:?}");
    (asts, index)
}

#[test]
fn action_closure_includes_full_neighborhood() {
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let closure = action_context("CompleteTodo", &refs, &index).unwrap();

    let names: Vec<&str> = closure.nodes.iter().map(|n| n.name.as_str()).collect();
    // capability / input-output 类型 / error / domain 都应进入闭包。
    for expected in [
        "CompleteTodo",
        "TodoCapability",
        "Todo",
        "TodoStatus",
        "TodoError",
        "TodoDomain",
    ] {
        assert!(names.contains(&expected), "闭包应含 {expected}：{names:?}");
    }
}

#[test]
fn action_closure_emits_explaining_edges() {
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let closure = action_context("CompleteTodo", &refs, &index).unwrap();

    let has = |from: &str, kind: ContextEdgeKind, to: &str| {
        closure
            .edges
            .iter()
            .any(|e| e.from == from && e.kind == kind && e.to == to)
    };
    assert!(has(
        "CompleteTodo",
        ContextEdgeKind::BindsCapability,
        "TodoCapability"
    ));
    assert!(has("CompleteTodo", ContextEdgeKind::UsesType, "Todo"));
    assert!(has("CompleteTodo", ContextEdgeKind::Raises, "TodoError"));
    // Todo 经字段 status 引用 TodoStatus。
    assert!(has("Todo", ContextEdgeKind::UsesType, "TodoStatus"));
}

#[test]
fn action_closure_is_deterministic() {
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let a = action_context("CompleteTodo", &refs, &index).unwrap();
    let b = action_context("CompleteTodo", &refs, &index).unwrap();
    assert_eq!(a, b, "闭包计算应确定性");
    // files 排序去重。
    let mut sorted = a.files.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(a.files, sorted);
}

#[test]
fn action_root_must_be_action() {
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let err = action_context("Todo", &refs, &index).unwrap_err();
    assert!(matches!(
        err,
        ClosureError::WrongRootKind {
            expected: NodeKind::Action,
            actual: NodeKind::Entity,
            ..
        }
    ));
}

#[test]
fn missing_root_reported() {
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let err = action_context("NoSuch", &refs, &index).unwrap_err();
    assert!(matches!(err, ClosureError::RootNotFound(_)));
}

#[test]
fn task_closure_includes_dependencies() {
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let closure = task_context("ImplementCompleteTodo", &refs, &index).unwrap();

    let names: Vec<&str> = closure.nodes.iter().map(|n| n.name.as_str()).collect();
    for expected in [
        "Todo",
        "TodoStatus",
        "TodoError",
        "TodoCapability",
        "CompleteTodo",
    ] {
        assert!(
            names.contains(&expected),
            "task 闭包应含 {expected}：{names:?}"
        );
    }
    // include 边从 task 指向各入口。
    assert!(closure
        .edges
        .iter()
        .any(|e| e.from == "ImplementCompleteTodo"
            && e.kind == ContextEdgeKind::Includes
            && e.to == "CompleteTodo"));
}

#[test]
fn task_closure_excluded_effect_no_longer_blocks() {
    // exclude 列 effect 引用（如 `Http.Get`）；移除 storage 节点后，exclude 不再命中任何 formal
    // 依赖节点，task 闭包正常归结（不报 ExcludedDependency）。这是 storage 移除后的行为变化。
    let (asts, index) = build();
    let refs: Vec<&Ast> = asts.iter().collect();
    let closure = task_context("ImplementCompleteTodo", &refs, &index);
    assert!(
        closure.is_ok(),
        "exclude 不命中节点时应正常归结：{closure:?}"
    );
}

// ---- effect 声明与引用解析（语言设计 §13） ----

mod effect_resolution {
    use sophia_hir::{
        resolve_program, resolve_program_with_libraries, HirDiagnosticKind, LibraryContent,
        LibraryRegistry, ProgramInput,
    };
    use sophia_syntax::{parse_ast, Ast};

    /// 内联中性库注册表：声明 `File.Read/Write` + `Http.Get`（同标准库形状，但 hir 测试不依赖
    /// sophia-stdlib——core 层不反向依赖内容层，故就近用清单构建）。
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

    fn resolve_single(domain: &str, path: &str, src: &str) -> Vec<sophia_hir::HirDiagnostic> {
        let ast: Ast = parse_ast(src.to_string()).unwrap();
        let inputs = vec![ProgramInput {
            domain,
            path,
            ast: &ast,
        }];
        // 注入库注册表（File / Http），与生产路径（CLI 经 sophia-stdlib 注入）同源。
        resolve_program_with_libraries(&inputs, &lib_registry())
            .unwrap()
            .1
    }

    /// 无库版本（仅语言内置 Console）。
    fn resolve_single_no_lib(
        domain: &str,
        path: &str,
        src: &str,
    ) -> Vec<sophia_hir::HirDiagnostic> {
        let ast: Ast = parse_ast(src.to_string()).unwrap();
        let inputs = vec![ProgramInput {
            domain,
            path,
            ast: &ast,
        }];
        resolve_program(&inputs).unwrap().1
    }

    #[test]
    fn builtin_effects_resolve() {
        // Console.Write（语言内置）/ File.Read / Http.Get（库 effect 族，经注册表注入）：
        // capability 引用应无诊断。
        let diags = resolve_single(
            "D",
            "domains/D/capabilities/Cap.sophia",
            r#"capability Cap { allow { File.Read; File.Write; Http.Get; Console.Write } deny { } }"#,
        );
        assert!(diags.is_empty(), "内置 / 库 effect 不应有诊断：{diags:?}");
    }

    #[test]
    fn console_resolves_without_libraries() {
        // 语言内置 Console.Write 无需任何库注册表即可解析（机制 vs 能力族边界）。
        let diags = resolve_single_no_lib(
            "D",
            "domains/D/capabilities/Cap.sophia",
            r#"capability Cap { allow { Console.Write } deny { } }"#,
        );
        assert!(diags.is_empty(), "内置 Console 不应有诊断：{diags:?}");
    }

    #[test]
    fn unknown_effect_op_reported() {
        let diags = resolve_single(
            "D",
            "domains/D/capabilities/Cap.sophia",
            r#"capability Cap { allow { Magic.Cast("x") } deny { } }"#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.kind == HirDiagnosticKind::UnresolvedEffect),
            "未声明 effect 应报 UnresolvedEffect：{diags:?}"
        );
    }

    #[test]
    fn effect_arity_mismatch_reported() {
        // Console.Write 期望 0 个参数，给 1 个 → arity 不符。
        let diags = resolve_single(
            "D",
            "domains/D/capabilities/Cap.sophia",
            r#"capability Cap { allow { Console.Write("x") } deny { } }"#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.kind == HirDiagnosticKind::UnresolvedEffect),
            "arity 不符应报 UnresolvedEffect：{diags:?}"
        );
    }

    #[test]
    fn declared_effect_family_resolves_across_files() {
        // effect 声明在一个文件，capability 引用其操作在另一个文件，应解析通过。
        let eff =
            parse_ast("effect Payment { operation Charge { param amount: Int } }".to_string())
                .unwrap();
        let cap = parse_ast(
            r#"capability PayCap { allow { Payment.Charge(100) } deny { } }"#.to_string(),
        )
        .unwrap();
        let inputs = vec![
            ProgramInput {
                domain: "D",
                path: "domains/D/effects/Payment.sophia",
                ast: &eff,
            },
            ProgramInput {
                domain: "D",
                path: "domains/D/capabilities/PayCap.sophia",
                ast: &cap,
            },
        ];
        let (index, diags) = resolve_program(&inputs).unwrap();
        assert!(diags.is_empty(), "声明的 effect 引用应解析通过：{diags:?}");
        // effect 进入 index 作为 Effect kind。
        assert_eq!(index.kind_of("Payment"), Some(sophia_hir::NodeKind::Effect));
        // effect 操作进入符号表。
        assert!(index.effect_op("Payment", "Charge").is_some());
    }
}
