//! `graph select` / `materialize` 与其 gate 重跑（design 10.10 / workflow_graph_spec 五A）。
//!
//! 类型态门禁证明无法跨进程持久化，故 select / materialize 各自**重跑** materialize gate
//! （对不可逆写盘是更稳妥的姿态）：code_check → constraint_audit（含 hidden case 真实执行）→
//! artifact_diff + runtime validation，喂入 `tools/materialize` 的类型状态链；任一未过即 emit
//! 对应 DiagnosticNode 并阻断（忠实反映，不伪造成功）。
//!
//! 与 `graph_cmd` 主模块（确定性命令 + design/implement LLM 命令）拆分：本模块只负责
//! 候选选中与物化的 gate 重跑，共享 helper（`open_store` / `parse_node` / `artifacts_dir` /
//! `write_code_artifacts`）由父模块以 `pub(super)` 提供。

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use sophia_engine::{code_check, domain_of_path, run_materialization, run_selection};
use sophia_graph_db::{
    derive_active_context, ConstraintView, DiagnosticItem, DiagnosticKind, DiagnosticPayload,
    DiagnosticSeverity, GraphStore, NodeId, NodePayload, NodeRole,
};
use sophia_materialize::{CodeCandidate, GateReport, Selected};

use super::{artifacts_dir, open_store, parse_node};

/// `sophia graph select <CodeId>`：重跑 gate 并选中候选（建 SelectionNode）。
pub fn select(root: &Path, node: &str, rationale: &str) -> Result<ExitCode> {
    let code = parse_node(node)?;
    let mut store = open_store(root)?;
    if store.role_of(code) != Some(NodeRole::Code) {
        anyhow::bail!("{code} 不是 Code 节点");
    }

    let candidate = match prepare_selected_candidate(root, &mut store, code)? {
        GateResult::Passed(c) => c,
        GateResult::Blocked(code_exit) => return Ok(code_exit),
    };

    let selection = run_selection(&mut store, &candidate, code, rationale).context("选中失败")?;
    println!(
        "已选中 {code} → {selection}（selects→）。运行 `graph materialize {selection}` 写入 domains/。"
    );
    Ok(ExitCode::SUCCESS)
}

/// `sophia graph materialize <SelectionId>`：重跑 gate 并经 staging/rename 物化到 `domains/`。
pub fn materialize(root: &Path, node: &str) -> Result<ExitCode> {
    let selection = parse_node(node)?;
    let mut store = open_store(root)?;
    if store.role_of(selection) != Some(NodeRole::Selection) {
        anyhow::bail!("{selection} 不是 Selection 节点");
    }

    // 找到 selects→ 的 Code 节点。
    let code = store
        .edges()
        .iter()
        .find(|e| e.kind == sophia_graph_db::EdgeKind::Selects && e.from == selection)
        .map(|e| e.to)
        .with_context(|| format!("{selection} 没有 selects→ Code 边"))?;

    let candidate = match prepare_selected_candidate(root, &mut store, code)? {
        GateResult::Passed(c) => c,
        GateResult::Blocked(code_exit) => return Ok(code_exit),
    };

    let write_root = root.join("domains");
    let (mat, written) =
        run_materialization(&mut store, candidate, selection, &write_root, "domains")
            .context("物化失败")?;
    println!(
        "已物化 {selection} → {mat}（materializes→），写入 {} 个文件到 domains/：",
        written.files.len()
    );
    for f in &written.files {
        println!("    {f}");
    }
    Ok(ExitCode::SUCCESS)
}

/// 从 artifacts 加载候选并重跑 gate（select / materialize 共用前置）。
fn prepare_selected_candidate(
    root: &Path,
    store: &mut GraphStore,
    code: NodeId,
) -> Result<GateResult> {
    let files = load_candidate_files(root, store, code)?;
    build_selected_candidate(root, store, code, &files)
}

/// gate 重跑结果：通过则给出 `Selected` 证明，否则给出失败退出码（诊断已 emit）。
enum GateResult {
    Passed(CodeCandidate<Selected>),
    Blocked(ExitCode),
}

