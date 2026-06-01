//! CST → AST lowering 的集成测试。
//!
//! 验证 lowering 把文档示例的表层结构正确搬运到 AST，并保持 arena/ID 引用一致。

use sophia_syntax::{
    parse_ast, BinOp, CallableKind, Effect, Expr, IncludeKind, Item, Pattern, Stmt, TypeRef,
};

const TODO_DOMAIN: &str = include_str!("../examples/TodoDomain.sophia");
const COMPLETE_TODO: &str = include_str!("../examples/CompleteTodo.sophia");
const CONTROL: &str = include_str!("../examples/Control.sophia");

#[test]
fn lowers_all_top_level_items() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let names: Vec<(&str, &str)> = ast
        .items
        .iter()
        .map(|it| (item_kind(it), it.name().text.as_str()))
        .collect();

    assert!(names.contains(&("domain", "TodoDomain")));
    assert!(names.contains(&("entity", "Todo")));
    assert!(names.contains(&("state", "TodoStatus")));
    assert!(names.contains(&("transition", "CompleteTodoTransition")));
    assert!(names.contains(&("error", "TodoError")));
    assert!(names.contains(&("capability", "TodoCapability")));
    assert!(names.contains(&("task", "ImplementCompleteTodo")));
}

#[test]
fn entity_fields_and_assists_lowered() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let entity = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Entity(e) if e.name.text == "Todo" => Some(e),
            _ => None,
        })
        .expect("Todo entity");

    // 5 个字段。
    let field_names: Vec<&str> = entity.fields.iter().map(|f| f.name.text.as_str()).collect();
    assert_eq!(
        field_names,
        ["id", "title", "status", "created_at", "completed_at"]
    );

    // completed_at 是 one of { Time, Null }。
    let completed = entity
        .fields
        .iter()
        .find(|f| f.name.text == "completed_at")
        .unwrap();
    match &completed.ty {
        TypeRef::OneOf { members, .. } => {
            assert_eq!(members.len(), 2);
            assert!(matches!(&members[0], TypeRef::Named { name, .. } if name.text == "Time"));
            assert!(matches!(&members[1], TypeRef::Named { name, .. } if name.text == "Null"));
        }
        other => panic!("期望 one of {{ Time, Null }}，得到 {other:?}"),
    }

    // assist：meaning + not（not 是多值）。
    assert!(entity
        .assists
        .iter()
        .any(|a| matches!(a.key, sophia_syntax::AssistKey::Meaning)));
    let not_field = entity
        .assists
        .iter()
        .find(|a| matches!(a.key, sophia_syntax::AssistKey::Not))
        .expect("not assist");
    assert_eq!(not_field.values.len(), 2);

    // 不变量：TitleNotEmpty（仅 require）、DoneHasCompletionTime（when + require）。
    assert_eq!(entity.invariants.len(), 2);
    let done = entity
        .invariants
        .iter()
        .find(|i| i.name.text == "DoneHasCompletionTime")
        .unwrap();
    assert!(done.when.is_some());
    assert!(done.require.is_some());

    // semantic_identity / evolution 被解析。
    let si = entity
        .semantic_identity
        .as_ref()
        .expect("semantic_identity");
    assert_eq!(si.core_capability.len(), 2);
    assert_eq!(si.drift_tolerance.as_deref(), Some("0.15"));
    let evo = entity.evolution.as_ref().expect("evolution");
    assert_eq!(evo.allowed.len(), 2);
}

#[test]
fn state_values_lowered() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let state = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::State(s) => Some(s),
            _ => None,
        })
        .unwrap();
    let values: Vec<&str> = state.values.iter().map(|v| v.name.text.as_str()).collect();
    assert_eq!(values, ["Pending", "Done"]);
}

