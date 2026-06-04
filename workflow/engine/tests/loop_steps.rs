//! 工作流闭环测试：design → implement → repair 串接，建产物节点 + 边。

mod common;

use common::{library_policy, req, seed_objective, MockClient};
use sophia_engine::{design_solution, implement_design, repair_code, LoopStepOutcome};
use sophia_graph_db::{EdgeKind, GraphStore, NodeRole, Provenance};
use sophia_llm::{LlmError, StructuredConfig};

#[tokio::test]
async fn full_loop_design_implement_repair() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);

    // 1) design_solution → Pseudocode。
    let design_client = MockClient::new(vec![Ok(
        r##"{"purpose":"complete todo","pseudocode":"# Purpose\n..."}"##.into(),
    )]);
    let pseudo = match design_solution(
        &mut store,
        &design_client,
        |_ctx| req(),
        &StructuredConfig::default(),
        &library_policy(),
        obj,
    )
    .await
    .unwrap()
    {
        LoopStepOutcome::Succeeded(art) => {
            assert_eq!(art.text, "# Purpose\n...");
            art.node
        }
        LoopStepOutcome::Failed { .. } => panic!("design 应成功"),
    };
    assert_eq!(store.role_of(pseudo), Some(NodeRole::Pseudocode));
    assert_eq!(store.provenance_of(pseudo), Some(Provenance::Llm));
    assert!(store.has_edge(pseudo, obj, EdgeKind::Addresses));

    // 2) implement_design → Code，implements→ Pseudocode。
    let impl_client = MockClient::new(vec![Ok(
        r#"{"files":[{"path":"D/A.sophia","content":"entity A {}"}]}"#.into(),
    )]);
    let code = match implement_design(
        &mut store,
        &impl_client,
        |_ctx| req(),
        &StructuredConfig::default(),
        obj,
        pseudo,
    )
    .await
    .unwrap()
    {
        LoopStepOutcome::Succeeded(art) => {
            assert_eq!(
                art.files,
                vec![("D/A.sophia".to_string(), "entity A {}".to_string())]
            );
            art.node
        }
        LoopStepOutcome::Failed { .. } => panic!("implement 应成功"),
    };
    assert_eq!(store.role_of(code), Some(NodeRole::Code));
    assert!(store.has_edge(code, obj, EdgeKind::Addresses));
    assert!(store.has_edge(code, pseudo, EdgeKind::Implements));

    // 3) repair_code → 新 Code，repairs→ 旧 Code。
    let repair_client = MockClient::new(vec![Ok(
        r#"{"files":[{"path":"D/A.sophia","content":"entity A { x }"}],"changes":["加字段 x"]}"#
            .into(),
    )]);
    let code2 = match repair_code(
        &mut store,
        &repair_client,
        |_ctx| req(),
        &StructuredConfig::default(),
        obj,
        code,
    )
    .await
    .unwrap()
    {
        LoopStepOutcome::Succeeded(art) => art.node,
        LoopStepOutcome::Failed { .. } => panic!("repair 应成功"),
    };
    assert_eq!(store.role_of(code2), Some(NodeRole::Code));
    assert!(store.has_edge(code2, code, EdgeKind::Repairs));
    assert!(store.has_edge(code2, obj, EdgeKind::Addresses));

    // 全图满足 I6（每个 LLM 产物都 consumed→ snapshot）。
    store.validate_i6().unwrap();
}

#[tokio::test]
async fn design_propagates_selected_libraries() {
    // S2 两阶段：design 输出的 libraries（LLM 从目录选库）须经 PseudocodeArtifact 传出，
    // 供 implement 注入对应库资产。
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);
    let client = MockClient::new(vec![Ok(
        r##"{"purpose":"fetch","pseudocode":"# Purpose\n...","libraries":["http"]}"##.into(),
    )]);
    let art = match design_solution(
        &mut store,
        &client,
        |_ctx| req(),
        &StructuredConfig::default(),
        &library_policy(),
        obj,
    )
    .await
    .unwrap()
    {
        LoopStepOutcome::Succeeded(a) => a,
        LoopStepOutcome::Failed { .. } => panic!("design 应成功"),
    };
    assert_eq!(art.libraries, vec!["http".to_string()]);
}

