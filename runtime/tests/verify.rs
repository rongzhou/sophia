//! Hidden-case verifier 执行器测试（regression gate 执行侧）。
//!
//! 见 docs/workflow_graph_spec.md 4.1.2、第七节接入点 4。验证 runner 在 v0 解释器上
//! 真正执行 hidden case 并如实判定 pass/fail（绝不伪造）。

use sophia_hir::{AsgIndex, IndexInput, LibraryRegistry};
use sophia_runtime::{
    run_hidden_case, run_hidden_cases, ExpectedOutcome, HiddenCase, Value, VerificationResult,
};
use sophia_semantic::{analyze_program, SemanticModel};
use sophia_syntax::{parse_ast, Ast};

struct Program {
    asts: Vec<Ast>,
}

impl Program {
    fn new(sources: &[&str]) -> Self {
        Program {
            asts: sources
                .iter()
                .map(|s| parse_ast(*s).expect("parse"))
                .collect(),
        }
    }

    fn analyze(&self) -> SemanticModel {
        let inputs: Vec<IndexInput> = self
            .asts
            .iter()
            .enumerate()
            .map(|(i, a)| IndexInput {
                domain: "D",
                path: Box::leak(format!("domains/D/n/{i}.sophia").into_boxed_str()),
                ast: a,
            })
            .collect();
        let index = AsgIndex::build(inputs, &LibraryRegistry::empty()).expect("index");
        let refs: Vec<&Ast> = self.asts.iter().collect();
        let analysis = analyze_program(&refs, &index);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        analysis.model
    }

    fn refs(&self) -> Vec<&Ast> {
        self.asts.iter().collect()
    }
}

const ADD_ONE: &str = "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }";

#[test]
fn passing_case_returns_passed() {
    let prog = Program::new(&[ADD_ONE]);
    let model = prog.analyze();
    let case = HiddenCase {
        verifier_ref: "hc:add_one_41".into(),
        entry_action: "AddOne".into(),
        args: vec![Value::Int(41)],
        expected: ExpectedOutcome::Returns(Value::Int(42)),
    };
    let r = run_hidden_case(&model, &prog.refs(), &case);
    assert_eq!(
        r,
        VerificationResult {
            verifier_ref: "hc:add_one_41".into(),
            passed: true,
            detail: "返回值匹配：42".into(),
        }
    );
}

#[test]
fn wrong_value_fails_without_faking() {
    let prog = Program::new(&[ADD_ONE]);
    let model = prog.analyze();
    let case = HiddenCase {
        verifier_ref: "hc:add_one_wrong".into(),
        entry_action: "AddOne".into(),
        args: vec![Value::Int(41)],
        expected: ExpectedOutcome::Returns(Value::Int(999)),
    };
    let r = run_hidden_case(&model, &prog.refs(), &case);
    assert!(!r.passed, "结果不匹配必须判 fail，绝不伪造");
    assert!(r.detail.contains("不匹配"));
}

#[test]
fn expected_raise_matches_variant() {
    let prog = Program::new(&[
        "error E { variant Bad { reason: Text } }",
        r#"action Fail {
  input { x: Int }
  output { y: Int }
  errors { Bad }
  body { raise Bad { reason = "nope" } }
}"#,
    ]);
    let model = prog.analyze();
    let case = HiddenCase {
        verifier_ref: "hc:fail_raises".into(),
        entry_action: "Fail".into(),
        args: vec![Value::Int(1)],
        expected: ExpectedOutcome::Raises("Bad".into()),
    };
    let r = run_hidden_case(&model, &prog.refs(), &case);
    assert!(r.passed, "raise variant 匹配应通过：{}", r.detail);
}

#[test]
fn returned_when_raise_expected_fails() {
    let prog = Program::new(&[ADD_ONE]);
    let model = prog.analyze();
    let case = HiddenCase {
        verifier_ref: "hc:expected_raise".into(),
        entry_action: "AddOne".into(),
        args: vec![Value::Int(1)],
        expected: ExpectedOutcome::Raises("Bad".into()),
    };
    let r = run_hidden_case(&model, &prog.refs(), &case);
    assert!(!r.passed);
    assert!(r.detail.contains("期望 raise"));
}

#[test]
fn execution_hard_error_is_fail_not_pass() {
    // 实参类型错误 → 执行硬错误；必须判 fail（不把硬错误当通过）。
    let prog = Program::new(&[ADD_ONE]);
    let model = prog.analyze();
    let case = HiddenCase {
        verifier_ref: "hc:bad_arg".into(),
        entry_action: "AddOne".into(),
        args: vec![Value::Text("not int".into())],
        expected: ExpectedOutcome::Returns(Value::Int(42)),
    };
    let r = run_hidden_case(&model, &prog.refs(), &case);
    assert!(!r.passed, "执行硬错误必须判 fail");
    assert!(r.detail.contains("硬错误"));
}

#[test]
fn batch_runs_all_cases_in_order() {
    let prog = Program::new(&[ADD_ONE]);
    let model = prog.analyze();
    let cases = vec![
        HiddenCase {
            verifier_ref: "a".into(),
            entry_action: "AddOne".into(),
            args: vec![Value::Int(0)],
            expected: ExpectedOutcome::Returns(Value::Int(1)),
        },
        HiddenCase {
            verifier_ref: "b".into(),
            entry_action: "AddOne".into(),
            args: vec![Value::Int(10)],
            expected: ExpectedOutcome::Returns(Value::Int(11)),
        },
    ];
    let results = run_hidden_cases(&model, &prog.refs(), &cases);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].verifier_ref, "a");
    assert_eq!(results[1].verifier_ref, "b");
    assert!(results.iter().all(|r| r.passed));
}
