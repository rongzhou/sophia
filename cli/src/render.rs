//! 诊断渲染（CLI 呈现层）。
//!
//! 见 docs/language_implementation.md 14.3：错误信息同时服务人工与 LLM 修复循环。
//! 行列按 1 基呈现。`core` / `tools` 产出结构化诊断；此处只负责文本投影。

use sophia_syntax::{Span, SyntaxDiagnostic};

/// 渲染一条语法诊断（`文件:行:列 [code] 信息`）。
pub fn syntax_line(path: &str, d: &SyntaxDiagnostic) -> String {
    format!(
        "  {}:{}:{} [{:?}] 节点 `{}`",
        path,
        d.span.start.row + 1,
        d.span.start.column + 1,
        d.kind,
        d.node_kind,
    )
}

/// 渲染一条通用诊断（HIR / semantic），带稳定 code 与信息。
pub fn diag_line(path: &str, span: Span, code: &str, message: &str) -> String {
    format!(
        "  {}:{}:{} [{}] {}",
        path,
        span.start.row + 1,
        span.start.column + 1,
        code,
        message,
    )
}

/// 呈现 Execution Graph 执行 Trace 投影（docs/language_implementation.md 9.4）。
///
/// 每条 span 一行：`seq` + 缩进（按 depth）+ callable 名 + 节点/边 ID + 结局。
/// 缩进直观反映调用层级；节点 / 边 ID 让 trace 可映射回执行图结构。
pub fn print_trace(trace: &sophia_runtime::Trace) {
    use sophia_runtime::SpanOutcome;
    println!(
        "执行 Trace（{} 条 span，投影到 Execution Graph）：",
        trace.len()
    );
    for span in trace.spans() {
        let indent = "  ".repeat(span.depth as usize + 1);
        let edge = match span.edge_id {
            Some(id) => format!("edge E{}", id.0),
            None => "顶层入口".to_string(),
        };
        let outcome = match span.outcome {
            SpanOutcome::Returned => "return",
            SpanOutcome::Raised => "raise",
        };
        println!(
            "{}#{} {} [node N{}, {}] → {}",
            indent, span.seq, span.callable, span.node_id.0, edge, outcome
        );
    }
}
