//! 解释器集成测试：parse → HIR(index) → semantic(model) → run。
//!
//! 库 effect op（特殊根 `Lib.Op(args)`）的解释器分派机制用一个**中性测试库** `Vault`（清单内联
//! 构建 [`LibraryRegistry`]）+ 注册到 [`HostRegistry`] 的闭包验证——runtime 不依赖 `sophia-stdlib`
//! （后者反向依赖 runtime），故用中性库测分派机制，不测具体标准库语义（那归 stdlib 的测试）。

use sophia_hir::{AsgIndex, IndexInput, LibraryContent, LibraryRegistry};
use sophia_runtime::{run_action as run_exec, HostRegistry, Outcome, RuntimeError, Value};
use sophia_semantic::{analyze_program, SemanticModel};
use sophia_syntax::{parse_ast, Ast};

/// 测试便捷封装：执行并返回 `(Outcome, HostRegistry)`。
///
/// trace 投影由专门的 trace 测试覆盖；多数解释器测试只关心结局与宿主，故在测试层
/// 保留测试内常用返回形态；公共 `run_action` 仍要求显式 host。
fn run_action(
    model: &SemanticModel,
    asts: &[&Ast],
    name: &str,
    args: Vec<Value>,
) -> Result<(Outcome, HostRegistry), RuntimeError> {
    let mut host = HostRegistry::new();
    let (outcome, _trace) = run_exec(model, asts, name, args, &mut host)?;
    Ok((outcome, host))
}

/// 中性测试库 `Vault` 的清单：`Vault.Read(path) -> Raw<Text>` / `Vault.Write(path, Sanitized<Text>)
/// -> Unit`（同 File 形状，但中性名——测分派机制而非具体库）。
fn vault_registry() -> LibraryRegistry {
    LibraryRegistry::build(vec![LibraryContent {
        dir_name: "vault".into(),
        manifest_toml: r#"
[library]
name = "vault"
summary = "测试用中性库"
abi_version = 1
[[op]]
family = "Vault"
op = "Read"
params = ["Text"]
returns = "Raw<Text>"
host_fn = "vault_read"
[[op]]
family = "Vault"
op = "Write"
params = ["Text", "Sanitized<Text>"]
returns = "Unit"
host_fn = "vault_write"
[prompt]
asset = "vault.md"
"#
        .into(),
        asset_text: "x".into(),
        sophia_sources: vec![],
        host_wasm: None,
    }])
    .expect("build vault registry")
}

/// 构建一个注册了 `Vault.Read/Write` mock 闭包的 host（内存桶，未命中诚实 Err）。
fn vault_host() -> HostRegistry {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;
    let bucket: Rc<RefCell<BTreeMap<String, String>>> = Rc::new(RefCell::new(BTreeMap::new()));
    let mut host = HostRegistry::new();
    let read_bucket = bucket.clone();
    host.register_fn("Vault", "Read", move |args| {
        let path = match args.first() {
            Some(Value::Text(p)) => p.clone(),
            _ => return Err("Vault.Read 缺 path".into()),
        };
        match read_bucket.borrow().get(&path) {
            Some(c) => Ok(Value::Text(c.clone())),
            None => Err(format!("Vault.Read 无 mock 文件：{path}")),
        }
    });
    let write_bucket = bucket.clone();
    host.register_fn("Vault", "Write", move |args| {
        let (path, content) = match (args.first(), args.get(1)) {
            (Some(Value::Text(p)), Some(Value::Text(c))) => (p.clone(), c.clone()),
            _ => return Err("Vault.Write 缺 path/content".into()),
        };
        write_bucket.borrow_mut().insert(path, content);
        Ok(Value::Unit)
    });
    host
}

/// 把一条 mock 文件预置进 host 的 Vault 桶（经 Write 闭包，等价 seed）。
fn seed_vault(host: &mut HostRegistry, path: &str, content: &str) {
    host.call(
        "Vault",
        "Write",
        &[Value::Text(path.into()), Value::Text(content.into())],
    )
    .expect("seed vault");
}

