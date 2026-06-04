//! Selection / Materialize 编排测试：gate 通过 → 建 Selection + Materialize 节点 + 边 + 原子写入。

mod common;

use common::{seed_code, temp_dir};
use sophia_engine::{run_selection_materialize, SelectMaterializeError};
use sophia_graph_db::{
    EdgeKind, GraphStore, NodeId, NodePayload, NodeRole, ObjectivePayload, Provenance,
};
use sophia_materialize::{CodeCandidate, GateReport};

/// 建一个通过全部 gate 的 Selected 候选。
fn selected(files: Vec<(String, String)>) -> CodeCandidate<sophia_materialize::Selected> {
    CodeCandidate::new(files)
        .run_check(&GateReport::pass())
        .unwrap()
        .run_audit(&GateReport::pass())
        .unwrap()
        .run_runtime_validation(&GateReport::pass(), &GateReport::pass())
        .unwrap()
        .select()
}

/// 建一个 LLM Code 节点（带 consumed→ snapshot 以满足 I6）作为 selects→ 目标。
fn code_node(store: &mut GraphStore, files: Vec<String>) -> NodeId {
    seed_code(store, files)
}

#[test]
fn happy_path_creates_nodes_edges_and_writes_files() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(&mut store, vec!["TodoDomain/entities/Todo.sophia".into()]);

    let root = temp_dir("happy");
    let candidate = selected(vec![(
        "TodoDomain/entities/Todo.sophia".into(),
        "entity Todo {}".into(),
    )]);

    let outcome = run_selection_materialize(
        &mut store,
        candidate,
        code,
        &root,
        "domains",
        "唯一通过全部 gate 的候选",
    )
    .unwrap();

    // Selection / Materialize 节点为 Deterministic provenance。
    assert_eq!(store.role_of(outcome.selection), Some(NodeRole::Selection));
    assert_eq!(
        store.provenance_of(outcome.selection),
        Some(Provenance::Deterministic)
    );
    assert_eq!(
        store.role_of(outcome.materialize),
        Some(NodeRole::Materialize)
    );
    assert_eq!(
        store.provenance_of(outcome.materialize),
        Some(Provenance::Deterministic)
    );

    // 边：selects→ Code、materializes→ Selection。
    assert!(store.has_edge(outcome.selection, code, EdgeKind::Selects));
    assert!(store.has_edge(
        outcome.materialize,
        outcome.selection,
        EdgeKind::Materializes
    ));

    // 文件已写入。
    let written = root.join("TodoDomain/entities/Todo.sophia");
    assert_eq!(std::fs::read_to_string(&written).unwrap(), "entity Todo {}");

    // MaterializeNode payload 记录逻辑根与相对文件。
    match &store.node(outcome.materialize).unwrap().payload {
        NodePayload::Materialize(m) => {
            assert_eq!(m.target_root, "domains");
            assert_eq!(m.files, vec!["TodoDomain/entities/Todo.sophia".to_string()]);
        }
        _ => panic!("应为 Materialize payload"),
    }

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn rejects_non_code_target() {
    let mut store = GraphStore::open_in_memory().unwrap();
    // 目标是 Objective，不是 Code。
    let obj = store
        .as_human()
        .objective(
            "goal",
            ObjectivePayload {
                title: "G".into(),
                description: "d".into(),
            },
        )
        .unwrap();

    let root = temp_dir("reject");
    let candidate = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);

    let err = run_selection_materialize(&mut store, candidate, obj, &root, "domains", "rationale")
        .unwrap_err();
    assert!(matches!(err, SelectMaterializeError::NotCodeNode(_)));

    // 不应产生 Selection / Materialize 节点（前置校验先行）。
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Selection));
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Materialize));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn i6_still_holds_after_orchestration() {
    // Selection / Materialize 是 Deterministic 节点，不受 I6 约束；编排后图整体仍满足 I6。
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(&mut store, vec!["D/A.sophia".into()]);
    let root = temp_dir("i6");
    let candidate = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);

    run_selection_materialize(&mut store, candidate, code, &root, "domains", "r").unwrap();

    store.validate_i6().unwrap();

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn multi_file_materialization() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(
        &mut store,
        vec!["D/A.sophia".into(), "D/sub/B.sophia".into()],
    );
    let root = temp_dir("multi");
    let candidate = selected(vec![
        ("D/A.sophia".into(), "entity A {}".into()),
        ("D/sub/B.sophia".into(), "entity B {}".into()),
    ]);

    let outcome =
        run_selection_materialize(&mut store, candidate, code, &root, "domains", "r").unwrap();

    assert_eq!(outcome.written.files.len(), 2);
    assert!(root.join("D/A.sophia").exists());
    assert!(root.join("D/sub/B.sophia").exists());

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn split_select_then_materialize_two_steps() {
    // run_selection 与 run_materialization 分两步（模拟 CLI select / materialize 两进程）。
    use sophia_engine::{run_materialization, run_selection};
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(&mut store, vec!["D/A.sophia".into()]);
    let root = temp_dir("split");

    // 步骤 1：select（候选作为 gate 证明，按引用传入）。
    let cand1 = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);
    let selection = run_selection(&mut store, &cand1, code, "唯一候选").unwrap();
    assert_eq!(store.role_of(selection), Some(NodeRole::Selection));
    assert!(store.has_edge(selection, code, EdgeKind::Selects));

    // 步骤 2：materialize（重新构造 gate 证明 → 写盘 + MaterializeNode）。
    let cand2 = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);
    let (materialize, written) =
        run_materialization(&mut store, cand2, selection, &root, "domains").unwrap();
    assert_eq!(store.role_of(materialize), Some(NodeRole::Materialize));
    assert!(store.has_edge(materialize, selection, EdgeKind::Materializes));
    assert_eq!(written.files, vec!["D/A.sophia".to_string()]);
    assert!(root.join("D/A.sophia").exists());

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn materialization_rejects_non_selection_node() {
    use sophia_engine::run_materialization;
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(&mut store, vec!["D/A.sophia".into()]);
    let root = temp_dir("matbad");
    let cand = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);

    // 传 Code 节点当 selection → 应拒绝。
    let err = run_materialization(&mut store, cand, code, &root, "domains").unwrap_err();
    assert!(matches!(err, SelectMaterializeError::NotSelectionNode(_)));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn materialization_rejects_invalid_payload_before_file_write() {
    use sophia_engine::{run_materialization, run_selection};
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(&mut store, vec!["D/A.sophia".into()]);
    let root = temp_dir("mat_payload_bad");
    let selected_for_selection = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);
    let selection = run_selection(&mut store, &selected_for_selection, code, "唯一候选").unwrap();
    let cand = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);

    let err = run_materialization(&mut store, cand, selection, &root, " ").unwrap_err();
    assert!(matches!(err, SelectMaterializeError::Graph(_)));
    assert!(
        !root.join("D/A.sophia").exists(),
        "Materialize payload 非法时不应先写文件"
    );
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Materialize));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn selection_materialize_prevalidates_materialize_payload_before_selection() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let code = code_node(&mut store, vec!["D/A.sophia".into()]);
    let root = temp_dir("select_mat_payload_bad");
    let candidate = selected(vec![("D/A.sophia".into(), "entity A {}".into())]);

    let err = run_selection_materialize(&mut store, candidate, code, &root, " ", "r").unwrap_err();
    assert!(matches!(err, SelectMaterializeError::Graph(_)));
    assert!(
        !root.join("D/A.sophia").exists(),
        "一体化路径预校验失败时不应写文件"
    );
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Selection));
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Materialize));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn ranked_selection_picks_compilable_winner() {
    // 多候选：一个可编译、一个不可编译；评分应选中可编译者并建 SelectionNode（selects→ 它）。
    use sophia_engine::{run_ranked_selection, RankedCandidate};

    let mut store = GraphStore::open_in_memory().unwrap();
    let bad_node = code_node(&mut store, vec!["D/Bad.sophia".into()]);
    let good_node = code_node(&mut store, vec!["D/Good.sophia".into()]);

    let bad_files = vec![("D/Bad.sophia".to_string(), "action A { broken".to_string())];
    let good_files = vec![(
        "D/Good.sophia".to_string(),
        "action A { input { x: Int } output { y: Int } body { return x } }".to_string(),
    )];

    let candidates = vec![
        RankedCandidate {
            code_node: bad_node,
            candidate: selected(bad_files.clone()),
            files: bad_files,
            compile_pass: false,
            tests_pass: false,
            constraints_pass: false,
            pseudocode_clarity: None,
        },
        RankedCandidate {
            code_node: good_node,
            candidate: selected(good_files.clone()),
            files: good_files,
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            pseudocode_clarity: None,
        },
    ];

    let result = run_ranked_selection(&mut store, candidates, None).unwrap();
    // winner 是第二个（可编译）。
    assert_eq!(result.winner_index, 1);
    // SelectionNode selects→ good_node（胜出候选）。
    assert_eq!(store.role_of(result.selection), Some(NodeRole::Selection));
    assert!(store.has_edge(result.selection, good_node, EdgeKind::Selects));
    // 评分不入图：图中无 Score 角色，只多了一个 Selection 节点。
    assert_eq!(result.ranking.len(), 2);
    assert!(result.ranking[0].1.overall > result.ranking[1].1.overall);
    // 胜出候选可继续物化（类型证明仍在）。
    let _ = result.candidate.file_paths();
}

#[test]
fn ranked_selection_empty_errors() {
    use sophia_engine::{run_ranked_selection, RankedCandidate};
    let mut store = GraphStore::open_in_memory().unwrap();
    let empty: Vec<RankedCandidate> = vec![];
    let err = run_ranked_selection(&mut store, empty, None).unwrap_err();
    assert!(matches!(err, SelectMaterializeError::NoCandidates));
}