/// 重跑 materialize gate（design 10.10），构造 `Selected` 证明或阻断。
///
/// 类型态证明无法跨进程持久化，故 select / materialize 各自重跑门禁——对不可逆写盘是
/// 更稳妥的姿态。各 gate 由确定性管线产出 `GateReport` 喂入 `tools/materialize` 的类型
/// 状态链；任一未过即 emit 对应 `DiagnosticNode` 并阻断（不伪造成功）。
///
/// - **code_check**：桥接 `tools/check`（语法 + HIR + 语义三层）；
/// - **constraint_audit**：对 active context 的 bound invariant 跑 `tools/audit`；带 `HiddenCase`
///   verifier 的 invariant 由 `runtime` 在候选上真正执行 hidden case 驱动 gate（见
///   [`run_constraint_audit`]）；声明 verifier 却缺用例 → 硬错误阻断（忠实反映，不伪造）；
/// - **artifact_diff**：strip-assist 等价（取自 check 报告）；
/// - **runtime validation**：input/output 结构校验由 hidden-case 执行覆盖；无待执行项则通过。
fn build_selected_candidate(
    root: &Path,
    store: &mut GraphStore,
    code: NodeId,
    files: &[(String, String)],
) -> Result<GateResult> {
    // gate 1：code_check。
    let code_check = code_check(files);
    let code_check_pass = code_check.ok;
    let code_check_detail = diag_summary(&code_check);
    let strip_pass = !code_check
        .diagnostics
        .iter()
        .any(|d| d.code == "STRIP-ASSIST");
    emit_diagnostic(store, code, code_check)?;

    let unchecked = CodeCandidate::new(files.to_vec());
    let checked = match unchecked.run_check(&report(code_check_pass, &code_check_detail)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("code_check gate 未通过：{e}");
            return Ok(GateResult::Blocked(ExitCode::FAILURE));
        }
    };

    // gate 2：constraint_audit（bound invariants，hidden case 由 runtime 在候选上执行驱动）。
    let audit = run_constraint_audit(root, store, code, files)?;
    let audited = match checked.run_audit(&audit) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("constraint_audit gate 未通过：{e}");
            return Ok(GateResult::Blocked(ExitCode::FAILURE));
        }
    };

    // gate 3：artifact_diff（strip-assist）+ runtime validation。
    let artifact_diff = report(strip_pass, "strip-assist 等价");
    // 起步阶段 runtime validation：无 hidden case 待跑（audit 已阻断有 verifier 的情形）。
    let runtime = GateReport::pass();
    let validated = match audited.run_runtime_validation(&artifact_diff, &runtime) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("artifact_diff / runtime validation gate 未通过：{e}");
            return Ok(GateResult::Blocked(ExitCode::FAILURE));
        }
    };

    Ok(GateResult::Passed(validated.select()))
}