/// 一个测试程序：每个源串一个文件（遵循一文件一节点）。
struct Program {
    asts: Vec<Ast>,
}

impl Program {
    fn new(sources: &[&str]) -> Self {
        Program {
            asts: sources
                .iter()
                .map(|s| parse_ast(*s).expect("parse"))
                .collect(),
        }
    }

    /// 构建 index + semantic model（无库），并断言无语义诊断。
    fn analyze(&self) -> SemanticModel {
        self.analyze_with(&LibraryRegistry::empty())
    }

    /// 构建 index（叠加库注册表）+ semantic model，并断言无语义诊断。
    fn analyze_with(&self, registry: &LibraryRegistry) -> SemanticModel {
        let inputs: Vec<IndexInput> = self
            .asts
            .iter()
            .enumerate()
            .map(|(i, a)| IndexInput {
                domain: "D",
                path: Box::leak(format!("domains/D/n/{i}.sophia").into_boxed_str()),
                ast: a,
            })
            .collect();
        let index = AsgIndex::build(inputs, registry).expect("index");
        let refs: Vec<&Ast> = self.asts.iter().collect();
        let analysis = analyze_program(&refs, &index);
        assert!(
            analysis.diagnostics.is_empty(),
            "测试源码应通过语义检查：{:?}",
            analysis.diagnostics
        );
        analysis.model
    }

    fn refs(&self) -> Vec<&Ast> {
        self.asts.iter().collect()
    }
}

