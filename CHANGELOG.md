# 变更日志

本文件记录面向用户的重要变更。格式参考 [Keep a Changelog](https://keepachangelog.com/)，
版本遵循 [语义化版本](https://semver.org/)。

> 详细的工程级进展与逐项变更记录见 `docs/dev_checklist_v1.md`（当前）/ `docs/dev_checklist_v0.md`（v0 归档）。

## [Unreleased]

### Changed
- **类型系统统一（F1）**：可失败 / 可空返回从设想的 `Result<T,E>` 改为 `one of {...}` 联合类型（成员直接构造、直接 match，无 `Ok`/`Err`/`Some`/`None` 包装子）；统一全部类型语法——`<>` 专属 Intent Type，结构类型用 `of` 关键字族（`list of T` / `one of { M, ... }` / `schema of T`）。废弃 `Optional<T>` / `List<T>` / `Schema<T>` / `Some` / `None` / `<optional>.exists`；新增 `Null` 内置类型与 match 类型 pattern。设计见 `docs/type_system.md`。

### Added
- **`Http` 内置 effect 族（F2）**：`Http.Get(url) -> Raw<Text>`，与 `Console`/`DB` 同构（零新语法）；不可信网络数据经 intent 边界静态管控。设计见 `docs/http_lib.md`。
- **HTTP 客户端真实 host（S1）**：CLI 协调层基于 `reqwest::blocking` 的 `Http.Get` 真实网络实现（runtime 保持零 IO）；按入口 effect 含 `Http.Get` 才注入。设计见 `docs/http_lib.md`。
- **标准库提示词脚手架（S2）**：按需取用的库 prompt 资产（`assets/stdlib/<lib>.md`，首份 `http`）——design 阶段看库目录自选、implement 阶段注入完整用法。设计见 `docs/stdlib_design.md` / `docs/stdlib_implementation.md`。
- 目标树遍历层 `run_goal_tree` 的人类授权检查点（`DecompositionReviewer`）：接受 `Decomposition` 后子目标沿 `member_of` 继承 binding。
- 端到端测试用例组 G5（storage 持久化）、G6（目标树遍历 decompose）。
- append-only / I9 不变量的 CI 守护测试。

### Removed
- 移除 agent 编排方向：`node` 顶层构造、`Llm`/`Tool`/`Stream` 内置 effect 族、五个内置节点（prompt/router/aggregator/tool/stream）、单 node 解释执行，以及整个 `sophia-stdlib` crate。理由：偏离语言定位（LLM 是程序员，非程序内置能力），属经 stdlib 侧门引入的过度设计。`effect` 顶层构造与 `Family.Op(args)` 通用引用**保留**（消除 grammar 硬编码 effect、可声明领域 effect 的独立成果）。

## [0.1.0]

首个公开基线：**v0 解释执行**（源码 → AST → HIR → Semantic IR → Execution Graph IR → 解释器，无 codegen）。

### Added
- **语法层**：Sophia-Core Tree-sitter grammar（9 类顶层节点 + body 子语言）、CST / AST、span。
- **HIR**：名称解析、ASG index、Task Closure / Semantic Paging。
- **语义 IR**：type / effect / contract 三层分析；strip-assist 等价门禁。
- **Execution Graph IR** 与**解释器**：跑通起步子集 + storage body 操作；runtime input/output validation；执行 Trace 投影。
- **`effect` 顶层构造**：`effect Family { operation Op {...} }` 声明 effect 族 + `Family.Op(args)` 通用引用（消除 grammar 硬编码 effect；内置 Console/DB，用户可声明领域 effect）。
- **Development Graph**：SQLite + 事件溯源持久化、节点 / 边 schema 与不变量、Active Context 推导。
- **工作流引擎**：LLM 抽象（OpenAI 兼容 / Ollama）+ 结构化输出、Prompt 模板、调度器 spine + 目标树遍历、多候选评分排序。
- **工具链**：`tools/check`（静态检查）、`tools/audit`（约束审计 / regression gate + hidden-case 执行器）、`tools/materialize`（Gate 类型状态链 + 原子写盘）。
- **Language Server**：hover / diagnostics / goto definition。
- **CLI** `sophia`：`init` / `parse` / `index` / `check` / `build` / `run`（含 `--trace`）/ `context` / `smoke` / `repair-context` / `graph` 工作流子命令 / `lsp`。

[Unreleased]: https://example.invalid/sophia/compare/v0.1.0...HEAD
[0.1.0]: https://example.invalid/sophia/releases/tag/v0.1.0
