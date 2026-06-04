//! Constraint audit 测试。

use sophia_audit::*;

fn invariant(id: &str, vref: Option<&str>, vkind: VerifierKind) -> Constraint {
    Constraint {
        id: id.into(),
        kind: ConstraintKind::Invariant,
        statement: "保持旧行为".into(),
        verifier: vref.map(|r| (vkind, r.into())),
    }
}

#[test]
fn invariant_with_passing_hidden_case_passes() {
    let cs = vec![invariant("N0001", Some("case_a"), VerifierKind::HiddenCase)];
    let outs = vec![VerifierOutcome {
        verifier_ref: "case_a".into(),
        passed: true,
        detail: String::new(),
    }];
    let report = audit_constraints(&cs, &outs).unwrap();
    assert!(report.ok());
    assert_eq!(report.items[0].verdict, ConstraintVerdict::Pass);
}

#[test]
fn invariant_with_failing_verifier_fails_gate() {
    let cs = vec![invariant("N0001", Some("rule_x"), VerifierKind::AuditRule)];
    let outs = vec![VerifierOutcome {
        verifier_ref: "rule_x".into(),
        passed: false,
        detail: "回归：旧行为被破坏".into(),
    }];
    let report = audit_constraints(&cs, &outs).unwrap();
    assert!(!report.ok());
    assert_eq!(report.failures().count(), 1);
    match &report.items[0].verdict {
        ConstraintVerdict::Fail { detail } => assert!(detail.contains("回归")),
        other => panic!("期望 Fail，得到 {other:?}"),
    }
}

#[test]
fn forbidden_with_failing_verifier_fails_gate() {
    let cs = vec![Constraint {
        id: "N0007".into(),
        kind: ConstraintKind::Forbidden,
        statement: "禁止写入项目根外文件".into(),
        verifier: Some((VerifierKind::HiddenCase, "forbidden_case".into())),
    }];
    let outs = vec![VerifierOutcome {
        verifier_ref: "forbidden_case".into(),
        passed: false,
        detail: "检测到禁止行为".into(),
    }];

    let report = audit_constraints(&cs, &outs).unwrap();

    assert!(!report.ok());
    assert_eq!(report.failures().count(), 1);
    assert_eq!(report.items[0].constraint_id, "N0007");
}

#[test]
fn forbidden_with_missing_verifier_outcome_is_hard_error() {
    let cs = vec![Constraint {
        id: "N0008".into(),
        kind: ConstraintKind::Forbidden,
        statement: "禁止网络访问".into(),
        verifier: Some((VerifierKind::HiddenCase, "net_case".into())),
    }];

    let err = audit_constraints(&cs, &[]).unwrap_err();

    assert!(matches!(err, AuditError::MissingVerifierOutcome { .. }));
}

#[test]
fn non_invariant_constraint_is_skipped() {
    let cs = vec![
        Constraint {
            id: "N0002".into(),
            kind: ConstraintKind::Preference,
            statement: "尽量简洁".into(),
            verifier: None,
        },
        Constraint {
            id: "N0003".into(),
            kind: ConstraintKind::OutOfScope,
            statement: "不处理并发".into(),
            verifier: None,
        },
    ];
    let report = audit_constraints(&cs, &[]).unwrap();
    assert!(report.ok());
    assert!(report
        .items
        .iter()
        .all(|i| matches!(i.verdict, ConstraintVerdict::Skipped { .. })));
}

#[test]
fn manual_verifier_is_skipped() {
    let cs = vec![invariant(
        "N0004",
        Some("human_review"),
        VerifierKind::Manual,
    )];
    let report = audit_constraints(&cs, &[]).unwrap();
    assert!(report.ok());
    assert!(matches!(
        report.items[0].verdict,
        ConstraintVerdict::Skipped { .. }
    ));
}

#[test]
fn invariant_without_verifier_is_skipped() {
    let cs = vec![invariant("N0005", None, VerifierKind::HiddenCase)];
    let report = audit_constraints(&cs, &[]).unwrap();
    assert!(matches!(
        report.items[0].verdict,
        ConstraintVerdict::Skipped { .. }
    ));
}

#[test]
fn missing_verifier_outcome_is_hard_error() {
    let cs = vec![invariant("N0006", Some("case_b"), VerifierKind::HiddenCase)];
    // 声明了可执行 verifier 但未提供结果 → 硬错误（不能静默放行）。
    let err = audit_constraints(&cs, &[]).unwrap_err();
    assert!(matches!(err, AuditError::MissingVerifierOutcome { .. }));
}

#[test]
fn mixed_constraints_report_only_invariant_failures() {
    let cs = vec![
        invariant("N0001", Some("c1"), VerifierKind::HiddenCase),
        invariant("N0002", Some("c2"), VerifierKind::HiddenCase),
        Constraint {
            id: "N0003".into(),
            kind: ConstraintKind::Forbidden,
            statement: "禁止网络".into(),
            verifier: None,
        },
    ];
    let outs = vec![
        VerifierOutcome {
            verifier_ref: "c1".into(),
            passed: true,
            detail: String::new(),
        },
        VerifierOutcome {
            verifier_ref: "c2".into(),
            passed: false,
            detail: "失败".into(),
        },
    ];
    let report = audit_constraints(&cs, &outs).unwrap();
    assert!(!report.ok());
    let fails: Vec<&str> = report
        .failures()
        .map(|i| i.constraint_id.as_str())
        .collect();
    assert_eq!(fails, vec!["N0002"]);
}