/// 对 active context 的 bound invariant 跑 constraint audit，产出 gate 报告。
///
/// 流程（design 10.10 / workflow_graph_spec 五A）：
/// 1. 加载隐藏验证用例存储 `sophia-runs/verifiers/hidden.json`（缺文件 = 空存储）；
/// 2. 取 bound constraint，从**图节点原始 payload** 读 `verifier`（`ConstraintView` 刻意不含
///    verifier，故不能用它）；投影为 `sophia_audit::Constraint`；
/// 3. 对带 `HiddenCase` verifier 的 invariant：按 `ref` 取用例 → `runtime::run_hidden_case`
///    在**候选**上真正执行 → 映射 `VerifierOutcome` 注入；缺用例则不注入（audit 侧据此硬错误）；
/// 4. `audit_constraints` 判定 → emit DiagnosticNode；任一 invariant fail 即阻断（不伪造）。
///
/// 声明了可执行 verifier 却缺用例 / 缺运行器 → `MissingVerifierOutcome` 硬错误，gate 阻断
/// （忠实反映，不伪造通过）。
fn run_constraint_audit(
    root: &Path,
    store: &mut GraphStore,
    code: NodeId,
    files: &[(String, String)],
) -> Result<GateReport> {
    let hidden = crate::verifier_store::HiddenVerifierStore::load(root)?;
    let ctx = derive_active_context(store);

    // 从图节点原始 payload 读 verifier（不经 ConstraintView——它不投影 verifier）。
    let constraints: Vec<sophia_audit::Constraint> = ctx
        .bound_constraints
        .iter()
        .map(|view| constraint_to_audit(store, view))
        .collect();

    // 对带 HiddenCase verifier 的 invariant，在候选上执行 hidden case → 注入 outcome。
    let outcomes = run_hidden_verifiers(&constraints, &hidden, files);

    let audit_report = match sophia_audit::audit_constraints(&constraints, &outcomes) {
        Ok(r) => r,
        Err(e) => {
            // 声明了可执行 verifier 却无结果 → 阻断 gate，并 emit RegressionGate 诊断。
            let payload = DiagnosticPayload {
                kind: DiagnosticKind::RegressionGate,
                ok: false,
                diagnostics: vec![DiagnosticItem {
                    code: "REGRESSION-GATE".to_string(),
                    severity: DiagnosticSeverity::Error,
                    problem: e.to_string(),
                    location: None,
                }],
            };
            emit_diagnostic(store, code, payload)?;
            return Ok(GateReport::fail(e.to_string()));
        }
    };

    let ok = audit_report.ok();
    let items: Vec<DiagnosticItem> = audit_report
        .failures()
        .map(|f| DiagnosticItem {
            code: "REGRESSION-GATE".to_string(),
            severity: DiagnosticSeverity::Error,
            problem: format!("{:?}", f.verdict),
            location: Some(f.constraint_id.clone()),
        })
        .collect();
    let detail = if ok {
        "constraint audit 通过".to_string()
    } else {
        format!("{} 条 invariant 失败", items.len())
    };
    emit_diagnostic(
        store,
        code,
        DiagnosticPayload {
            kind: DiagnosticKind::ConstraintAudit,
            ok,
            diagnostics: items,
        },
    )?;
    Ok(report(ok, detail))
}

/// 把一条 bound constraint 投影为 audit 层的 `Constraint`，**从图节点原始 payload 读 verifier**。
///
/// `ConstraintView`（active context）刻意不含 verifier（anti-cheat：不投影给 LLM），故这里
/// 用 view 的 id 回查 ConstraintNode 原始 payload 取 `verifier`——gate 侧需要它来定位 hidden case，
/// 但它从不进 active context / snapshot。
fn constraint_to_audit(store: &GraphStore, view: &ConstraintView) -> sophia_audit::Constraint {
    use sophia_graph_db::ConstraintKind as GK;
    let kind = match view.kind {
        GK::Invariant => sophia_audit::ConstraintKind::Invariant,
        GK::OutOfScope => sophia_audit::ConstraintKind::OutOfScope,
        GK::Preference => sophia_audit::ConstraintKind::Preference,
        GK::Forbidden => sophia_audit::ConstraintKind::Forbidden,
    };
    // 从原始节点 payload 读 verifier（view 不含）。
    let verifier = match store.node(view.id).map(|n| &n.payload) {
        Some(NodePayload::Constraint(c)) => c.verifier.as_ref().map(|v| {
            use sophia_graph_db::VerifierKind as VK;
            let vk = match v.kind {
                VK::HiddenCase => sophia_audit::VerifierKind::HiddenCase,
                VK::AuditRule => sophia_audit::VerifierKind::AuditRule,
                VK::Manual => sophia_audit::VerifierKind::Manual,
            };
            (vk, v.r#ref.clone())
        }),
        _ => None,
    };
    sophia_audit::Constraint {
        id: view.id.as_string(),
        kind,
        statement: view.statement.clone(),
        verifier,
    }
}

/// 对带 `HiddenCase` verifier 的 invariant / forbidden，在**候选**上执行 hidden case，产出注入审计的 outcomes。
///
/// 分层（architecture §3.3）：执行属 `runtime`（`run_hidden_case`），判定属 `tools/audit`；本协调层
/// 加载候选 → 构建模型 → 执行 → 零损耗映射 `VerificationResult` 为 `VerifierOutcome`。缺用例则
/// **不注入**对应 outcome——`audit_constraints` 据此触发 `MissingVerifierOutcome` 硬错误阻断
/// （不伪造通过）。候选模型构建失败（理论上已过 code_check，不应发生）同样不注入，交由 audit 阻断。
fn run_hidden_verifiers(
    constraints: &[sophia_audit::Constraint],
    hidden: &crate::verifier_store::HiddenVerifierStore,
    files: &[(String, String)],
) -> Vec<sophia_audit::VerifierOutcome> {
    // 收集需要执行的 hidden case（仅 Invariant / Forbidden + HiddenCase verifier 且存储里有用例）。
    let cases: Vec<sophia_runtime::HiddenCase> = constraints
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                sophia_audit::ConstraintKind::Invariant | sophia_audit::ConstraintKind::Forbidden
            )
        })
        .filter_map(|c| match &c.verifier {
            Some((sophia_audit::VerifierKind::HiddenCase, vref)) => hidden.get(vref).cloned(),
            _ => None,
        })
        .collect();
    if cases.is_empty() {
        return Vec::new();
    }
    // 构建候选的语义模型（执行 hidden case 需要）。失败则返回空 outcomes（audit 侧阻断）。
    let asts: Vec<sophia_syntax::Ast> = match files
        .iter()
        .map(|(_, src)| sophia_syntax::parse_ast(src.as_str()))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    let domains: Vec<String> = files.iter().map(|(path, _)| domain_of_path(path)).collect();
    let inputs: Vec<sophia_hir::ProgramInput> = files
        .iter()
        .zip(&asts)
        .zip(&domains)
        .map(|(((path, _), ast), domain)| sophia_hir::ProgramInput { domain, path, ast })
        .collect();
    let Ok((index, _)) = sophia_hir::resolve_program(&inputs, &sophia_stdlib::standard_registry())
    else {
        return Vec::new();
    };
    let ast_refs: Vec<&sophia_syntax::Ast> = asts.iter().collect();
    let model = sophia_semantic::analyze_program(&ast_refs, &index).model;

    // 执行每个 hidden case，映射为 VerifierOutcome。
    sophia_runtime::run_hidden_cases(&model, &ast_refs, &cases)
        .into_iter()
        .map(|r| sophia_audit::VerifierOutcome {
            verifier_ref: r.verifier_ref,
            passed: r.passed,
            detail: r.detail,
        })
        .collect()
}

