//! append-only / I9 不变量的 CI 守护测试。
//!
//! 见 docs/workflow_graph_spec.md（N1 节点不可变 / N2 边不可变 / I9 事件日志 append-only）、
//! docs/engineering_architecture.md 第六节（事件溯源）。
//!
//! Development Graph 的核心安全属性是**事件日志只增不改**：任何写操作（建节点 / 加边）
//! 只能在日志**末尾追加**新记录，绝不重写或删除既有记录。这是 provenance 可信、上下文
//! 可复现（I10）与 anti-cheat 的物理基础。本测试在确定性管线（`cargo test`，进 CI）中
//! 守护该属性——区别于真实 LLM e2e（不进 CI）。
//!
//! 守护手段：`GraphStore::raw_event_log` 返回 append-only 日志的原始序列化记录序列；
//! 我们断言每次写操作后，**旧日志是新日志的严格前缀**（既有记录逐字节不变），且仅在末尾
//! 增长。store 本身不提供任何 update / delete API（N1 / N2 由类型层面封死），本测试补上
//! 日志层面的逐字节前缀稳定性证据。

use sophia_graph_db::*;

fn obj_payload(title: &str) -> ObjectivePayload {
    ObjectivePayload {
        title: title.into(),
        description: "desc".into(),
    }
}

/// 旧日志必须是新日志的严格前缀（既有记录逐字节不变，新增只在末尾）。
fn assert_append_only(before: &[String], after: &[String]) {
    assert!(
        after.len() >= before.len(),
        "事件日志长度不应缩短：{} → {}",
        before.len(),
        after.len()
    );
    for (i, (old, new)) in before.iter().zip(after).enumerate() {
        assert_eq!(
            old, new,
            "第 {i} 条既有事件记录被改写（违反 append-only / I9）"
        );
    }
}

#[test]
fn each_write_only_appends_to_event_log() {
    let mut store = GraphStore::open_in_memory().unwrap();

    // 初始为空。
    let mut log = store.raw_event_log().unwrap();
    assert!(log.is_empty(), "新库事件日志应为空");

    // 建第一个节点：日志增长 1 条。
    let o1 = store.as_human().objective("o1", obj_payload("O1")).unwrap();
    let after = store.raw_event_log().unwrap();
    assert_append_only(&log, &after);
    assert_eq!(after.len(), 1, "建一个节点应追加恰好 1 条事件");
    log = after;

    // 建第二个节点。
    let o2 = store.as_human().objective("o2", obj_payload("O2")).unwrap();
    let after = store.raw_event_log().unwrap();
    assert_append_only(&log, &after);
    assert_eq!(after.len(), 2);
    log = after;

    // 加一条边：同样只在末尾追加。
    let crit = store
        .as_human()
        .acceptance_criterion(
            "c",
            AcceptanceCriterionPayload {
                statement: "must".into(),
                verifier: None,
            },
        )
        .unwrap();
    let after = store.raw_event_log().unwrap();
    assert_append_only(&log, &after);
    log = after;

    store.append_edge(o1, crit, EdgeKind::ValidatedBy).unwrap();
    let after = store.raw_event_log().unwrap();
    assert_append_only(&log, &after);
    log = after;

    // 即便写操作失败（非法边），也不得改动既有日志（拒绝即无副作用）。
    let err = store.append_edge(o1, o2, EdgeKind::Selects);
    assert!(err.is_err(), "Objective→Objective 的 selects 应被拒");
    let after = store.raw_event_log().unwrap();
    assert_eq!(after, log, "被拒绝的写不应在日志留下任何记录");
}

#[test]
fn reopened_store_preserves_and_extends_log_verbatim() {
    // 跨进程（重开库）：replay 后既有日志逐字节不变，新写仍只在末尾追加。
    let dir = std::env::temp_dir().join(format!("sophia_append_only_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("graph.sqlite");

    let log_after_first = {
        let mut store = GraphStore::open(&db).unwrap();
        let o = store.as_human().objective("o", obj_payload("O")).unwrap();
        let c = store
            .as_human()
            .constraint(
                "c",
                ConstraintPayload {
                    kind: ConstraintKind::Invariant,
                    statement: "keep".into(),
                    verifier: None,
                },
            )
            .unwrap();
        store.append_edge(o, c, EdgeKind::ConstrainedBy).unwrap();
        store.raw_event_log().unwrap()
    };

    // 重开：日志逐字节保持（replay 不改写历史）。
    let mut store = GraphStore::open(&db).unwrap();
    let log_on_reopen = store.raw_event_log().unwrap();
    assert_eq!(
        log_on_reopen, log_after_first,
        "重开库后既有事件日志应逐字节保持（append-only / 跨进程持久化）"
    );

    // 继续写：仍只在末尾追加，旧记录不变。
    store.as_human().objective("o3", obj_payload("O3")).unwrap();
    let log_after_more = store.raw_event_log().unwrap();
    assert_append_only(&log_on_reopen, &log_after_more);
    assert_eq!(log_after_more.len(), log_on_reopen.len() + 1);

    std::fs::remove_dir_all(&dir).ok();
}