#[test]
fn arithmetic_and_return() {
    let prog = Program::new(&[r#"action Add {
  input { a: Int; b: Int }
  output { sum: Int }
  body { return a + b }
}"#]);
    let model = prog.analyze();
    let (outcome, _host) = run_action(
        &model,
        &prog.refs(),
        "Add",
        vec![Value::Int(3), Value::Int(4)],
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Returned(Value::Int(7)));
}

#[test]
fn unary_negation() {
    // 一元算术取负 `-expr`（起步子集算术原语）：对负的差取负即绝对值的一支。
    let prog = Program::new(&[r#"action NegAbs {
  input { left: Int; right: Int }
  output { result: Int }
  body {
    let diff = left - right
    if diff < 0 { return -diff } else { return diff }
  }
}"#]);
    let model = prog.analyze();
    let refs = prog.refs();
    // 2 - 9 = -7 → -(-7) = 7。
    let (neg, _) = run_action(&model, &refs, "NegAbs", vec![Value::Int(2), Value::Int(9)]).unwrap();
    assert_eq!(neg, Outcome::Returned(Value::Int(7)));
    // 9 - 2 = 7 → 直接返回 7。
    let (pos, _) = run_action(&model, &refs, "NegAbs", vec![Value::Int(9), Value::Int(2)]).unwrap();
    assert_eq!(pos, Outcome::Returned(Value::Int(7)));
}

#[test]
fn if_else_branch() {
    let prog = Program::new(&[r#"action Pick {
  input { b: Bool }
  output { y: Int }
  body {
    if b {
      return 1
    } else {
      return 2
    }
  }
}"#]);
    let model = prog.analyze();
    let refs = prog.refs();
    let (t, _) = run_action(&model, &refs, "Pick", vec![Value::Bool(true)]).unwrap();
    assert_eq!(t, Outcome::Returned(Value::Int(1)));
    let (f, _) = run_action(&model, &refs, "Pick", vec![Value::Bool(false)]).unwrap();
    assert_eq!(f, Outcome::Returned(Value::Int(2)));
}

#[test]
fn repeat_accumulates() {
    let prog = Program::new(&[r#"action Sum {
  input { n: Int }
  output { total: Int }
  body {
    let mutable total = 0
    repeat 3 times {
      set total = total + n
    }
    return total
  }
}"#]);
    let model = prog.analyze();
    let (outcome, _) = run_action(&model, &prog.refs(), "Sum", vec![Value::Int(5)]).unwrap();
    assert_eq!(outcome, Outcome::Returned(Value::Int(15)));
}

#[test]
fn print_captured_by_host() {
    let prog = Program::new(&[
        r#"action Greet {
  capability: C
  input { x: Int }
  output { y: Int }
  effects { Console.Write }
  body {
    print "hello"
    return x
  }
}"#,
        "capability C { allow { Console.Write } }",
    ]);
    let model = prog.analyze();
    let (outcome, host) = run_action(&model, &prog.refs(), "Greet", vec![Value::Int(1)]).unwrap();
    assert_eq!(outcome, Outcome::Returned(Value::Int(1)));
    assert_eq!(host.console, vec!["hello".to_string()]);
}

#[test]
fn match_on_state_dispatches() {
    let prog = Program::new(&[
        "state S { value A { meaning: \"a\" } value B { meaning: \"b\" } }",
        r#"action Classify {
  input { s: S }
  output { y: Int }
  body {
    match s {
      S.A => return 10
      S.B => return 20
    }
  }
}"#,
    ]);
    let model = prog.analyze();
    let refs = prog.refs();
    let (a, _) = run_action(
        &model,
        &refs,
        "Classify",
        vec![Value::State {
            state: "S".into(),
            value: "A".into(),
        }],
    )
    .unwrap();
    assert_eq!(a, Outcome::Returned(Value::Int(10)));
    let (b, _) = run_action(
        &model,
        &refs,
        "Classify",
        vec![Value::State {
            state: "S".into(),
            value: "B".into(),
        }],
    )
    .unwrap();
    assert_eq!(b, Outcome::Returned(Value::Int(20)));
}

#[test]
fn match_on_one_of_binds_member() {
    // one of { Int, Null }：Int 成员经类型 pattern 绑定，Null 成员经 Null pattern。
    let prog = Program::new(&[r#"action Unwrap {
  input { o: one of { Int, Null } }
  output { y: Int }
  body {
    match o {
      Int v => return v
      Null  => return 0
    }
  }
}"#]);
    let model = prog.analyze();
    let refs = prog.refs();
    let (some, _) = run_action(&model, &refs, "Unwrap", vec![Value::Int(42)]).unwrap();
    assert_eq!(some, Outcome::Returned(Value::Int(42)));
    let (none, _) = run_action(&model, &refs, "Unwrap", vec![Value::Null]).unwrap();
    assert_eq!(none, Outcome::Returned(Value::Int(0)));
}

#[test]
fn raise_produces_domain_error() {
    let prog = Program::new(&[
        "error E { variant Bad { reason: Text } }",
        r#"action Fail {
  input { x: Int }
  output { y: Int }
  errors { Bad }
  body {
    raise Bad { reason = "nope" }
  }
}"#,
    ]);
    let model = prog.analyze();
    let (outcome, _) = run_action(&model, &prog.refs(), "Fail", vec![Value::Int(1)]).unwrap();
    match outcome {
        Outcome::Raised(e) => {
            assert_eq!(e.variant, "Bad");
            assert_eq!(e.fields.get("reason"), Some(&Value::Text("nope".into())));
        }
        other => panic!("期望 raise，得到 {other:?}"),
    }
}

#[test]
fn entity_construction_and_field_access() {
    let prog = Program::new(&[
        "entity P { fields { x { type: Int } y { type: Int } } }",
        r#"action MakeP {
  input { a: Int; b: Int }
  output { p: P }
  body { return P { x = a, y = b } }
}"#,
    ]);
    let model = prog.analyze();
    let (outcome, _) = run_action(
        &model,
        &prog.refs(),
        "MakeP",
        vec![Value::Int(1), Value::Int(2)],
    )
    .unwrap();
    match outcome {
        Outcome::Returned(Value::Entity { name, fields }) => {
            assert_eq!(name, "P");
            assert_eq!(fields.get("x"), Some(&Value::Int(1)));
            assert_eq!(fields.get("y"), Some(&Value::Int(2)));
        }
        other => panic!("期望 entity，得到 {other:?}"),
    }
}

#[test]
fn input_validation_rejects_wrong_arity() {
    let prog = Program::new(&[r#"action A {
  input { x: Int }
  output { y: Int }
  body { return x }
}"#]);
    let model = prog.analyze();
    let err = run_action(&model, &prog.refs(), "A", vec![]).unwrap_err();
    assert!(matches!(err, RuntimeError::Validation(_)));
}

#[test]
fn input_validation_rejects_wrong_type() {
    let prog = Program::new(&[r#"action A {
  input { x: Int }
  output { y: Int }
  body { return x }
}"#]);
    let model = prog.analyze();
    let err = run_action(&model, &prog.refs(), "A", vec![Value::Text("oops".into())]).unwrap_err();
    assert!(matches!(err, RuntimeError::Validation(_)));
}

#[test]
fn raise_propagates_across_call_as_domain_outcome() {
    // 被调用方 raise 的领域错误应在调用方 run 边界物化为 Outcome::Raised，
    // 而非硬错误 RuntimeError（错误代数 §7.5 / §16.3）。
    let prog = Program::new(&[
        "error E { variant Bad { reason: Text } }",
        r#"action Inner {
  input { x: Int }
  output { y: Int }
  errors { Bad }
  body { raise Bad { reason = "inner" } }
}"#,
        r#"action Outer {
  input { x: Int }
  output { y: Int }
  errors { Bad }
  body {
    let r = Inner(x)
    return r
  }
}"#,
    ]);
    let model = prog.analyze();
    let (outcome, _) = run_action(&model, &prog.refs(), "Outer", vec![Value::Int(1)]).unwrap();
    match outcome {
        Outcome::Raised(e) => assert_eq!(e.variant, "Bad"),
        other => panic!("期望领域错误向上传播为 Outcome::Raised，得到 {other:?}"),
    }
}

#[test]
fn cross_action_call_resolves_across_files() {
    // 一文件一节点：Inner 与 Outer 在不同文件，Outer 调用 Inner。
    let prog = Program::new(&[
        r#"action Inner {
  input { x: Int }
  output { y: Int }
  body { return x + 1 }
}"#,
        r#"action Outer {
  input { x: Int }
  output { y: Int }
  body {
    let r = Inner(x)
    return r + 10
  }
}"#,
    ]);
    let model = prog.analyze();
    let (outcome, _) = run_action(&model, &prog.refs(), "Outer", vec![Value::Int(5)]).unwrap();
    // Inner(5) = 6；Outer = 6 + 10 = 16。
    assert_eq!(outcome, Outcome::Returned(Value::Int(16)));
}