#[test]
fn error_variants_and_fields_lowered() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let err = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Error(e) => Some(e),
            _ => None,
        })
        .unwrap();
    assert_eq!(err.variants.len(), 1);
    let already = err
        .variants
        .iter()
        .find(|v| v.name.text == "TodoAlreadyDone")
        .unwrap();
    let fnames: Vec<&str> = already
        .fields
        .iter()
        .map(|f| f.name.text.as_str())
        .collect();
    assert_eq!(fnames, ["id", "done_at"]);
}

#[test]
fn capability_allow_deny_lowered() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let cap = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Capability(c) => Some(c),
            _ => None,
        })
        .unwrap();
    assert_eq!(cap.allow.len(), 1);
    assert_eq!(cap.deny.len(), 0);
    assert!(is_console_write(&cap.allow[0]));
}

/// 断言一个 effect 是 `Console.Write`。
fn is_console_write(e: &Effect) -> bool {
    matches!(
        e,
        Effect::Op { family, op, args, .. }
            if family.text == "Console" && op.text == "Write" && args.is_empty()
    )
}

#[test]
fn intent_field_type_lowered() {
    // entity Todo 的 title 字段是 Sanitized<Text>；id 字段是裸 Uuid（intent / 具名 lowering）。
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let entity = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Entity(e) if e.name.text == "Todo" => Some(e),
            _ => None,
        })
        .unwrap();
    let title = entity
        .fields
        .iter()
        .find(|f| f.name.text == "title")
        .unwrap();
    assert!(matches!(
        &title.ty,
        TypeRef::Intent { head, .. } if head.text == "Sanitized"
    ));
    let id = entity.fields.iter().find(|f| f.name.text == "id").unwrap();
    assert!(matches!(
        &id.ty,
        TypeRef::Named { name, .. } if name.text == "Uuid"
    ));
}

#[test]
fn task_includes_and_excludes_lowered() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let task = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Task(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        task.goal.as_ref().unwrap().value,
        "Implement and verify CompleteTodo."
    );
    assert_eq!(task.includes.len(), 6);
    assert!(task
        .includes
        .iter()
        .any(|i| i.kind == IncludeKind::Action && i.name.text == "CompleteTodo"));
    assert_eq!(task.excludes.len(), 1);
}

#[test]
fn transition_is_pure_with_body_and_ensures() {
    let ast = parse_ast(TODO_DOMAIN).expect("parse");
    let tr = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Transition(c) => Some(c),
            _ => None,
        })
        .unwrap();
    assert_eq!(tr.kind, CallableKind::Transition);
    assert_eq!(tr.effects, vec![Effect::Pure]);
    assert!(tr.body.is_some());
    assert_eq!(tr.ensures.len(), 2);
    // input: todo (with predicate) + completed_time。
    assert_eq!(tr.inputs.len(), 2);
    assert!(tr.inputs[0].predicate.is_some());
}

#[test]
fn action_body_match_and_raise_lowered() {
    let ast = parse_ast(COMPLETE_TODO).expect("parse");
    let action = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Action(c) => Some(c),
            _ => None,
        })
        .unwrap();

    assert_eq!(action.kind, CallableKind::Action);
    assert_eq!(action.capability.as_ref().unwrap().text, "TodoCapability");
    assert_eq!(action.errors.len(), 1);
    // effects: Console.Write。
    assert_eq!(action.effects.len(), 1);

    let body = action.body.as_ref().unwrap();
    // body 唯一顶层语句是对 todo.status 的 match。
    let Stmt::Match { arms, .. } = &body.stmts[0] else {
        panic!("期望第一条语句为 match");
    };
    assert_eq!(arms.len(), 2);
    // 两个 arm 都是 state 值 pattern（TodoStatus.Done / TodoStatus.Pending）。
    assert!(matches!(arms[0].pattern, Pattern::State { .. }));
    assert!(matches!(arms[1].pattern, Pattern::State { .. }));
}

