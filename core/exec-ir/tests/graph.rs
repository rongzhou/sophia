//! Execution Graph IR 构建测试。

use sophia_exec_ir::{ExecGraph, ExecNodeKind};
use sophia_hir::{AsgIndex, IndexInput, LibraryRegistry};
use sophia_semantic::SemanticModel;
use sophia_syntax::{parse_ast, Ast};

#[test]
fn builds_one_node_per_callable_in_stable_order() {
    let action =
        parse_ast("action Beta { input { x: Int } output { y: Int } body { return x } }").unwrap();
    let transition = parse_ast(
        "transition Alpha { input { x: Int } output { y: Int } effects { Pure } body { return x } }",
    )
    .unwrap();

    let inputs = vec![
        IndexInput {
            domain: "D",
            path: "domains/D/actions/Beta.sophia",
            ast: &action,
        },
        IndexInput {
            domain: "D",
            path: "domains/D/transitions/Alpha.sophia",
            ast: &transition,
        },
    ];
    let index = AsgIndex::build(inputs, &LibraryRegistry::empty()).unwrap();
    let asts: Vec<&Ast> = vec![&action, &transition];
    let model = SemanticModel::build(&asts, &index);

    let graph = ExecGraph::from_model(&model, &asts);
    // 两个 callable → 两个节点；按名字典序（Alpha 在 Beta 前）。
    assert_eq!(graph.nodes().len(), 2);
    assert_eq!(graph.nodes()[0].name(), "Alpha");
    assert_eq!(graph.nodes()[1].name(), "Beta");
    assert!(matches!(graph.nodes()[0].kind, ExecNodeKind::Transition(_)));
    assert!(matches!(graph.nodes()[1].kind, ExecNodeKind::Action(_)));

    // 按名查找。
    assert!(graph.node_by_name("Alpha").is_some());
    assert!(graph.node_by_name("Ghost").is_none());
}

#[test]
fn builds_call_edges_from_body() {
    // action Caller 调用 transition Helper（构造式调用）→ 一条 Control 调用边。
    let helper = parse_ast(
        "transition Helper { input { x: Int } output { y: Int } effects { Pure } body { return x } }",
    )
    .unwrap();
    let caller = parse_ast(
        "action Caller { input { n: Int } output { y: Int } body { let r = Helper { x = n } return r } }",
    )
    .unwrap();

    let inputs = vec![
        IndexInput {
            domain: "D",
            path: "domains/D/transitions/Helper.sophia",
            ast: &helper,
        },
        IndexInput {
            domain: "D",
            path: "domains/D/actions/Caller.sophia",
            ast: &caller,
        },
    ];
    let index = AsgIndex::build(inputs, &LibraryRegistry::empty()).unwrap();
    let asts: Vec<&Ast> = vec![&helper, &caller];
    let model = SemanticModel::build(&asts, &index);

    let graph = ExecGraph::from_model(&model, &asts);
    assert!(graph.has_node("Caller"));
    assert!(graph.has_node("Helper"));
    // Caller → Helper 调用边存在；反向不存在。
    assert!(graph.has_call_edge("Caller", "Helper"));
    assert!(!graph.has_call_edge("Helper", "Caller"));
    assert_eq!(graph.edges().len(), 1);
}

#[test]
fn no_edge_to_non_callable_constructs() {
    // 构造 entity（非 callable）不产生调用边。
    let ent = parse_ast("entity T { fields { v { type: Int } } }").unwrap();
    let act =
        parse_ast("action Make { input { n: Int } output { t: T } body { return T { v = n } } }")
            .unwrap();
    let inputs = vec![
        IndexInput {
            domain: "D",
            path: "domains/D/entities/T.sophia",
            ast: &ent,
        },
        IndexInput {
            domain: "D",
            path: "domains/D/actions/Make.sophia",
            ast: &act,
        },
    ];
    let index = AsgIndex::build(inputs, &LibraryRegistry::empty()).unwrap();
    let asts: Vec<&Ast> = vec![&ent, &act];
    let model = SemanticModel::build(&asts, &index);
    let graph = ExecGraph::from_model(&model, &asts);
    // entity 不是 callable，无调用边。
    assert_eq!(graph.edges().len(), 0);
}

/// Execution Graph IR 结构的确定性快照（守护节点 / 调用边构建不被静默改动）。
///
/// 用一个三 callable 程序（一个 action 调用两个 transition），把图渲染为稳定文本
/// （节点按名、边按 from→to）后 `insta` 快照。任何节点 / 边语义变化都会被快照捕获。
#[test]
fn exec_graph_structure_snapshot() {
    let add = parse_ast(
        "transition Add { input { a: Int; b: Int } output { y: Int } effects { Pure } body { return a + b } }",
    )
    .unwrap();
    let dbl = parse_ast(
        "transition Double { input { n: Int } output { y: Int } effects { Pure } body { return n + n } }",
    )
    .unwrap();
    let combine = parse_ast(
        "action Combine { input { x: Int } output { y: Int } body { let d = Double { n = x } let s = Add { a = d, b = x } return s } }",
    )
    .unwrap();
    let inputs = vec![
        IndexInput {
            domain: "D",
            path: "domains/D/transitions/Add.sophia",
            ast: &add,
        },
        IndexInput {
            domain: "D",
            path: "domains/D/transitions/Double.sophia",
            ast: &dbl,
        },
        IndexInput {
            domain: "D",
            path: "domains/D/actions/Combine.sophia",
            ast: &combine,
        },
    ];
    let index = AsgIndex::build(inputs, &LibraryRegistry::empty()).unwrap();
    let asts: Vec<&Ast> = vec![&add, &dbl, &combine];
    let model = SemanticModel::build(&asts, &index);
    let graph = ExecGraph::from_model(&model, &asts);

    // 渲染为稳定文本：节点（按构建序=名字典序）+ 边（from→to，kind）。
    let mut out = String::from("nodes:\n");
    for n in graph.nodes() {
        out.push_str(&format!("  {:?}\n", n.kind));
    }
    out.push_str("edges:\n");
    for e in graph.edges() {
        let from = graph.nodes()[e.from.0 as usize].name();
        let to = graph.nodes()[e.to.0 as usize].name();
        out.push_str(&format!("  {from} --{:?}-> {to}\n", e.kind));
    }
    insta::assert_snapshot!("exec_graph_structure", out);
}