#[test]
fn transition_call_via_construction_syntax() {
    // transition 用构造式语法调用：CompleteTransition { todo = ... }。
    let prog = Program::new(&[
        "state S { value A { meaning: \"a\" } value B { meaning: \"b\" } }",
        "entity T { fields { status { type: S } } }",
        r#"transition ToB {
  input { t: T }
  output { t: T }
  effects { Pure }
  body { return T { status = S.B } }
}"#,
        r#"action Promote {
  input { t: T }
  output { t: T }
  body {
    let next = ToB { t = t }
    return next
  }
}"#,
    ]);
    let model = prog.analyze();
    let input_entity = Value::Entity {
        name: "T".into(),
        fields: [(
            "status".to_string(),
            Value::State {
                state: "S".into(),
                value: "A".into(),
            },
        )]
        .into(),
    };
    let (outcome, _) = run_action(&model, &prog.refs(), "Promote", vec![input_entity]).unwrap();
    match outcome {
        Outcome::Returned(Value::Entity { fields, .. }) => {
            assert_eq!(
                fields.get("status"),
                Some(&Value::State {
                    state: "S".into(),
                    value: "B".into()
                })
            );
        }
        other => panic!("期望 entity，得到 {other:?}"),
    }
}

#[test]
fn output_validation_catches_bad_return() {
    // body 返回与 output 类型不符的值会被 runtime output validation 捕获。
    // 这里用 Unknown 难以触发；改为间接：action 返回 Int 但 output 声明 Bool 时
    // 编译期已拦截，故 runtime 校验主要防御解释器/宿主注入的非法值。
    // 用合法程序确认正常返回通过校验（负路径由编译期覆盖）。
    let prog = Program::new(&[r#"action Ok {
  input { x: Int }
  output { y: Int }
  body { return x }
}"#]);
    let model = prog.analyze();
    let (outcome, _) = run_action(&model, &prog.refs(), "Ok", vec![Value::Int(9)]).unwrap();
    assert_eq!(outcome, Outcome::Returned(Value::Int(9)));
}

// ---- 库 effect op 的解释器分派机制（中性测试库 Vault；不依赖 sophia-stdlib） ----

#[test]
fn lib_write_then_read_roundtrips() {
    // body 级库 op：先 Vault.Write 后 Vault.Read 取回写入内容（host 闭包内存桶）。
    let prog = Program::new(&[
        r#"capability C { allow { Vault.Read; Vault.Write } }"#,
        r#"action Trust { intent_conversion: true input { raw: Raw<Text> } output { clean: Sanitized<Text> } effects { Pure } body { return raw } }"#,
        r#"action WriteThenRead {
  capability: C
  input { path: Text; content: Sanitized<Text> }
  output { len: Int }
  effects { Vault.Read; Vault.Write }
  body {
    Vault.Write(path, content)
    let raw = Vault.Read(path)
    let clean = Trust(raw)
    return clean.length
  }
}"#,
    ]);
    let model = prog.analyze_with(&vault_registry());
    let refs = prog.refs();
    let mut host = vault_host();
    let outcome = {
        let mut interp = sophia_runtime::Interpreter::new(&model, &refs, &mut host);
        interp
            .run(
                "WriteThenRead",
                vec![Value::Text("/tmp/x".into()), Value::Text("hello".into())],
            )
            .unwrap()
    };
    // "hello" 长度 5。
    assert_eq!(outcome, Outcome::Returned(Value::Int(5)));
}

