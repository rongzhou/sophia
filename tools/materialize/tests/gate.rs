//! Materialize gate 与原子写入测试。

use sophia_materialize::*;

fn sample_files() -> Vec<(String, String)> {
    vec![(
        "TodoDomain/actions/CompleteTodo.sophia".to_string(),
        "action CompleteTodo { }\n".to_string(),
    )]
}

/// 走完全部 gate 后物化的辅助。
fn full_pipeline(files: Vec<(String, String)>) -> MaterializeResult<CodeCandidate<Selected>> {
    let checked = CodeCandidate::new(files).run_check(&GateReport::pass())?;
    let audited = checked.run_audit(&GateReport::pass())?;
    let validated = audited.run_runtime_validation(&GateReport::pass(), &GateReport::pass())?;
    Ok(validated.select())
}

#[test]
fn full_gate_pipeline_then_materialize_writes_files() {
    let dir = std::env::temp_dir().join(format!("sophia_mat_ok_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let selected = full_pipeline(sample_files()).expect("gate pipeline");
    let outcome = selected.materialize(&dir).expect("materialize");

    assert_eq!(outcome.files.len(), 1);
    let written = dir.join("TodoDomain/actions/CompleteTodo.sophia");
    assert!(written.exists(), "目标文件应被写入");
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        "action CompleteTodo { }\n"
    );
    // staging 目录应已清理。
    assert!(!dir.join(".sophia-staging").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn materialize_does_not_reuse_fixed_staging_directory() {
    let dir = std::env::temp_dir().join(format!("sophia_mat_staging_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".sophia-staging")).unwrap();
    std::fs::write(dir.join(".sophia-staging/marker"), "keep").unwrap();

    let selected = full_pipeline(sample_files()).expect("gate pipeline");
    selected.materialize(&dir).expect("materialize");

    assert_eq!(
        std::fs::read_to_string(dir.join(".sophia-staging/marker")).unwrap(),
        "keep"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_failure_stops_pipeline() {
    let err = CodeCandidate::new(sample_files())
        .run_check(&GateReport::fail("CHECK-TYPE-001 类型不匹配"))
        .unwrap_err();
    assert!(matches!(err, MaterializeError::CheckFailed(_)));
}

#[test]
fn audit_failure_stops_pipeline() {
    let checked = CodeCandidate::new(sample_files())
        .run_check(&GateReport::pass())
        .unwrap();
    let err = checked
        .run_audit(&GateReport::fail("约束 X 被违反"))
        .unwrap_err();
    assert!(matches!(err, MaterializeError::AuditFailed(_)));
}

#[test]
fn artifact_diff_failure_stops_pipeline() {
    let audited = CodeCandidate::new(sample_files())
        .run_check(&GateReport::pass())
        .unwrap()
        .run_audit(&GateReport::pass())
        .unwrap();
    let err = audited
        .run_runtime_validation(
            &GateReport::fail("strip-assist 不等价"),
            &GateReport::pass(),
        )
        .unwrap_err();
    assert!(matches!(err, MaterializeError::ArtifactDiffFailed(_)));
}

#[test]
fn runtime_validation_failure_stops_pipeline() {
    let audited = CodeCandidate::new(sample_files())
        .run_check(&GateReport::pass())
        .unwrap()
        .run_audit(&GateReport::pass())
        .unwrap();
    let err = audited
        .run_runtime_validation(&GateReport::pass(), &GateReport::fail("output 校验失败"))
        .unwrap_err();
    assert!(matches!(err, MaterializeError::RuntimeValidationFailed(_)));
}

#[test]
fn file_paths_available_before_materialize() {
    let selected = full_pipeline(sample_files()).unwrap();
    assert_eq!(
        selected.file_paths(),
        vec!["TodoDomain/actions/CompleteTodo.sophia".to_string()]
    );
}

#[test]
fn atomic_write_rejects_parent_escape() {
    let dir = std::env::temp_dir().join(format!("sophia_mat_escape_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = vec![("../evil.sophia".to_string(), "x".to_string())];
    let err = atomic_write_all(&dir, &files).unwrap_err();
    assert!(matches!(err, MaterializeError::Write(_)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn atomic_write_rejects_absolute_path() {
    let dir = std::env::temp_dir().join(format!("sophia_mat_abs_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = vec![("/etc/evil".to_string(), "x".to_string())];
    let err = atomic_write_all(&dir, &files).unwrap_err();
    assert!(matches!(err, MaterializeError::Write(_)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn materialize_writes_multiple_files() {
    let dir = std::env::temp_dir().join(format!("sophia_mat_multi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let files = vec![
        ("D/entities/A.sophia".to_string(), "entity A {}".to_string()),
        ("D/actions/B.sophia".to_string(), "action B {}".to_string()),
    ];
    let selected = full_pipeline(files).unwrap();
    selected.materialize(&dir).unwrap();

    assert!(dir.join("D/entities/A.sophia").exists());
    assert!(dir.join("D/actions/B.sophia").exists());

    std::fs::remove_dir_all(&dir).ok();
}
