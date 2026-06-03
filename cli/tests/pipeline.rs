//! CLI 端到端集成测试：init → index → check → run。
//!
//! 直接调用编译出的 `sophia` 二进制（`CARGO_BIN_EXE_sophia`），在临时目录内建项目。

use std::path::PathBuf;
use std::process::Command;

fn sophia() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sophia"))
}

/// 唯一临时项目目录。
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sophia_cli_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(root: &std::path::Path, rel: &str, content: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn init_creates_skeleton() {
    let dir = temp_project("init");
    let status = sophia().args(["init"]).arg(&dir).status().unwrap();
    assert!(status.success());
    assert!(dir.join("sophia.toml").exists());
    assert!(dir.join("domains").is_dir());
    assert!(dir.join("sophia-runs/graph").is_dir());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_and_run_well_formed_action() {
    let dir = temp_project("run_ok");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/MathDomain/actions/AddOne.sophia",
        "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }",
    );

    // check 通过。
    let check = sophia().arg("check").arg(&dir).status().unwrap();
    assert!(check.success(), "check 应通过");

    // run AddOne 41 → 42。
    let out = sophia()
        .args(["run", "AddOne", "--root"])
        .arg(&dir)
        .args(["--arg", "int:41"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("=> 42"), "应输出 => 42，实际：{stdout}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_trace_projects_execution_graph() {
    // `sophia run --trace`：跨调用程序应打印 Execution Graph 执行 Trace 投影（§9.4）。
    let dir = temp_project("run_trace");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/MathDomain/actions/Inner.sophia",
        "action Inner { input { x: Int } output { y: Int } body { return x + 1 } }",
    );
    write_file(
        &dir,
        "domains/MathDomain/actions/Outer.sophia",
        "action Outer { input { x: Int } output { y: Int } body { let a = Inner(x) return a + 10 } }",
    );

    let out = sophia()
        .args(["run", "Outer", "--root"])
        .arg(&dir)
        .args(["--arg", "int:5", "--trace"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("执行 Trace"), "应打印 trace：{stdout}");
    assert!(stdout.contains("Outer"), "trace 应含顶层 Outer：{stdout}");
    assert!(stdout.contains("Inner"), "trace 应含被调 Inner：{stdout}");
    assert!(stdout.contains("顶层入口"), "Outer 应标顶层入口：{stdout}");
    assert!(stdout.contains("edge E"), "Inner 应投影到调用边：{stdout}");
    assert!(stdout.contains("=> 16"), "Inner(5)=6, Outer=16：{stdout}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn index_generates_asg_index_json() {
    let dir = temp_project("index");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/entities/Todo.sophia",
        "entity Todo { fields { id { type: Int } } }",
    );
    let status = sophia().arg("index").arg(&dir).status().unwrap();
    assert!(status.success());

    let index_path = dir.join("sophia-runs/asg_index.json");
    assert!(index_path.exists());
    let json = std::fs::read_to_string(&index_path).unwrap();
    assert!(json.contains("\"Todo\""));
    assert!(json.contains("\"kind\": \"Entity\""));
    assert!(json.contains("\"version\": 1"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_reports_semantic_error() {
    let dir = temp_project("check_fail");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    // print 但未声明 Console.Write effect → 语义诊断。
    write_file(
        &dir,
        "domains/D/actions/Bad.sophia",
        "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }",
    );
    let out = sophia().arg("check").arg(&dir).output().unwrap();
    assert!(!out.status.success(), "check 应失败");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CHECK-EFFECT-001"),
        "应含未声明 effect 诊断：{stderr}"
    );
    assert!(
        stderr.contains("domains/D/actions/Bad.sophia"),
        "诊断应归属到文件：{stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_refuses_when_check_fails() {
    let dir = temp_project("run_fail");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/actions/Bad.sophia",
        "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }",
    );
    let out = sophia()
        .args(["run", "Bad", "--root"])
        .arg(&dir)
        .args(["--arg", "int:1"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "语义检查未通过应拒绝运行");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_propagates_domain_error_as_failure() {
    let dir = temp_project("run_raise");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/errors/E.sophia",
        "error E { variant Bad { reason: Text } }",
    );
    write_file(
        &dir,
        "domains/D/actions/Fail.sophia",
        "action Fail { input { n: Int } output { r: Int } errors { Bad } body { raise Bad { reason = \"no\" } } }",
    );
    let out = sophia()
        .args(["run", "Fail", "--root"])
        .arg(&dir)
        .args(["--arg", "int:1"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "raise 应以失败退出码呈现");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("raise"), "应呈现 raise：{stderr}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_summary_still_works_without_subcommand() {
    // `graph --root <dir>`（无子命令）应仍输出 ASG 摘要。
    let dir = temp_project("graph_asg");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/entities/Todo.sophia",
        "entity Todo { fields { id { type: Int } } }",
    );
    let out = sophia()
        .args(["graph", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ASG 摘要"), "应输出 ASG 摘要：{stdout}");
    assert!(stdout.contains("Todo"), "应含 Todo 节点：{stdout}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_dev_workflow_init_start_nodes_context() {
    // Development Graph 工作流子命令端到端：init → start → nodes → context。
    let dir = temp_project("graph_dev");
    sophia().args(["init"]).arg(&dir).status().unwrap();

    // graph init 建库。
    let init = sophia()
        .args(["graph", "init", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(init.status.success());
    assert!(dir.join("sophia-runs/graph/dev_graph.sqlite").exists());

    // graph start 建 ObjectiveNode。
    let start = sophia()
        .args(["graph", "start", "实现功能", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(start.status.success());
    let start_out = String::from_utf8_lossy(&start.stdout);
    assert!(start_out.contains("N0001"), "应创建 N0001：{start_out}");

    // graph nodes 列出节点（事件溯源 replay 跨进程持久化）。
    let nodes = sophia()
        .args(["graph", "nodes", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(nodes.status.success());
    let nodes_out = String::from_utf8_lossy(&nodes.stdout);
    assert!(
        nodes_out.contains("N0001"),
        "replay 应见 N0001：{nodes_out}"
    );
    assert!(nodes_out.contains("Objective"));
    assert!(nodes_out.contains("Human"));

    // graph context：human 目标隐式 bound，应出现在绑定目标中。
    let ctx = sophia()
        .args(["graph", "context", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(ctx.status.success());
    let ctx_out = String::from_utf8_lossy(&ctx.stdout);
    assert!(
        ctx_out.contains("Active Context"),
        "应输出 active context：{ctx_out}"
    );
    assert!(ctx_out.contains("N0001"), "human 目标应 bound：{ctx_out}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_start_appends_across_invocations() {
    // 两次 start 应 append N0001、N0002（事件溯源仅增）。
    let dir = temp_project("graph_append");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    sophia()
        .args(["graph", "start", "目标一", "--root"])
        .arg(&dir)
        .status()
        .unwrap();
    sophia()
        .args(["graph", "start", "目标二", "--root"])
        .arg(&dir)
        .status()
        .unwrap();

    let nodes = sophia()
        .args(["graph", "nodes", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&nodes.stdout);
    assert!(
        out.contains("N0001") && out.contains("N0002"),
        "应见两节点：{out}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_design_unreachable_backend_emits_raw_llm_and_fails() {
    // design 对不可达后端：应 emit RawLlmNode（attempted→ 目标）、失败退出，不伪造成功。
    let dir = temp_project("graph_design_fail");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    sophia()
        .args(["graph", "start", "实现功能", "--root"])
        .arg(&dir)
        .status()
        .unwrap();

    // 指向一个不可达的本地端口（确定性失败，不依赖外网）。
    let out = sophia()
        .args([
            "graph",
            "design",
            "N0001",
            "--model",
            "qwen3",
            "--mode",
            "ollama",
            "--base-url",
            "http://127.0.0.1:59999",
            "--root",
        ])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success(), "后端不可达应失败退出");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("未伪造成功"), "应声明未伪造成功：{stderr}");

    // 图中应出现 RawLlm 兜底节点与 ContextSnapshot（调用前已建）。
    let nodes = sophia()
        .args(["graph", "nodes", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    let nodes_out = String::from_utf8_lossy(&nodes.stdout);
    assert!(
        nodes_out.contains("RawLlm"),
        "应保留 RawLlm 兜底：{nodes_out}"
    );
    assert!(
        nodes_out.contains("ContextSnapshot"),
        "调用前应已建 snapshot：{nodes_out}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_design_rejects_non_target_domain() {
    // design 的节点须是 Objective | Milestone | FirstSlice；指向不存在的节点应失败。
    let dir = temp_project("graph_design_bad_node");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    sophia()
        .args(["graph", "init", "--root"])
        .arg(&dir)
        .status()
        .unwrap();

    let out = sophia()
        .args([
            "graph", "design", "N0099", "--model", "qwen3", "--mode", "ollama", "--root",
        ])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success(), "不存在的节点应失败");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_implement_loop_rejects_non_pseudocode_source() {
    // implement-loop 的 --pseudo 必须是 Pseudocode 节点；指向 Objective 应失败。
    let dir = temp_project("graph_il_bad_pseudo");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    sophia()
        .args(["graph", "start", "目标", "--root"])
        .arg(&dir)
        .status()
        .unwrap();

    let out = sophia()
        .args([
            "graph",
            "implement-loop",
            "N0001",
            "--pseudo",
            "N0001",
            "--model",
            "qwen3",
            "--root",
        ])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success(), "非 Pseudocode 源应失败");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("不是 Pseudocode"),
        "应提示非 Pseudocode：{stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn context_action_outputs_semantic_closure() {
    // sophia context --action：从 action root 计算语义闭包并稳定输出节点 / 边 / 文件。
    let dir = temp_project("ctx_action");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/TodoDomain/entities/Todo.sophia",
        "entity Todo { fields { id { type: Int } status { type: TodoStatus } } }",
    );
    write_file(
        &dir,
        "domains/TodoDomain/states/TodoStatus.sophia",
        "state TodoStatus { value Pending { meaning: \"p\" } value Done { meaning: \"d\" } }",
    );
    write_file(
        &dir,
        "domains/TodoDomain/capabilities/TodoCapability.sophia",
        "capability TodoCapability { allow { Console.Write } }",
    );
    write_file(
        &dir,
        "domains/TodoDomain/actions/GetTodo.sophia",
        "action GetTodo { capability: TodoCapability input { todo: Todo } output { todo: Todo } effects { Console.Write } body { print \"got\" return todo } }",
    );

    let out = sophia()
        .args(["context", "--action", "GetTodo", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "context --action 应成功");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("语义闭包（root = GetTodo）"));
    assert!(
        stdout.contains("binds_capability"),
        "应有 capability 边：{stdout}"
    );
    assert!(stdout.contains("Todo"), "应含 entity 依赖：{stdout}");
    assert!(stdout.contains("TodoStatus"), "应传递含 state：{stdout}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn context_requires_action_or_task() {
    let dir = temp_project("ctx_none");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/actions/A.sophia",
        "action A { input { n: Int } output { r: Int } body { return n } }",
    );
    let out = sophia()
        .args(["context", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success(), "缺 --action/--task 应失败");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_emits_wasm_artifact() {
    // `sophia build`：check 通过后 emit WASM artifact 到 sophia-runs/build/program.wasm。
    let dir = temp_project("build_wasm");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/MathDomain/actions/AddOne.sophia",
        "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }",
    );

    let out = sophia().arg("build").arg(&dir).output().unwrap();
    assert!(out.status.success(), "build 应通过");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("WASM artifact"),
        "应报 emit artifact：{stdout}"
    );
    assert!(
        stdout.contains("strip-assist artifact 等价"),
        "应报 artifact 门禁通过：{stdout}"
    );

    let wasm = dir.join("sophia-runs/build/program.wasm");
    assert!(wasm.exists(), "应产出 program.wasm");
    let bytes = std::fs::read(&wasm).unwrap();
    assert_eq!(&bytes[0..4], b"\0asm", "应是合法 WASM 魔数");
    assert_eq!(&bytes[4..8], &[1, 0, 0, 0], "WASM 版本应为 1");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_uses_project_library_sources() {
    let dir = temp_project("build_lib_source");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "sophia_libs/math_sophia/library.toml",
        r#"[library]
name = "math_sophia"
summary = "测试用纯 Sophia 数学库"
abi_version = 1

[surface]
sophia_sources = ["src/double.sophia"]

[prompt]
asset = "math_sophia.md"
"#,
    );
    write_file(&dir, "sophia_libs/math_sophia/math_sophia.md", "测试资产");
    write_file(
        &dir,
        "sophia_libs/math_sophia/src/double.sophia",
        "action LibDouble { input { n: Int } output { y: Int } body { return n + n } }",
    );
    write_file(
        &dir,
        "domains/MathDomain/actions/UseLib.sophia",
        "action UseLib { input { n: Int } output { y: Int } body { return LibDouble(n) } }",
    );

    let out = sophia().arg("build").arg(&dir).output().unwrap();
    assert!(
        out.status.success(),
        "build 应并入项目三方 Sophia 库源码，stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let wasm = dir.join("sophia-runs/build/program.wasm");
    assert!(wasm.exists(), "应产出 program.wasm");
    let bytes = std::fs::read(&wasm).unwrap();
    assert_eq!(&bytes[0..4], b"\0asm", "应是合法 WASM 魔数");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_reports_uncovered_construct_honestly() {
    // codegen 尚未覆盖的构造（list）：build 诚实报告，不伪造产出（解释执行仍可用）。
    let dir = temp_project("build_uncovered");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/actions/Pack.sophia",
        "action Pack { input { a: Int; b: Int } output { xs: list of Int } body { return [a, b] } }",
    );

    let out = sophia().arg("build").arg(&dir).output().unwrap();
    assert!(!out.status.success(), "未覆盖构造应使 build 失败");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("尚未覆盖"), "应诚实报告未覆盖：{stderr}");
    assert!(
        !dir.join("sophia-runs/build/program.wasm").exists(),
        "失败时不应产出 artifact"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn smoke_passes_for_well_formed_project_and_runs_action() {
    // smoke：init（幂等）→ check → build → run AddOne，全链路通过。
    let dir = temp_project("smoke_ok");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/MathDomain/actions/AddOne.sophia",
        "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }",
    );

    let out = sophia()
        .args(["smoke", "--action", "AddOne", "--root"])
        .arg(&dir)
        .args(["--arg", "int:41"])
        .output()
        .unwrap();
    assert!(out.status.success(), "smoke 应通过");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("OK：smoke 通过"),
        "应报 smoke 通过：{stdout}"
    );
    assert!(stdout.contains("=> 42"), "run 步骤应输出 => 42：{stdout}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn smoke_without_action_does_check_and_build_only() {
    // 未指定 --action：smoke 只做 check / build，跳过 run。
    let dir = temp_project("smoke_noaction");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/entities/Todo.sophia",
        "entity Todo { fields { id { type: Int } } }",
    );

    let out = sophia()
        .args(["smoke", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "无 action 的 smoke 应通过");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("跳过"), "应跳过 run 步骤：{stdout}");
    assert!(stdout.contains("OK：smoke 通过"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn smoke_fails_when_check_fails() {
    // check 未过时 smoke 必须以失败退出码中止（不伪造通过）。
    let dir = temp_project("smoke_fail");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/actions/Bad.sophia",
        "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }",
    );
    let out = sophia()
        .args(["smoke", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success(), "check 失败应使 smoke 失败");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("check 未通过"), "应中止于 check：{stderr}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn repair_context_emits_structured_context_for_matching_diagnostic() {
    // repair-context：对未声明 effect 的语义诊断，给出位置 + 诊断码 + 相关节点闭包。
    let dir = temp_project("repair_ctx");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/D/actions/Bad.sophia",
        "action Bad { input { n: Int } output { r: Int } body { print \"hi\" return n } }",
    );

    let out = sophia()
        .args(["repair-context", "--error", "CHECK-EFFECT", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "repair-context 自身应成功退出");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("修复上下文"), "应输出修复上下文：{stdout}");
    assert!(
        stdout.contains("CHECK-EFFECT-001"),
        "应含匹配诊断码：{stdout}"
    );
    assert!(
        stdout.contains("domains/D/actions/Bad.sophia"),
        "应归属到文件：{stdout}"
    );
    assert!(
        stdout.contains("相关节点"),
        "应给出 action 语义闭包：{stdout}"
    );
    assert!(
        stdout.contains("不臆造修复建议"),
        "应声明不臆造修复建议：{stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn repair_context_reports_no_match_cleanly() {
    // 无匹配诊断（项目干净）：成功退出并提示未找到。
    let dir = temp_project("repair_clean");
    sophia().args(["init"]).arg(&dir).status().unwrap();
    write_file(
        &dir,
        "domains/MathDomain/actions/AddOne.sophia",
        "action AddOne { input { n: Int } output { r: Int } body { return n + 1 } }",
    );

    let out = sophia()
        .args(["repair-context", "--error", "CHECK-TYPE", "--root"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("未找到匹配"),
        "干净项目应提示无匹配：{stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}
