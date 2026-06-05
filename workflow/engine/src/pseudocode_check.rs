//! Deterministic pseudocode format gate.
//!
//! This check validates the stable pseudocode envelope expected by the workflow.
//! It does not inspect task content and does not rewrite LLM output.

use sophia_graph_db::{
    DiagnosticItem, DiagnosticKind, DiagnosticPayload, DiagnosticSeverity, EdgeKind, GraphStore,
    NodeId,
};

/// Validate the workflow pseudocode envelope and return a `PseudoCheck` diagnostic payload.
pub fn pseudocode_check(text: &str) -> DiagnosticPayload {
    const REQUIRED: &[&str] = &[
        "Purpose",
        "Inputs",
        "Outputs",
        "Algorithm",
        "Constraints",
        "Forbidden",
    ];

    let schema_version_ok = text
        .lines()
        .next()
        .map(|line| line.trim() == "<!-- sophia-pseudo: v1 -->")
        .unwrap_or(false);
    let missing: Vec<&str> = REQUIRED
        .iter()
        .copied()
        .filter(|heading| !has_pseudo_heading(text, heading))
        .collect();

    let mut diagnostics = Vec::new();
    if !schema_version_ok {
        diagnostics.push(DiagnosticItem {
            code: "PSEUDO-SCHEMA".to_string(),
            severity: DiagnosticSeverity::Error,
            problem: "伪代码首行必须是 `<!-- sophia-pseudo: v1 -->`".to_string(),
            location: Some("content.pseudo:1".to_string()),
        });
    }
    if !missing.is_empty() {
        diagnostics.push(DiagnosticItem {
            code: "PSEUDO-HEADINGS".to_string(),
            severity: DiagnosticSeverity::Error,
            problem: format!("伪代码缺少固定 heading：{}", missing.join(", ")),
            location: Some("content.pseudo".to_string()),
        });
    }

    DiagnosticPayload {
        kind: DiagnosticKind::PseudoCheck,
        ok: diagnostics.is_empty(),
        diagnostics,
    }
}

/// Emit a deterministic `PseudoCheck` diagnostic with `checks-> pseudocode`.
pub fn record_pseudocode_check(
    store: &mut GraphStore,
    pseudocode: NodeId,
    summary: &str,
    text: &str,
) -> Result<(NodeId, bool), sophia_graph_db::GraphError> {
    let payload = pseudocode_check(text);
    let ok = payload.ok;
    let diag = store.as_deterministic().diagnostic(summary, payload)?;
    store.append_edge(diag, pseudocode, EdgeKind::Checks)?;
    Ok((diag, ok))
}

fn has_pseudo_heading(text: &str, heading: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        matches!(
            trimmed.strip_prefix('#'),
            Some(rest) if rest.trim_start_matches('#').trim() == heading
        )
    })
}

#[cfg(test)]
mod tests {
    use super::pseudocode_check;

    #[test]
    fn accepts_required_envelope() {
        let text = "\
<!-- sophia-pseudo: v1 -->
# Purpose
# Inputs
# Outputs
# Algorithm
# Constraints
# Forbidden
";

        assert!(pseudocode_check(text).ok);
    }

    #[test]
    fn rejects_missing_version_and_headings() {
        let payload = pseudocode_check("# Purpose\n");

        assert!(!payload.ok);
        assert_eq!(payload.diagnostics.len(), 2);
    }
}