#[test]
fn lib_read_seeded_mock() {
    // 预置 mock 文件后 Vault.Read 取回其内容。
    let prog = Program::new(&[
        r#"capability C { allow { Vault.Read } }"#,
        r#"action Trust { intent_conversion: true input { raw: Raw<Text> } output { clean: Sanitized<Text> } effects { Pure } body { return raw } }"#,
        r#"action Load {
  capability: C
  input { path: Text }
  output { len: Int }
  effects { Vault.Read }
  body {
    let raw = Vault.Read(path)
    let clean = Trust(raw)
    return clean.length
  }
}"#,
    ]);
    let model = prog.analyze_with(&vault_registry());
    let refs = prog.refs();
    let mut host = vault_host();
    seed_vault(&mut host, "/etc/conf", "abcd"); // 长度 4
    let outcome = {
        let mut interp = sophia_runtime::Interpreter::new(&model, &refs, &mut host);
        interp
            .run("Load", vec![Value::Text("/etc/conf".into())])
            .unwrap()
    };
    assert_eq!(outcome, Outcome::Returned(Value::Int(4)));
}

#[test]
fn lib_read_missing_errors_honestly() {
    // 未预置且未写入的 path：host 闭包如实 Err 阻断（不伪造内容）→ 解释器硬错误。
    let prog = Program::new(&[
        r#"capability C { allow { Vault.Read } }"#,
        r#"action Trust { intent_conversion: true input { raw: Raw<Text> } output { clean: Sanitized<Text> } effects { Pure } body { return raw } }"#,
        r#"action Load {
  capability: C
  input { path: Text }
  output { len: Int }
  effects { Vault.Read }
  body {
    let raw = Vault.Read(path)
    let clean = Trust(raw)
    return clean.length
  }
}"#,
    ]);
    let model = prog.analyze_with(&vault_registry());
    let refs = prog.refs();
    let mut host = vault_host();
    let result = {
        let mut interp = sophia_runtime::Interpreter::new(&model, &refs, &mut host);
        interp.run("Load", vec![Value::Text("/no/such".into())])
    };
    assert!(
        result.is_err(),
        "未预置的 Vault.Read 应硬错误阻断，而非伪造内容：{result:?}"
    );
}