/// emit 一个确定性 DiagnosticNode 并连 `checks→ code`。
fn emit_diagnostic(store: &mut GraphStore, code: NodeId, payload: DiagnosticPayload) -> Result<()> {
    let kind = payload.kind;
    let id = store
        .as_deterministic()
        .diagnostic(format!("{kind:?}"), payload)
        .with_context(|| format!("创建 {kind:?} DiagnosticNode 失败"))?;
    store
        .append_edge(id, code, sophia_graph_db::EdgeKind::Checks)
        .context("连接 checks→ 边失败")?;
    Ok(())
}

/// 由 pass + 说明构造 GateReport。
fn report(pass: bool, detail: impl Into<String>) -> GateReport {
    if pass {
        GateReport::pass()
    } else {
        GateReport::fail(detail.into())
    }
}

/// 汇总诊断为单行说明。
fn diag_summary(p: &DiagnosticPayload) -> String {
    if p.ok {
        "通过".to_string()
    } else {
        p.diagnostics
            .iter()
            .map(|d| format!("[{}] {}", d.code, d.problem))
            .collect::<Vec<_>>()
            .join("；")
    }
}

/// 从 artifacts 目录加载候选 Code 节点的文件（path → content）。
fn load_candidate_files(
    root: &Path,
    store: &GraphStore,
    code: NodeId,
) -> Result<Vec<(String, String)>> {
    let paths = match store.node(code).map(|n| &n.payload) {
        Some(NodePayload::Code(c)) => c.files.clone(),
        _ => anyhow::bail!("{code} 不是 Code 节点或不存在"),
    };
    let base = artifacts_dir(root).join(code.as_string());
    let mut files = Vec::new();
    for rel in paths {
        let path = base.join(&rel);
        let content = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "读取候选文件 {} 失败（先运行 `graph implement-loop`？）",
                path.display()
            )
        })?;
        files.push((rel, content));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sophia_graph_db::ObjectivePayload;
    use std::path::PathBuf;

    use super::super::write_code_artifacts;

    /// 唯一临时项目目录。
    fn temp_root(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sophia_sm_cli_{tag}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 在图中建一个 Code 节点（带 consumed→ snapshot 满足 I6），并把候选文件落盘 artifacts。
    fn seed_code(root: &Path, files: &[(&str, &str)]) -> NodeId {
        let mut store = open_store(root).unwrap();
        let ctx = derive_active_context(&store);
        let snap = store
            .as_deterministic()
            .context_snapshot("snap", sophia_graph_db::snapshot_payload(&ctx))
            .unwrap();
        let paths: Vec<String> = files.iter().map(|(p, _)| p.to_string()).collect();
        let code = store
            .as_llm()
            .code("code", sophia_graph_db::CodePayload { files: paths })
            .unwrap();
        store
            .append_edge(code, snap, sophia_graph_db::EdgeKind::Consumed)
            .unwrap();
        // 候选正文落盘 artifacts/<code>/<rel>。
        let owned: Vec<(String, String)> = files
            .iter()
            .map(|(p, c)| (p.to_string(), c.to_string()))
            .collect();
        write_code_artifacts(root, code, &owned).unwrap();
        code
    }

    #[test]
    fn select_then_materialize_clean_candidate_writes_domains() {
        let root = temp_root("ok");
        // 干净候选（语义通过）。
        let code = seed_code(
            &root,
            &[(
                "MathDomain/actions/AddOne.sophia",
                "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }",
            )],
        );

        // select：重跑 gate 通过 → SelectionNode。
        let sel_exit = select(&root, &code.as_string(), "唯一候选").unwrap();
        assert_eq!(sel_exit, ExitCode::SUCCESS);

        // 找到 SelectionNode。
        let store = open_store(&root).unwrap();
        let selection = store
            .nodes()
            .find(|n| n.meta.role == NodeRole::Selection)
            .map(|n| n.meta.id)
            .expect("应有 SelectionNode");
        drop(store);

        // materialize：重跑 gate → 写入 domains/。
        let mat_exit = materialize(&root, &selection.as_string()).unwrap();
        assert_eq!(mat_exit, ExitCode::SUCCESS);

        let written = root.join("domains/MathDomain/actions/AddOne.sophia");
        assert!(written.exists(), "应写入 domains/");

        // 图中应有 Selection + Materialize + 多个 Diagnostic 节点。
        let store = open_store(&root).unwrap();
        assert!(store.nodes().any(|n| n.meta.role == NodeRole::Materialize));
        assert!(
            store
                .nodes()
                .filter(|n| n.meta.role == NodeRole::Diagnostic)
                .count()
                >= 2
        );
        store.validate_i6().unwrap();

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn select_blocks_on_failing_code_check() {
        let root = temp_root("block");
        // 语义错误候选：print 未声明 Console.Write effect。
        let code = seed_code(
            &root,
            &[(
                "D/actions/Bad.sophia",
                "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }",
            )],
        );

        let exit = select(&root, &code.as_string(), "试图选中坏候选").unwrap();
        assert_eq!(exit, ExitCode::FAILURE, "code_check 失败应阻断 select");

        // 不应产生 SelectionNode；但应有 CodeCheck DiagnosticNode（忠实记录）。
        let store = open_store(&root).unwrap();
        assert!(!store.nodes().any(|n| n.meta.role == NodeRole::Selection));
        assert!(store.nodes().any(|n| n.meta.role == NodeRole::Diagnostic));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn materialize_rejects_non_selection() {
        let root = temp_root("badnode");
        let code = seed_code(
            &root,
            &[("D/actions/A.sophia", "action A { body { return } }")],
        );
        // 把 Code 节点 ID 当 selection → 应硬错误。
        let err = materialize(&root, &code.as_string()).unwrap_err();
        assert!(err.to_string().contains("不是 Selection"));
        std::fs::remove_dir_all(&root).ok();
    }

    /// seed 一个 bound invariant ConstraintNode（human Objective constrained_by 它），带 verifier。
    /// human provenance 隐式接受 → objective bound → 经 constrained_by 继承 → constraint bound。
    fn seed_bound_invariant(root: &Path, verifier_ref: &str) {
        use sophia_graph_db::{
            ConstraintKind, ConstraintPayload, EdgeKind, Verifier, VerifierKind,
        };
        let mut store = open_store(root).unwrap();
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
        let constraint = store
            .as_human()
            .constraint(
                "inv",
                ConstraintPayload {
                    kind: ConstraintKind::Invariant,
                    statement: "AddOne 自增 1".into(),
                    verifier: Some(Verifier {
                        kind: VerifierKind::HiddenCase,
                        r#ref: verifier_ref.into(),
                    }),
                },
            )
            .unwrap();
        store
            .append_edge(obj, constraint, EdgeKind::ConstrainedBy)
            .unwrap();
    }

    /// 写隐藏验证用例存储 `sophia-runs/verifiers/hidden.json`（图外、不进 active context）。
    fn write_hidden(root: &Path, json: &str) {
        let path = crate::verifier_store::store_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json).unwrap();
    }

    const ADD_ONE: &str =
        "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }";

    #[test]
    fn hidden_case_passing_lets_gate_select() {
        // bound invariant 带 HiddenCase verifier；hidden.json 提供通过用例 → constraint_audit 放行。
        let root = temp_root("hc_pass");
        seed_bound_invariant(&root, "hc:add_one");
        write_hidden(
            &root,
            r#"[{"ref":"hc:add_one","entry_action":"AddOne","args":[{"Int":41}],"expected":{"Returns":{"Int":42}}}]"#,
        );
        let code = seed_code(&root, &[("MathDomain/actions/AddOne.sophia", ADD_ONE)]);

        let exit = select(&root, &code.as_string(), "候选").unwrap();
        assert_eq!(exit, ExitCode::SUCCESS, "hidden case 通过应放行 select");

        // 图中应有 ConstraintAudit DiagnosticNode 且 ok。
        let store = open_store(&root).unwrap();
        let audit_ok = store.nodes().any(|n| {
            matches!(&n.payload, NodePayload::Diagnostic(d)
                if d.kind == DiagnosticKind::ConstraintAudit && d.ok)
        });
        assert!(audit_ok, "应有通过的 ConstraintAudit 诊断");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn hidden_case_failing_blocks_gate() {
        // hidden.json 提供错误期望 → 真实执行判 fail → regression gate 阻断 select（不伪造）。
        let root = temp_root("hc_fail");
        seed_bound_invariant(&root, "hc:add_one");
        write_hidden(
            &root,
            r#"[{"ref":"hc:add_one","entry_action":"AddOne","args":[{"Int":41}],"expected":{"Returns":{"Int":999}}}]"#,
        );
        let code = seed_code(&root, &[("MathDomain/actions/AddOne.sophia", ADD_ONE)]);

        let exit = select(&root, &code.as_string(), "候选").unwrap();
        assert_eq!(exit, ExitCode::FAILURE, "hidden case 失败应阻断 select");

        let store = open_store(&root).unwrap();
        assert!(
            !store.nodes().any(|n| n.meta.role == NodeRole::Selection),
            "gate 阻断不应建 SelectionNode"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_hidden_case_blocks_gate_honestly() {
        // 声明了 HiddenCase verifier 但 hidden.json 缺对应 ref → 硬错误阻断（不伪造通过）。
        let root = temp_root("hc_missing");
        seed_bound_invariant(&root, "hc:absent");
        // 不写 hidden.json（空存储）。
        let code = seed_code(&root, &[("MathDomain/actions/AddOne.sophia", ADD_ONE)]);

        let exit = select(&root, &code.as_string(), "候选").unwrap();
        assert_eq!(exit, ExitCode::FAILURE, "缺用例应诚实阻断");

        // 应 emit RegressionGate 诊断（MissingVerifierOutcome 硬错误）。
        let store = open_store(&root).unwrap();
        let has_regression = store.nodes().any(|n| {
            matches!(&n.payload, NodePayload::Diagnostic(d)
                if d.kind == DiagnosticKind::RegressionGate && !d.ok)
        });
        assert!(has_regression, "应 emit 阻断性 RegressionGate 诊断");
        std::fs::remove_dir_all(&root).ok();
    }
}
