# Sophia

Sophia 是一门面向无人监管 LLM 自动编程的 LLM-native 编程语言和工作流。

它探索一个设计前提：LLM 和人类有不同的优势与弱点，因此为 LLM 编程设计的语言不应简单复用人类优先的语言惯例。Sophia 把编程纪律外部化为带类型的语义产物、确定性检查、capability/effect 边界、intent type、action-rooted context 和版本化修复门禁。

当前工作流：

```text
goal -> LLM decision -> .pseudo -> LLM implementation -> deterministic gates -> LLM repair/decision -> materialize -> build -> run
```

当前实现是 v0.2 TypeScript CLI 原型。端到端实验中不伪造成功路径。

LLM 在工作流中负责生成结构化伪代码、生成可检查的 Sophia 候选源码、以及做探索图的启发式节点选择。scaffold、diagnostics 和 gates 只提供约束、上下文和验证，不能替代这些 LLM 能力。

## 文档

- [当前状态](docs/cn/status.md)：当前已实现能力、验证状态和已知限制。
- [语言设计](docs/cn/sophia_language_design.md)：Sophia-Core 语义和 v0.2 边界。
- [启发式工作流](docs/cn/heuristic_workflow.md)：`.pseudo`、探索图、LLM decision、repair 和 materialize gate 的工作流规范。
- [路线图](docs/cn/roadmap.md)：唯一有效的当前路线图和研究方向。
- [诊断码](docs/cn/diagnostic_codes.md)：parser / checker / build / run 诊断码参考。
- [Benchmark 命令](docs/cn/benchmark_runs.md)：可复现 benchmark 命令示例。

历史实施计划、旧路线图、实验日志和论文草稿已归档到 `docs/archive/`。

## 环境

本地验证过的工具链：

```text
Node.js >= 26
npm >= 11
本地安装 Ollama
```

安装依赖：

```bash
npm install
```

## 核心命令

```bash
npm run typecheck
npm test
npm run build
```

从源码运行 CLI：

```bash
npm run dev -- --help
```

运行已构建 CLI：

```bash
node dist/cli/main.js --help
```

初始化 workspace：

```bash
node dist/cli/main.js init
```

## 常用 CLI 流程

初始化并使用探索图：

```bash
node dist/cli/main.js graph init
node dist/cli/main.js graph start "计算、打印并返回前十个兔子数。"
node dist/cli/main.js graph design N0001 --model qwen3.6:latest
node dist/cli/main.js graph implement-loop N0002 --model qwen3.6:latest --max-repairs 2
node dist/cli/main.js graph check N0005
node dist/cli/main.js graph audit N0005
node dist/cli/main.js graph diff N0005
node dist/cli/main.js graph verify N0005
node dist/cli/main.js graph select N0005
node dist/cli/main.js graph materialize N0005
```

检查、索引、生成上下文、构建并运行已 materialize 的 Sophia 源码：

```bash
node dist/cli/main.js check
node dist/cli/main.js index
node dist/cli/main.js context --action SumFirstFive
node dist/cli/main.js build
node dist/cli/main.js smoke
node dist/cli/main.js run SumFirstFive
node dist/cli/main.js run DoubleInput --input '{"count":7}'
```

检查 pseudo 与 repair 产物：

```bash
node dist/cli/main.js graph pseudo-check fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-outline fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-scaffold fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js repair-context N0006
node dist/cli/main.js graph report
```

如果 Ollama 未运行，依赖 LLM 的命令会显式失败，并创建失败的 RawLlmNode 产物，而不是伪造成功的 CodeNode。

## 基准测试

运行一个防泄漏 benchmark verifier：

```bash
node dist/cli/main.js experiment list --suite benchmarks/category_a
node dist/cli/main.js experiment verify --task benchmarks/category_a/account_pipeline/task.json
node dist/cli/main.js experiment run --task benchmarks/category_a/account_pipeline/task.json --model qwen3.6:latest --mode full --max-design-revisions 2 --max-repairs 2 --ollama-timeout-ms 900000 --out sophia-runs/results/account-pipeline-full.jsonl
```

`experiment run` 有意只接受一个 `--task`；hidden verifier cases 保存在 `task.json` 中，不进入模型 prompt。串行 suite 运行见 [docs/cn/benchmark_runs.md](docs/cn/benchmark_runs.md)。