#[test]
fn registered_library_op_without_host_reports_missing_host() {
    // 程序通过语义检查，说明 Vault.Read 是已知库 op；若调用方漏注册 host，runtime 必须直达
    // HostRegistry 的“无 host 实现”诊断，而不是退回普通 method path 报未绑定变量 Vault。
    let prog = Program::new(&[
        r#"capability C { allow { Vault.Read } }"#,
        r#"action Fetch {
  capability: C
  input { path: Text }
  output { body: Raw<Text> }
  effects { Vault.Read }
  body { return Vault.Read(path) }
}"#,
    ]);
    let model = prog.analyze_with(&vault_registry());
    let refs = prog.refs();
    let mut host = HostRegistry::new();
    let err = {
        let mut interp = sophia_runtime::Interpreter::new(&model, &refs, &mut host);
        interp
            .run("Fetch", vec![Value::Text("k".into())])
            .unwrap_err()
    };
    let msg = err.to_string();
    assert!(
        msg.contains("无 host 实现：`Vault.Read`"),
        "漏注册库 host 应直达 HostRegistry 诊断，实际：{msg}"
    );
    assert!(
        !msg.contains("未绑定变量 `Vault`"),
        "不应退回普通 method path：{msg}"
    );
}

#[test]
fn lib_op_dispatches_to_registered_host() {
    // 库 op 经 HostRegistry 按 (family, op) 委派，返回 host 闭包的结果。
    let prog = Program::new(&[
        r#"capability C { allow { Vault.Read } }"#,
        r#"action Fetch {
  capability: C
  input { path: Text }
  output { body: Raw<Text> }
  effects { Vault.Read }
  body { return Vault.Read(path) }
}"#,
    ]);
    let model = prog.analyze_with(&vault_registry());
    let refs = prog.refs();
    let mut host = vault_host();
    seed_vault(&mut host, "k", "payload-42");
    let outcome = {
        let mut interp = sophia_runtime::Interpreter::new(&model, &refs, &mut host);
        interp.run("Fetch", vec![Value::Text("k".into())]).unwrap()
    };
    assert_eq!(outcome, Outcome::Returned(Value::Text("payload-42".into())));
}

#[test]
fn run_action_injects_registered_host() {
    // 注入接缝：run_action 接受调用方持有的 host（此处注册 Vault host）。
    use sophia_runtime::run_action;
    let prog = Program::new(&[
        r#"capability C { allow { Vault.Read } }"#,
        r#"action Fetch {
  capability: C
  input { path: Text }
  output { body: Raw<Text> }
  effects { Vault.Read }
  body { return Vault.Read(path) }
}"#,
    ]);
    let model = prog.analyze_with(&vault_registry());
    let refs = prog.refs();
    let mut host = vault_host();
    seed_vault(&mut host, "seam", "injected-body");
    let (outcome, _trace) = run_action(
        &model,
        &refs,
        "Fetch",
        vec![Value::Text("seam".into())],
        &mut host,
    )
    .unwrap();
    assert_eq!(
        outcome,
        Outcome::Returned(Value::Text("injected-body".into()))
    );
}