#[test]
fn control_flow_lowered() {
    let ast = parse_ast(CONTROL).expect("parse");
    let report = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Action(c) if c.name.text == "ReportProgress" => Some(c),
            _ => None,
        })
        .unwrap();
    let body = report.body.as_ref().unwrap();

    // let mutable total = 0
    assert!(matches!(&body.stmts[0], Stmt::Let { mutable: true, .. }));
    // repeat 3 times { set total = total + 1 }
    let Stmt::Repeat { body: rbody, .. } = &body.stmts[1] else {
        panic!("期望 repeat");
    };
    assert!(matches!(rbody.stmts[0], Stmt::Set { .. }));
    // if total > 2 { print } else { print }
    let Stmt::If { alternative, .. } = &body.stmts[2] else {
        panic!("期望 if");
    };
    assert!(alternative.is_some());
}

#[test]
fn intent_conversion_flag_lowered() {
    let ast = parse_ast(CONTROL).expect("parse");
    let sanitize = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Action(c) if c.name.text == "SanitizeTitle" => Some(c),
            _ => None,
        })
        .unwrap();
    assert!(sanitize.intent_conversion);
}

#[test]
fn binary_expr_and_precedence_in_ast() {
    // total > 0 and not (total == 5) —— 验证表达式 arena 与运算符。
    let ast = parse_ast(CONTROL).expect("parse");
    let report = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::Action(c) if c.name.text == "ReportProgress" => Some(c),
            _ => None,
        })
        .unwrap();
    let body = report.body.as_ref().unwrap();
    // 找到 `let flag = ...`
    let flag = body
        .stmts
        .iter()
        .find_map(|s| match s {
            Stmt::Let { name, value, .. } if name.text == "flag" => Some(*value),
            _ => None,
        })
        .unwrap();
    // 顶层应是 `and`。
    match ast.expr(flag) {
        Expr::Binary { op, .. } => assert_eq!(*op, BinOp::And),
        other => panic!("期望顶层 and，得到 {other:?}"),
    }
}

#[test]
fn call_and_method_call_args_lowered() {
    let src = "action A { body { let x = foo(a, b) let y = obj.bar(c, d, e) return x } }";
    let ast = parse_ast(src).expect("parse");
    let Item::Action(action) = &ast.items[0] else {
        panic!("期望 action");
    };
    let body = action.body.as_ref().unwrap();

    // foo(a, b) —— 2 个实参。
    let Stmt::Let { value, .. } = &body.stmts[0] else {
        panic!("期望 let");
    };
    match ast.expr(*value) {
        Expr::Call { callee, args, .. } => {
            assert_eq!(callee.text, "foo");
            assert_eq!(args.len(), 2);
        }
        other => panic!("期望 call_expr，得到 {other:?}"),
    }

    // obj.bar(c, d, e) —— base = obj，3 个实参。
    let Stmt::Let { value, .. } = &body.stmts[1] else {
        panic!("期望 let");
    };
    match ast.expr(*value) {
        Expr::MethodCall { method, args, .. } => {
            assert_eq!(method.text, "bar");
            assert_eq!(args.len(), 3);
        }
        other => panic!("期望 method_call，得到 {other:?}"),
    }
}

#[test]
fn comments_are_discarded_inside_expressions() {
    // 注释是 grammar 的 extras，可能出现在表达式内部；lowering 必须丢弃 trivia。
    let src = "action A { body { let x = foo(/* c */ 5) return x } }";
    let ast = parse_ast(src).expect("parse");
    let Item::Action(action) = &ast.items[0] else {
        panic!("期望 action");
    };
    let body = action.body.as_ref().unwrap();
    let Stmt::Let { value, .. } = &body.stmts[0] else {
        panic!("期望 let");
    };
    // foo(5) 应被正确还原，唯一实参为整数 5（注释被丢弃）。
    match ast.expr(*value) {
        Expr::Call { callee, args, .. } => {
            assert_eq!(callee.text, "foo");
            assert_eq!(args.len(), 1);
            match ast.expr(args[0]) {
                Expr::Int { text, .. } => assert_eq!(text, "5"),
                other => panic!("实参应为整数 5，得到 {other:?}"),
            }
        }
        other => panic!("期望 call(...)，得到 {other:?}"),
    }
}

