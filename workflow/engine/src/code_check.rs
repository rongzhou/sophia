//! 确定性 code_check 桥接：候选 `.sophia` 文件正文 → 结构化诊断（`DiagnosticPayload`）。
//!
//! 这是 implement-loop（[`crate::run_implement_loop`]）注入的确定性门禁的**单一实现**：
//! 把候选文件先过语法层、再（语法干净时）过 HIR 名称解析 + 语义三层 + strip-assist 等价
//! （桥接 `tools/check` 的 [`sophia_check::check_program`]），产出 `CodeCheck` 诊断 payload。
//!
//! 此前 CLI（`graph implement-loop`）、e2e harness、benchmark 三处各有一份逐字重复的桥接；
//! 统一到此（workflow 层——code_check 是工作流的确定性 gate，与 `run_implement_loop` 同层）。
//! 可观测性（逐轮打印）留给调用方（如 harness 包一层打印），本函数只做纯计算。

use sophia_graph_db::{DiagnosticItem, DiagnosticKind, DiagnosticPayload, DiagnosticSeverity};

/// 对候选文件运行确定性 code_check，返回 `CodeCheck` 诊断 payload（`ok` 表示是否全过）。
///
/// 阶段：① 语法层（每文件 `parse_str` + tree errors）；② 语法干净时进 HIR + 语义三层 +
/// strip-assist 等价（`check_program`）。任一语法错误即跳过语义阶段；`check_program` 作为公共
/// raw-source API 也会结构化返回语法错误，这里先过滤是为了保留更细的逐文件语法诊断。
pub fn code_check(files: &[(String, String)]) -> DiagnosticPayload {
    let mut items = path_diagnostics(files);
    if !items.is_empty() {
        return DiagnosticPayload {
            kind: DiagnosticKind::CodeCheck,
            ok: false,
            diagnostics: items,
        };
    }

    items.extend(syntax_diagnostics(files));
    if items.is_empty() {
        items.extend(semantic_diagnostics(files));
    }
    DiagnosticPayload {
        kind: DiagnosticKind::CodeCheck,
        ok: items.is_empty(),
        diagnostics: items,
    }
}

/// 候选文件路径来自 LLM / artifact 边界，先收敛为 domain-first `.sophia` 相对路径。
pub fn validate_candidate_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("候选文件路径不能为空".to_string());
    }
    if path.starts_with('/') {
        return Err(format!("候选文件路径不能是绝对路径：{path}"));
    }
    if path.contains('\\') {
        return Err(format!("候选文件路径必须使用正斜杠：{path}"));
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 3 {
        return Err(format!(
            "候选文件路径必须为 domain-first 布局 `<domain>/<category>/<file>.sophia`：{path}"
        ));
    }
    if parts.iter().any(|part| part.is_empty() || *part == ".") {
        return Err(format!("候选文件路径包含空段或 `.`：{path}"));
    }
    if parts.contains(&"..") {
        return Err(format!("候选文件路径不能包含 `..`：{path}"));
    }
    if !path.ends_with(".sophia") {
        return Err(format!("候选文件路径必须以 `.sophia` 结尾：{path}"));
    }
    Ok(())
}

fn path_diagnostics(files: &[(String, String)]) -> Vec<DiagnosticItem> {
    files
        .iter()
        .filter_map(|(path, _)| {
            validate_candidate_path(path)
                .err()
                .map(|problem| DiagnosticItem {
                    code: "PATH".to_string(),
                    severity: DiagnosticSeverity::Error,
                    problem,
                    location: Some(path.clone()),
                })
        })
        .collect()
}

/// 阶段一：语法层诊断（每文件解析，收集 tree-sitter ERROR/MISSING 节点）。
fn syntax_diagnostics(files: &[(String, String)]) -> Vec<DiagnosticItem> {
    let mut items = Vec::new();
    for (path, content) in files {
        match sophia_syntax::parse_str(content.clone()) {
            Ok(tree) => {
                let lines: Vec<&str> = content.lines().collect();
                for d in tree.errors() {
                    let row = d.span.start.row;
                    let snippet = lines.get(row).map(|l| l.trim()).unwrap_or("");
                    items.push(DiagnosticItem {
                        code: "SYNTAX".to_string(),
                        severity: DiagnosticSeverity::Error,
                        problem: format!("语法错误（{:?}）：靠近 `{}`", d.kind, snippet),
                        location: Some(format!("{path}:{}", row + 1)),
                    });
                }
            }
            Err(e) => items.push(DiagnosticItem {
                code: "SYNTAX".to_string(),
                severity: DiagnosticSeverity::Error,
                problem: e.to_string(),
                location: Some(path.clone()),
            }),
        }
    }
    items
}

/// 阶段二：HIR 名称解析 + 语义三层 + strip-assist 等价（桥接 `tools/check`）。
fn semantic_diagnostics(files: &[(String, String)]) -> Vec<DiagnosticItem> {
    let sources: Vec<(String, String, String)> = files
        .iter()
        .map(|(path, content)| (domain_of_path(path), path.clone(), content.clone()))
        .collect();
    let report = match sophia_check::check_program(&sources) {
        Ok(r) => r,
        Err(e) => {
            return vec![DiagnosticItem {
                code: "CHECK-BUILD".to_string(),
                severity: DiagnosticSeverity::Error,
                problem: e.to_string(),
                location: None,
            }]
        }
    };

    let line_loc = |row: usize| Some(format!("line {}", row + 1));
    let mut items: Vec<DiagnosticItem> = Vec::new();
    for d in &report.hir {
        items.push(DiagnosticItem {
            code: format!("{:?}", d.kind),
            severity: DiagnosticSeverity::Error,
            problem: d.message.clone(),
            location: line_loc(d.span.start.row),
        });
    }
    for d in &report.semantic {
        items.push(DiagnosticItem {
            code: d.code().to_string(),
            severity: DiagnosticSeverity::Error,
            problem: d.message.clone(),
            location: line_loc(d.span.start.row),
        });
    }
    if !report.strip_assist.equivalent {
        items.push(DiagnosticItem {
            code: "STRIP-ASSIST".to_string(),
            severity: DiagnosticSeverity::Error,
            problem: report
                .strip_assist
                .detail
                .clone()
                .unwrap_or_else(|| "strip-assist 等价性被破坏".to_string()),
            location: None,
        });
    }
    items
}

/// 从 domain-first 路径推导 domain（首段，如 `MathDomain/actions/Add.sophia` → `MathDomain`）。
pub fn domain_of_path(path: &str) -> String {
    path.split('/').next().unwrap_or("").to_string()
}
