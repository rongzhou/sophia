//! 多候选评分排序测试（design 10.9）。

use sophia_materialize::{rank_candidates, score_candidate, ScoreInputs, ScoreWeights};

fn files(content: &str) -> Vec<(String, String)> {
    vec![("D/A.sophia".to_string(), content.to_string())]
}

#[test]
fn compile_fail_caps_overall_at_049() {
    // 不可编译候选：即使其它维度全满，overall 也不得超过 0.49（design 10.9 硬约束）。
    let fs = files("action A { input { x: Int } output { y: Int } body { return x } }");
    let inputs = ScoreInputs {
        compile_pass: false,
        tests_pass: true,
        constraints_pass: true,
        files: &fs,
        pseudocode_clarity: Some(1.0),
    };
    let s = score_candidate(&inputs, &ScoreWeights::default());
    assert_eq!(s.compile, 0.0);
    assert!(
        s.overall <= 0.49,
        "compile=0 时 overall 应封顶 0.49，实际 {}",
        s.overall
    );
}

#[test]
fn compile_pass_beats_compile_fail() {
    // 可编译候选应排在不可编译候选之前。
    let ok = files("action A { input { x: Int } output { y: Int } body { return x } }");
    let bad = files("action A { input { x: Int } output y int body { return n } }");
    let inputs = vec![
        ScoreInputs {
            compile_pass: false,
            tests_pass: false,
            constraints_pass: false,
            files: &bad,
            pseudocode_clarity: None,
        },
        ScoreInputs {
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            files: &ok,
            pseudocode_clarity: None,
        },
    ];
    let ranking = rank_candidates(&inputs, &ScoreWeights::default());
    // 第二个（可编译）应排第一。
    assert_eq!(ranking[0].0, 1, "可编译候选应胜出");
    assert!(ranking[0].1.overall > ranking[1].1.overall);
}

#[test]
fn simpler_candidate_wins_when_correctness_equal() {
    // 正确性维度相同时，更简单（更短）的候选 overall 更高。
    let short = files("action A { input { x: Int } output { y: Int } body { return x } }");
    let long = files(
        "action A { input { x: Int } output { y: Int } body { \
         let a = x let b = a let c = b let d = c let e = d return e } }",
    );
    let inputs = vec![
        ScoreInputs {
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            files: &long,
            pseudocode_clarity: None,
        },
        ScoreInputs {
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            files: &short,
            pseudocode_clarity: None,
        },
    ];
    let ranking = rank_candidates(&inputs, &ScoreWeights::default());
    assert_eq!(ranking[0].0, 1, "更短的候选应胜出（simplicity 更高）");
}

#[test]
fn fewer_capability_decls_score_higher_minimality() {
    // 声明更少 effect / capability 的候选，capability_minimality 更高。
    let minimal = files("action A { input { x: Int } output { y: Int } body { return x } }");
    let permissive = files(
        "action A { capability: C input { x: Int } output { y: Int } \
         effects { Console.Write } body { print x return x } }",
    );
    let s_min = score_candidate(
        &ScoreInputs {
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            files: &minimal,
            pseudocode_clarity: None,
        },
        &ScoreWeights::default(),
    );
    let s_perm = score_candidate(
        &ScoreInputs {
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            files: &permissive,
            pseudocode_clarity: None,
        },
        &ScoreWeights::default(),
    );
    assert!(
        s_min.capability_minimality > s_perm.capability_minimality,
        "更少权限声明应有更高 capability_minimality"
    );
}

#[test]
fn ranking_is_deterministic_with_stable_tiebreak() {
    // 完全相同的候选：overall 相等 → 按原始下标升序（确定性平局打破）。
    let fs = files("action A { input { x: Int } output { y: Int } body { return x } }");
    let mk = || ScoreInputs {
        compile_pass: true,
        tests_pass: true,
        constraints_pass: true,
        files: &fs,
        pseudocode_clarity: None,
    };
    let inputs = vec![mk(), mk(), mk()];
    let ranking = rank_candidates(&inputs, &ScoreWeights::default());
    assert_eq!(
        ranking.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "平局应按原始下标升序"
    );
}

#[test]
fn empty_input_yields_empty_ranking() {
    let ranking = rank_candidates(&[], &ScoreWeights::default());
    assert!(ranking.is_empty());
}

#[test]
fn pseudocode_clarity_defaults_neutral() {
    // 无 pseudocode_clarity 信号 → 取中性 0.5（不伪造）。
    let fs = files("action A { input { x: Int } output { y: Int } body { return x } }");
    let s = score_candidate(
        &ScoreInputs {
            compile_pass: true,
            tests_pass: true,
            constraints_pass: true,
            files: &fs,
            pseudocode_clarity: None,
        },
        &ScoreWeights::default(),
    );
    assert_eq!(s.pseudocode_clarity, 0.5);
}