fn item_kind(it: &Item) -> &'static str {
    match it {
        Item::Domain(_) => "domain",
        Item::Entity(_) => "entity",
        Item::State(_) => "state",
        Item::Transition(_) => "transition",
        Item::Error(_) => "error",
        Item::Capability(_) => "capability",
        Item::Action(_) => "action",
        Item::Task(_) => "task",
        Item::Effect(_) => "effect",
    }
}

#[test]
fn lowers_effect_declaration() {
    let src = r#"effect Llm {
  meaning: "大模型补全。"
  operation Complete { param model: Text }
  operation Embed
}"#;
    let ast = parse_ast(src).expect("parse");
    let Item::Effect(e) = &ast.items[0] else {
        panic!("期望 effect");
    };
    assert_eq!(e.name.text, "Llm");
    assert_eq!(e.operations.len(), 2);
    assert_eq!(e.operations[0].name.text, "Complete");
    assert_eq!(e.operations[0].params.len(), 1);
    assert_eq!(e.operations[0].params[0].name.text, "model");
    // 无参 operation。
    assert_eq!(e.operations[1].name.text, "Embed");
    assert!(e.operations[1].params.is_empty());
    // assist 字段不影响形式核心。
    assert_eq!(e.assists.len(), 1);
}

#[test]
fn strip_assists_removes_effect_assist() {
    // effect 声明的 assist 字段应被 strip 移除，形式核心（operations）不变。
    let src = r#"effect Payment {
  meaning: "支付副作用族。"
  operation Charge { param amount: Int }
}"#;
    let mut ast = parse_ast(src).expect("parse");
    ast.strip_assists();
    let Item::Effect(e) = &ast.items[0] else {
        panic!("期望 effect");
    };
    assert!(e.assists.is_empty(), "assist 应被移除");
    assert_eq!(e.operations.len(), 1, "operations 不应受影响");
}

#[test]
fn type_of_family_lowered() {
    // 统一类型语法：list of / one of / schema of / intent <>，各落到对应 TypeRef 变体。
    let src = "entity E { fields { \
        a { type: list of Int } \
        b { type: schema of Text } \
        c { type: one of { Int, Null } } \
        d { type: Sanitized<Text> } \
    } }";
    let ast = parse_ast(src).expect("parse");
    let Item::Entity(e) = &ast.items[0] else {
        panic!("期望 entity");
    };
    let ty = |name: &str| &e.fields.iter().find(|f| f.name.text == name).unwrap().ty;
    // a: list of Int
    match ty("a") {
        TypeRef::ListOf { elem, .. } => {
            assert!(matches!(elem.as_ref(), TypeRef::Named { name, .. } if name.text == "Int"))
        }
        other => panic!("a 期望 list of Int，得到 {other:?}"),
    }
    // b: schema of Text
    match ty("b") {
        TypeRef::SchemaOf { arg, .. } => {
            assert!(matches!(arg.as_ref(), TypeRef::Named { name, .. } if name.text == "Text"))
        }
        other => panic!("b 期望 schema of Text，得到 {other:?}"),
    }
    // c: one of { Int, Null }
    match ty("c") {
        TypeRef::OneOf { members, .. } => assert_eq!(members.len(), 2),
        other => panic!("c 期望 one of，得到 {other:?}"),
    }
    // d: Sanitized<Text>（intent，<> 专属）
    match ty("d") {
        TypeRef::Intent { head, arg, .. } => {
            assert_eq!(head.text, "Sanitized");
            assert!(matches!(arg.as_ref(), TypeRef::Named { name, .. } if name.text == "Text"));
        }
        other => panic!("d 期望 Sanitized<Text>，得到 {other:?}"),
    }
}
