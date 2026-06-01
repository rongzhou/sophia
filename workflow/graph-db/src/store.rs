//! GraphStore：SQLite + 事件溯源持久化，强制 append-only 不变量。
//!
//! 见 docs/engineering_architecture.md 第六节、docs/workflow_graph_spec.md 第三节。
//! 工程层后果（强制项）：
//! - `update_node` 不暴露 payload 写权限（本类型根本不提供 update_node）；
//! - `append_edge` 写入前校验 `(from.role, to.role, type)`（I3）；
//! - `append_node` 写入前校验 `(role, provenance)`（I2）与 `creation_status`（I8）；
//! - `supersedes` 校验链不成环、两端 role 相同（I4）；
//! - 悬空引用拒绝（I5）；
//! - LLM-provenance 节点必须配套 `consumed→ ContextSnapshot` 边（I6，见 `validate_i6`）。
//!
//! 节点 / 边一旦写入即只读（N1 / N2 / I9）：本类型不提供任何修改 / 删除 API。

use crate::edge::{Edge, EdgeKind};
use crate::error::{GraphError, GraphResult};
use crate::event::GraphEvent;
use crate::ids::{NodeCreationStatus, NodeId, NodeRole, Provenance};
use crate::meta::NodeMeta;
use crate::payload::{ClarificationKind, NodePayload};
use rusqlite::Connection;
use std::collections::BTreeMap;

/// 一个已存储节点（meta + payload），只读。
#[derive(Debug, Clone)]
pub struct StoredNode {
    pub meta: NodeMeta,
    pub payload: NodePayload,
}

/// Development Graph 持久化存储。
pub struct GraphStore {
    conn: Connection,
    /// 内存物化视图：节点（按 ID）。replay / append 时同步维护，加速查询。
    nodes: BTreeMap<NodeId, StoredNode>,
    /// 内存物化视图：边集合（按写入顺序）。
    edges: Vec<Edge>,
    /// 下一个分配的 NodeId 序号。
    next_id: u32,
}