#[tokio::test]
async fn design_libraries_defaults_empty_when_absent() {
    // design 未声明 libraries（不用库）→ 默认空（serde default）。
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);
    let client = MockClient::new(vec![Ok(
        r##"{"purpose":"pure","pseudocode":"# Purpose\n..."}"##.into(),
    )]);
    let art = match design_solution(
        &mut store,
        &client,
        |_ctx| req(),
        &StructuredConfig::default(),
        &library_policy(),
        obj,
    )
    .await
    .unwrap()
    {
        LoopStepOutcome::Succeeded(a) => a,
        LoopStepOutcome::Failed { .. } => panic!("design 应成功"),
    };
    assert!(
        art.libraries.is_empty(),
        "未声明库应默认空：{:?}",
        art.libraries
    );
}

#[tokio::test]
async fn design_failure_emits_raw_llm_and_no_pseudocode() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);
    let client = MockClient::new(vec![Err(LlmError::BackendUnavailable("down".into()))]);

    let outcome = design_solution(
        &mut store,
        &client,
        |_ctx| req(),
        &StructuredConfig::default(),
        &library_policy(),
        obj,
    )
    .await
    .unwrap();

    match outcome {
        LoopStepOutcome::Failed { raw_llm, error } => {
            assert!(matches!(error, LlmError::BackendUnavailable(_)));
            assert_eq!(store.role_of(raw_llm), Some(NodeRole::RawLlm));
            assert!(store.has_edge(raw_llm, obj, EdgeKind::Attempted));
        }
        LoopStepOutcome::Succeeded(_) => panic!("后端不可用应失败"),
    }
    // 不应有 Pseudocode 节点。
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Pseudocode));
}

#[tokio::test]
async fn design_rejects_unknown_library_before_pseudocode_node() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);
    let invalid =
        r##"{"purpose":"bad lib","pseudocode":"# Purpose\n...","libraries":["missing"]}"##;
    let client = MockClient::new(vec![
        Ok(invalid.into()),
        Ok(invalid.into()),
        Ok(invalid.into()),
    ]);

    let outcome = design_solution(
        &mut store,
        &client,
        |_ctx| req(),
        &StructuredConfig::default(),
        &library_policy(),
        obj,
    )
    .await
    .unwrap();

    match outcome {
        LoopStepOutcome::Failed { raw_llm, error } => {
            assert!(
                matches!(
                    error,
                    LlmError::SchemaValidation { .. } | LlmError::SelfCheck(_)
                ),
                "未知库应作为结构化输出失败：{error}"
            );
            assert_eq!(store.role_of(raw_llm), Some(NodeRole::RawLlm));
            assert!(store.has_edge(raw_llm, obj, EdgeKind::Attempted));
        }
        LoopStepOutcome::Succeeded(_) => panic!("未知库应在建 Pseudocode 前失败"),
    }
    assert!(store.nodes().all(|n| n.meta.role != NodeRole::Pseudocode));
}

#[tokio::test]
async fn implement_rejects_non_pseudocode_source() {
    let mut store = GraphStore::open_in_memory().unwrap();
    let obj = seed_objective(&mut store);
    // 传一个 Objective 当作 pseudocode → 应硬错误（图结构前置校验）。
    let client = MockClient::new(vec![Ok(r#"{"files":[{"path":"a","content":"x"}]}"#.into())]);
    let err = implement_design(
        &mut store,
        &client,
        |_ctx| req(),
        &StructuredConfig::default(),
        obj,
        obj,
    )
    .await
    .unwrap_err();
    // LoopError = LlmStepError::Graph
    assert!(matches!(err, sophia_engine::LoopError::Graph(_)));
}

#[tokio::test]
async fn design_rejects_non_addressable_target() {
    let mut store = GraphStore::open_in_memory().unwrap();
    // 用一个 Constraint 当 target（不是 addresses→ 允许的目标域）。
    let c = store
        .as_human()
        .constraint(
            "c",
            sophia_graph_db::ConstraintPayload {
                kind: sophia_graph_db::ConstraintKind::Invariant,
                statement: "keep".into(),
                verifier: None,
            },
        )
        .unwrap();
    let client = MockClient::new(vec![Ok(r#"{"purpose":"p","pseudocode":"q"}"#.into())]);
    let err = design_solution(
        &mut store,
        &client,
        |_ctx| req(),
        &StructuredConfig::default(),
        &library_policy(),
        c,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, sophia_engine::LoopError::Graph(_)));
}
