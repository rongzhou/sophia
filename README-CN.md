# Sophia

> 一门面向 LLM-native / Agent-native 系统的**确定性语义编程语言**，为无人监管下的 LLM 自动编程而设计。

Sophia 要回答的核心问题是：如果一个 LLM 不擅长传统语法与惯例，但具备较强的自然语言语义理解能力，能否通过专为它设计的语言、检查器与工作流，让它在没有人工审查兜底的条件下稳定完成自主编程？

Sophia 的回答是分工：

- **LLM 负责**语义理解、任务分解、结构化表达与修复建议；
- **语言、编译器与工具链负责**确定性、边界、类型、副作用、错误与能力约束。

LLM 可以生成源码，但源码的行为只能由形式语言与编译器决定。Sophia 不是自然语言编程，也不是 prompt DSL，而是一门可编译语言。

> ⚠️ 项目处于早期阶段（`v0.1.0`）。当前为 **v0 解释执行**（已完成）+ **v1 WASM codegen 工作流 A 已落地**：源码经完整编译管线后既可由进程内解释器执行，也可经 `sophia build` emit 为 WASM artifact（与解释器逐 case 等价，差测试守护；起步子集全覆盖）。v1 余下为语言 / 标准库按需扩充与增量查询架构，路线见 `docs/cn/engineering_architecture.md` §14.2、`docs/cn/wasm_codegen.md`。API 与语言表面仍可能变化。

## 两层系统

| 层 | 性质 | 职责 |
| --- | --- | --- |
| **启发式探索层** | 非确定、可分叉、可失败 | 让 LLM 在受控的 Development Graph 上提出候选方案，保留版本与失败路径 |
| **确定性编译层** | 确定、可复现、可测试 | 解析、检查、审计、物化与运行正式 `.sophia` 源码 |

两个铁律：

1. 探索过程可以非确定，**正式源码与编译结果必须确定**；
2. **编译器不调用 LLM**——所有 LLM 调用只发生在工作流层，语言核心保持纯确定性。

## 编译管线（v0）

```
源码 (.sophia)
  → AST            (core/syntax，Tree-sitter)
  → HIR            (core/hir，名称解析 / 模块解析 / Task Closure)
  → Semantic IR    (core/semantic，type / effect / contract 三层)
  → Execution Graph IR (core/exec-ir)
  → 解释器          (runtime，v0 唯一执行后端)
```

## 工作区结构

严格分层：`core/*` 零 IO、不依赖 `workflow/*`。

| 路径 | 职责 |
| --- | --- |
| `core/syntax` | Tree-sitter grammar、CST、AST、span |
| `core/hir` | 名称解析、ASG index、Task Closure / Semantic Paging |
| `core/semantic` | type / effect / contract 三层语义分析 |
| `core/exec-ir` | Execution Graph IR |
| `runtime` | 解释器、EffectHost、input/output validation、执行 Trace |
| `tools/check` | 静态检查器（语法 + 语义 + strip-assist 等价门禁） |
| `tools/audit` | 约束审计 / regression gate |
| `tools/materialize` | Materialize Gate 类型状态链 + 原子写盘 |
| `workflow/graph-db` | Development Graph 持久化（SQLite + 事件溯源） |
| `workflow/llm` | LLM 后端抽象（OpenAI 兼容 / Ollama）+ 结构化输出 |
| `workflow/prompt` | Prompt 模板与 JSON Schema 管理 |
| `workflow/engine` | 工作流编排（调度器 spine + 目标树遍历层） |
| `lsp` | Language Server（hover / diagnostics / goto definition） |
| `cli` | `sophia` 命令行入口（IO 与呈现的归属层） |

## 快速上手

前置条件与详细步骤见 [INSTALL-CN.md](INSTALL-CN.md)。

```bash
# 构建并运行测试
cargo build --workspace
cargo test --workspace

# 创建一个项目骨架
cargo run -p sophia-cli -- init my-project

# 静态检查与解释执行
cargo run -p sophia-cli -- check --root my-project
cargo run -p sophia-cli -- run <ActionName> --root my-project --arg int:41
```

### 常用 CLI 命令

确定性命令（不调用 LLM）：`init` / `parse` / `index` / `check` / `build` / `run`（含 `--trace`）/ `context` / `smoke` / `repair-context` / `graph`（工作流子命令）/ `lsp`。

LLM 命令（经 `--model` / `--mode` 构造后端）：`graph design` / `graph implement-loop`。

完整命令表见 `docs/cn/engineering_architecture.md` 第九节。

### 真实 LLM 测试与基准（可选）

两套真实 LLM 入口都是 `example`（**不进** `cargo test` 门禁，无 API key 时干净跳过）：

- **e2e**（`cargo run -p sophia-cli --example e2e`）：验证 Sophia v0 闭环端到端可用。见 `docs/cn/e2e_test.md`。
- **benchmark**（`cargo run -p sophia-cli --example benchmark`）：横向对比「LLM 直接写 Python」与「Sophia 工作流」在多组小题上的**成功率 + 耗时**。`baseline` mode 需 `python3`（缺失则只跑 `sophia`，`python3` 仅运行期外部工具、不进 Cargo 依赖树）。见 `docs/cn/benchmark_test.md`。

## 文档

新读者建议从概念导览开始：

- **`docs/cn/concepts.md` — 概念导览（先读这篇）**：用图表讲清两层系统、三个"graph"、`.pseudo`/`.sophia` 两阶段、action/transition/effect/capability 的关系
- `docs/cn/language_design.md` — 语言与工作流概念、设计决策（面向 LLM 的"大语言"层）
- `docs/cn/language_implementation.md` — 编译器 / 运行时实现（AST、IR、类型推导、检查器流水线）
- `docs/cn/engineering_architecture.md` — 工具链、目录结构、CLI
- `docs/cn/workflow_graph_spec.md` — Development Graph schema 与不变量（SSOT）
- `docs/cn/dev_checklist_v1.md` — 工程进展（当前 SSOT，v1）；含 v1 需求 / 语言 / 标准库扩展计划。`docs/cn/dev_checklist_v0.md` — v0 阶段归档（只读）
- `docs/cn/engineering_notes.md` — 工程决策日志
- 测试指南（三类测试）：`docs/cn/unit_test.md`（单元测试：进 `cargo test` 门禁、确定性、唯一可 mock）、`docs/cn/e2e_test.md`（端到端：真实 LLM + 真实 IO、禁 mock）、`docs/cn/benchmark_test.md`（基准：Sophia 工作流 vs 直接写 Python 的成功率 / 耗时对比、禁 mock）
- v1 特性设计文档：`docs/cn/type_system.md`（F1 类型语法统一 `one of` / `list of`）、`docs/cn/wasm_codegen.md`（工作流 A：WASM codegen 设计评审）
- 库文档：`docs/cn/stdlib_design.md`（库设计：清单驱动插件模型 / 标准库 + 三方库统一 / 「I/O = 库」边界 / 提示词脚手架）、`docs/cn/stdlib_implementation.md`（库实现：`sophia-library` 注册表 + `sophia-stdlib` 内容 + 路线 B host 注入）、`docs/cn/http_lib.md`（`Http` 库）、`docs/cn/file_lib.md`（`File` 库）

贡献流程与代码规范见 [CONTRIBUTING-CN.md](CONTRIBUTING-CN.md)。

## 许可

本项目以 MIT License 授权，见 [LICENSE](LICENSE)。

除非另有明确声明，你有意提交并被纳入本项目的贡献将以 MIT License 授权，无附加条款。