impl GraphStore {
    /// 打开内存库（测试 / 临时）。
    pub fn open_in_memory() -> GraphResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    /// 打开文件库（本地开发，零配置单文件）。
    pub fn open(path: &std::path::Path) -> GraphResult<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> GraphResult<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS graph_events (
                seq     INTEGER PRIMARY KEY AUTOINCREMENT,
                payload TEXT NOT NULL
            );",
        )?;
        let mut store = GraphStore {
            conn,
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            next_id: 1,
        };
        store.replay()?;
        Ok(store)
    }

    /// 从事件流重建内存视图（事件溯源 replay）。
    fn replay(&mut self) -> GraphResult<()> {
        let rows: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT payload FROM graph_events ORDER BY seq ASC")?;
            let mapped = stmt.query_map([], |row| row.get::<_, String>(0))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        for raw in rows {
            let event: GraphEvent = serde_json::from_str(&raw)?;
            self.apply_in_memory(event);
        }
        Ok(())
    }

    /// 把事件应用到内存视图（不写库；写库由 append 持久化后调用）。
    fn apply_in_memory(&mut self, event: GraphEvent) {
        match event {
            GraphEvent::NodeCreated { meta, payload } => {
                let id = meta.id;
                if id.0 >= self.next_id {
                    self.next_id = id.0 + 1;
                }
                self.nodes.insert(
                    id,
                    StoredNode {
                        meta: *meta,
                        payload: *payload,
                    },
                );
            }
            GraphEvent::EdgeAdded { edge } => self.edges.push(edge),
        }
    }

    /// 持久化一个事件到 SQLite。
    fn persist(&self, event: &GraphEvent) -> GraphResult<()> {
        let raw = serde_json::to_string(event)?;
        self.conn
            .execute("INSERT INTO graph_events (payload) VALUES (?1)", [raw])?;
        Ok(())
    }

    // ---- 节点 ----

    /// 追加一个节点（**crate 内部原语**）。强制 I1 / I2 / I8 与 summary 非空、
    /// payload 字段约束。ID 由 store 分配。
    ///
    /// N6（provenance 由创建路径强制）：本方法不对外公开，外部只能经
    /// [`crate::factory`] 的 provenance 分组工厂入口创建节点，使得调用方无法
    /// 自由伪造 provenance。本方法仍校验 `(role, provenance)` 矩阵作为最低保障。
    pub(crate) fn append_node(
        &mut self,
        role: NodeRole,
        provenance: Provenance,
        creation_status: NodeCreationStatus,
        summary: impl Into<String>,
        payload: NodePayload,
    ) -> GraphResult<NodeId> {
        // role 与 payload 必须一致。
        if payload.role() != role {
            return Err(GraphError::InvalidPayload(format!(
                "payload 对应 role {:?} 与声明 role {:?} 不一致",
                payload.role(),
                role
            )));
        }
        // I2：(role, provenance) 矩阵。
        if !provenance.allowed_for(role) {
            return Err(GraphError::ProvenanceNotAllowed { role, provenance });
        }
        // Clarification 的 kind↔provenance 精确约束。
        if let NodePayload::Clarification(c) = &payload {
            let expect = match c.kind {
                ClarificationKind::Question => Provenance::Llm,
                ClarificationKind::Answer => Provenance::Human,
            };
            if provenance != expect {
                return Err(GraphError::ProvenanceNotAllowed { role, provenance });
            }
        }
        // I8：Failed 仅 RawLlm。
        match (role, creation_status) {
            (NodeRole::RawLlm, NodeCreationStatus::Failed) => {}
            (_, NodeCreationStatus::Failed) => {
                return Err(GraphError::InvalidFailedStatus { role });
            }
            (_, NodeCreationStatus::Ok) => {}
        }
        // RawLlm 必须 Failed。
        if role == NodeRole::RawLlm && creation_status != NodeCreationStatus::Failed {
            return Err(GraphError::InvalidPayload(
                "RawLlm 节点的 creation_status 必须为 Failed".into(),
            ));
        }

        let summary = summary.into();
        if summary.trim().is_empty() {
            return Err(GraphError::EmptySummary);
        }
        validate_payload(&payload)?;

        let id = NodeId(self.next_id);
        let meta = NodeMeta {
            id,
            role,
            provenance,
            creation_status,
            created_at: chrono::Utc::now(),
            summary,
            tags: Vec::new(),
            model: None,
            prompt_artifact: None,
            response_artifact: None,
        };
        let event = GraphEvent::NodeCreated {
            meta: Box::new(meta),
            payload: Box::new(payload),
        };
        self.persist(&event)?;
        self.apply_in_memory(event);
        Ok(id)
    }

    /// 读取节点（只读）。
    pub fn node(&self, id: NodeId) -> Option<&StoredNode> {
        self.nodes.get(&id)
    }

    /// 节点 role（便捷）。
    pub fn role_of(&self, id: NodeId) -> Option<NodeRole> {
        self.nodes.get(&id).map(|n| n.meta.role)
    }

    /// 全部节点（按 ID 升序）。
    pub fn nodes(&self) -> impl Iterator<Item = &StoredNode> {
        self.nodes.values()
    }

    /// 全部边（写入顺序）。
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// 某节点的创建时间。
    pub fn created_at(&self, id: NodeId) -> Option<chrono::DateTime<chrono::Utc>> {
        self.nodes.get(&id).map(|n| n.meta.created_at)
    }

    /// 某节点的 provenance。
    pub fn provenance_of(&self, id: NodeId) -> Option<Provenance> {
        self.nodes.get(&id).map(|n| n.meta.provenance)
    }

    /// 遍历满足某 `(kind)` 的边（按写入顺序）。
    pub fn edges_of_kind(&self, kind: EdgeKind) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.kind == kind)
    }

    /// 只读审计：按 `seq` 升序返回 append-only 事件日志的**原始序列化记录**。
    ///
    /// 用于 CI / 审计层校验 append-only 不变量（N1 / N2 / I9）：每条记录是一行不透明的
    /// 序列化 payload（内部 `GraphEvent` 表示不外泄），调用方只比对其作为字节序列的前缀
    /// 稳定性——任何写操作只应在**末尾追加**记录，绝不重写 / 删除既有记录。
    pub fn raw_event_log(&self) -> GraphResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM graph_events ORDER BY seq ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 是否存在指定 `(from, to, kind)` 的边。
    pub fn has_edge(&self, from: NodeId, to: NodeId, kind: EdgeKind) -> bool {
        self.edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == kind)
    }

    // ---- 边 ----

    /// 追加一条边。写入前校验：
    /// - I5：两端节点存在；
    /// - I3：`(from.role, to.role, kind)` 在允许集合中；
    /// - I4：`supersedes` 两端同 role、不成环、单出边。
    pub fn append_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> GraphResult<()> {
        let from_role = self
            .role_of(from)
            .ok_or(GraphError::DanglingReference(from))?;
        let to_role = self.role_of(to).ok_or(GraphError::DanglingReference(to))?;

        if !kind.allows(from_role, to_role) {
            return Err(GraphError::InvalidEdge {
                edge: kind.name(),
                from_role,
                to_role,
            });
        }

        // payload 级边约束（kind 依赖 payload，无法在 EdgeKind::allows 的 role 层表达）。
        self.validate_edge_payload(from, to, kind)?;

        if kind == EdgeKind::Supersedes {
            self.validate_supersedes(from, to)?;
        }

        let edge = Edge { from, to, kind };
        let event = GraphEvent::EdgeAdded { edge };
        self.persist(&event)?;
        self.apply_in_memory(event);
        Ok(())
    }

    /// payload 级边约束（第六节 6.1）：依赖节点 payload 而非仅 role 的约束。
    /// - `answers`：from 必须是 `Clarification(Answer)`，to 必须是 `Clarification(Question)`；
    /// - `asks_about`：from 必须是 `Clarification(Question)`；
    /// - `requires`：to 必须是 `Constraint(kind=Invariant)`；
    /// - `excludes`：to 必须是 `Constraint(kind=OutOfScope)`。
    fn validate_edge_payload(&self, from: NodeId, to: NodeId, kind: EdgeKind) -> GraphResult<()> {
        use crate::payload::{ConstraintKind, NodePayload as P};
        let from_payload = self.node(from).map(|n| &n.payload);
        let to_payload = self.node(to).map(|n| &n.payload);

        match kind {
            EdgeKind::Answers => {
                let from_is_answer = matches!(
                    from_payload,
                    Some(P::Clarification(c)) if c.kind == ClarificationKind::Answer
                );
                let to_is_question = matches!(
                    to_payload,
                    Some(P::Clarification(c)) if c.kind == ClarificationKind::Question
                );
                if !from_is_answer || !to_is_question {
                    return Err(GraphError::InvalidPayload(
                        "answers 必须从 Clarification(Answer) 指向 Clarification(Question)".into(),
                    ));
                }
            }
            EdgeKind::AsksAbout => {
                let from_is_question = matches!(
                    from_payload,
                    Some(P::Clarification(c)) if c.kind == ClarificationKind::Question
                );
                if !from_is_question {
                    return Err(GraphError::InvalidPayload(
                        "asks_about 必须从 Clarification(Question) 发出".into(),
                    ));
                }
            }
            EdgeKind::Requires => {
                let to_is_invariant = matches!(
                    to_payload,
                    Some(P::Constraint(c)) if c.kind == ConstraintKind::Invariant
                );
                if !to_is_invariant {
                    return Err(GraphError::InvalidPayload(
                        "requires 的目标必须是 Constraint(kind=Invariant)".into(),
                    ));
                }
            }
            EdgeKind::Excludes => {
                let to_is_oos = matches!(
                    to_payload,
                    Some(P::Constraint(c)) if c.kind == ConstraintKind::OutOfScope
                );
                if !to_is_oos {
                    return Err(GraphError::InvalidPayload(
                        "excludes 的目标必须是 Constraint(kind=OutOfScope)".into(),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// 校验 supersedes 的 I4：两端 role 相同（已由 allows 保证）、单出边、不成环。
    fn validate_supersedes(&self, from: NodeId, to: NodeId) -> GraphResult<()> {
        // 一个节点最多一条出向 supersedes 边。
        if self
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Supersedes && e.from == from)
        {
            return Err(GraphError::InvalidSupersedes(format!(
                "{from} 已有一条 supersedes 出边"
            )));
        }
        // 不成环：从 `to` 沿 supersedes 链前进，不得回到 `from`。
        let mut cur = to;
        let mut guard = 0;
        loop {
            if cur == from {
                return Err(GraphError::InvalidSupersedes(format!(
                    "supersedes {from} → {to} 会成环"
                )));
            }
            let Some(next) = self
                .edges
                .iter()
                .find(|e| e.kind == EdgeKind::Supersedes && e.from == cur)
                .map(|e| e.to)
            else {
                break;
            };
            cur = next;
            guard += 1;
            if guard > self.nodes.len() + 1 {
                return Err(GraphError::InvalidSupersedes("supersedes 链异常".into()));
            }
        }
        Ok(())
    }

    /// I6 守护：每个 LLM-provenance 的 Decision / Pseudocode / Code / Assessment /
    /// Decomposition 节点必须存在一条 `consumed→ ContextSnapshot` 边。
    ///
    /// 作为图整体不变量检查（CI / 收尾校验调用），不在单条 append 中强制——因为
    /// 节点先创建、`consumed` 边后补是合法的写入顺序。
    pub fn validate_i6(&self) -> GraphResult<()> {
        for node in self.nodes.values() {
            let needs = matches!(
                node.meta.role,
                NodeRole::Decision
                    | NodeRole::Pseudocode
                    | NodeRole::Code
                    | NodeRole::Assessment
                    | NodeRole::Decomposition
            ) && node.meta.provenance == Provenance::Llm;
            if needs {
                let has = self.edges.iter().any(|e| {
                    e.kind == EdgeKind::Consumed
                        && e.from == node.meta.id
                        && self.role_of(e.to) == Some(NodeRole::ContextSnapshot)
                });
                if !has {
                    return Err(GraphError::InvalidPayload(format!(
                        "{} 是 LLM-provenance {:?}，但缺少 consumed→ ContextSnapshot 边（I6）",
                        node.meta.id, node.meta.role
                    )));
                }
            }
        }
        Ok(())
    }
}

/// payload 字段级约束（非空字符串、非空列表等）。
fn validate_payload(payload: &NodePayload) -> GraphResult<()> {
    use NodePayload as P;
    let nonempty = |s: &str, what: &str| -> GraphResult<()> {
        if s.trim().is_empty() {
            Err(GraphError::InvalidPayload(format!("{what} 不能为空")))
        } else {
            Ok(())
        }
    };
    match payload {
        P::Objective(o) => {
            nonempty(&o.title, "title")?;
            nonempty(&o.description, "description")?;
        }
        P::Constraint(c) => nonempty(&c.statement, "statement")?,
        P::AcceptanceCriterion(a) => nonempty(&a.statement, "statement")?,
        P::Decomposition(d) => nonempty(&d.rationale, "rationale")?,
        P::Milestone(m) => {
            nonempty(&m.name, "name")?;
            nonempty(&m.summary, "summary")?;
        }
        P::ChangeRequest(c) => nonempty(&c.request, "request")?,
        P::FirstSlice(f) => nonempty(&f.purpose, "purpose")?,
        P::Withdrawal(w) => nonempty(&w.reason, "reason")?,
        P::Clarification(c) => nonempty(&c.body, "body")?,
        P::ContextSnapshot(s) => {
            // digest 必须是 64 位 lower-case hex。
            if s.digest.len() != 64
                || !s
                    .digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(GraphError::InvalidPayload(
                    "ContextSnapshot.digest 必须为 64 位小写 hex".into(),
                ));
            }
        }
        P::Decision(d) => {
            nonempty(&d.rationale, "rationale")?;
            if !(0.0..=1.0).contains(&d.confidence) {
                return Err(GraphError::InvalidPayload(
                    "Decision.confidence 必须在 [0,1]".into(),
                ));
            }
        }
        P::Pseudocode(p) => {
            nonempty(&p.purpose, "purpose")?;
            if p.artifact_path != "content.pseudo" {
                return Err(GraphError::InvalidPayload(
                    "Pseudocode.artifact_path 必须为 \"content.pseudo\"".into(),
                ));
            }
        }
        P::Code(c) => {
            if c.files.is_empty() {
                return Err(GraphError::InvalidPayload("Code.files 不能为空".into()));
            }
        }
        P::Diagnostic(d) => {
            for item in &d.diagnostics {
                nonempty(&item.code, "diagnostic.code")?;
                nonempty(&item.problem, "diagnostic.problem")?;
            }
        }
        P::Selection(s) => nonempty(&s.rationale, "rationale")?,
        P::Materialize(m) => {
            nonempty(&m.target_root, "target_root")?;
            if m.files.is_empty() {
                return Err(GraphError::InvalidPayload(
                    "Materialize.files 不能为空".into(),
                ));
            }
        }
        P::RawLlm(r) => {
            nonempty(&r.operation, "operation")?;
            nonempty(&r.error_summary, "error_summary")?;
        }
        // verifier.ref 非空。
        P::Assessment(_) | P::Acceptance(_) | P::Activation(_) => {}
    }
    // verifier ref 非空（Constraint / AcceptanceCriterion）。
    let verifier_ref_ok = |v: &Option<crate::payload::Verifier>| -> GraphResult<()> {
        if let Some(ver) = v {
            if ver.r#ref.trim().is_empty() {
                return Err(GraphError::InvalidPayload("verifier.ref 不能为空".into()));
            }
        }
        Ok(())
    };
    match payload {
        P::Constraint(c) => verifier_ref_ok(&c.verifier)?,
        P::AcceptanceCriterion(a) => verifier_ref_ok(&a.verifier)?,
        _ => {}
    }
    Ok(())
}
