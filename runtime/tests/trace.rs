//! 执行 Trace 投影测试（docs/language_implementation.md 9.4）。
//!
//! 验证 trace 把解释器执行映射回 Execution Graph IR 的节点与边：每次 callable 进入
//! 一条 span，携带其 `ExecNodeId` 与触发它的调用边 `ExecEdgeId`（顶层入口无入边）。

use sophia_exec_ir::ExecGraph;
use sophia_hir::{AsgIndex, IndexInput, LibraryRegistry};
use sophia_runtime::{run_action, HostRegistry, SpanOutcome, Value};
use sophia_semantic::{analyze_program, SemanticModel};
use sophia_syntax::{parse_ast, Ast};

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

    fn analyze(&self) -> SemanticModel {
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
        let index = AsgIndex::build(inputs, &LibraryRegistry::empty()).expect("index");
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
fn single_action_trace_has_one_top_level_span() {
    let prog = Program::new(&[r#"action Add {
  input { a: Int; b: Int }
  output { sum: Int }
  body { return a + b }
}"#]);
    let model = prog.analyze();
    let refs = prog.refs();
    let mut host = HostRegistry::new();
    let (_outcome, trace) = run_action(
        &model,
        &refs,
        "Add",
        vec![Value::Int(3), Value::Int(4)],
        &mut host,
    )
    .unwrap();

    assert_eq!(trace.len(), 1, "单 action 应只有一条 span");
    let span = &trace.spans()[0];
    assert_eq!(span.seq, 0);
    assert_eq!(span.callable, "Add");
    assert_eq!(span.depth, 0, "顶层入口深度为 0");
    assert_eq!(span.edge_id, None, "顶层入口无入边");
    assert_eq!(span.outcome, SpanOutcome::Returned);

    // span.node_id 投影回图：对应 Add 节点。
    let graph = ExecGraph::from_model(&model, &refs);
    assert_eq!(graph.node_id_by_name("Add"), Some(span.node_id));
}

#[test]
fn cross_call_trace_projects_call_edge() {
    // Outer 调用 Inner：trace 有两条 span，子 span 携带 Outer→Inner 的调用边 ID。
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
    let refs = prog.refs();
    let mut host = HostRegistry::new();
    let (outcome, trace) =
        run_action(&model, &refs, "Outer", vec![Value::Int(5)], &mut host).unwrap();
    assert_eq!(outcome, sophia_runtime::Outcome::Returned(Value::Int(16)));

    assert_eq!(trace.len(), 2, "Outer + Inner 两条 span");

    // 第一条：顶层 Outer（depth 0，无入边）。
    let outer = &trace.spans()[0];
    assert_eq!(outer.callable, "Outer");
    assert_eq!(outer.depth, 0);
    assert_eq!(outer.edge_id, None);

    // 第二条：被调用的 Inner（depth 1），携带 Outer→Inner 调用边 ID。
    let inner = &trace.spans()[1];
    assert_eq!(inner.callable, "Inner");
    assert_eq!(inner.depth, 1);

    let graph = ExecGraph::from_model(&model, &refs);
    let expected_edge = graph.call_edge_id("Outer", "Inner");
    assert!(expected_edge.is_some(), "图中应有 Outer→Inner 调用边");
    assert_eq!(inner.edge_id, expected_edge, "子 span 应投影到调用边");

    // 投影一致性：edge 的 from/to 对应两条 span 的 node_id。
    let edge = graph.edge(inner.edge_id.unwrap()).unwrap();
    assert_eq!(edge.from, outer.node_id);
    assert_eq!(edge.to, inner.node_id);
}

#[test]
fn trace_records_raise_outcome() {
    // 被调用方 raise：子 span 的结局投影为 Raised。
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
    let refs = prog.refs();
    let mut host = HostRegistry::new();
    let (_outcome, trace) =
        run_action(&model, &refs, "Outer", vec![Value::Int(1)], &mut host).unwrap();

    assert_eq!(trace.len(), 2);
    // 两条 span 都因领域错误冒泡而结局为 Raised（Inner raise，Outer 在边界物化为 Raised）。
    for span in trace.spans() {
        assert_eq!(
            span.outcome,
            SpanOutcome::Raised,
            "{} 的结局应投影为 Raised",
            span.callable
        );
    }
}

#[test]
fn repeated_calls_produce_distinct_ordered_spans() {
    // Outer 两次调用 Inner：trace 有 3 条 span（Outer + 两次 Inner），seq 连续递增。
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
    let a = Inner(x)
    let b = Inner(a)
    return b
  }
}"#,
    ]);
    let model = prog.analyze();
    let refs = prog.refs();
    let mut host = HostRegistry::new();
    let (outcome, trace) =
        run_action(&model, &refs, "Outer", vec![Value::Int(0)], &mut host).unwrap();
    assert_eq!(outcome, sophia_runtime::Outcome::Returned(Value::Int(2)));

    assert_eq!(trace.len(), 3, "Outer + 两次 Inner");
    // seq 连续 0,1,2（深度优先进入序）。
    assert_eq!(
        trace.spans().iter().map(|s| s.seq).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    // 后两条都是 Inner，depth=1，携带同一条调用边。
    assert_eq!(trace.spans()[1].callable, "Inner");
    assert_eq!(trace.spans()[2].callable, "Inner");
    assert_eq!(trace.spans()[1].edge_id, trace.spans()[2].edge_id);
    assert_eq!(trace.spans()[1].depth, 1);
}
