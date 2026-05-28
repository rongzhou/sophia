# 基准测试运行命令

本项目把 `experiment run --task <task.json>` 作为每个 benchmark 结果的来源。Suite 命令只是串行便利封装：它加载一个 suite，逐个 task 走同一条 single-task 代码路径，写入 JSONL 记录，并在失败后继续执行后续 task，从而暴露完整回归面。

## 当前 Benchmark 布局

- `benchmarks/L1`：没有循环或分支的线性任务。
- `benchmarks/L2`：单循环、列表和 effect 任务。
- `benchmarks/L3`：分支和 `match` 任务，包括 effect、pure-return、`Optional` 和 `state` 变体。
- `benchmarks/L4`：目标/流程语义转换为单任务契约。
- `benchmarks/L5`：变更应用类单任务契约。
- `benchmarks/category_a`：跨 action / entity pipeline 任务。

列出已注册任务：

```bash
node dist/cli/main.js experiment list --suite benchmarks
```

## 单任务运行

调试单个失败或生成论文级 task record 时使用：

```bash
node dist/cli/main.js experiment run \
  --task benchmarks/L2/rabbit_ten/task.json \
  --model qwen3.6:latest \
  --mode full \
  --max-design-revisions 2 \
  --max-repairs 2 \
  --ollama-timeout-ms 900000 \
  --out sophia-runs/results/rabbit-ten-full.jsonl
```

汇总一个或多个 JSONL 文件：

```bash
node dist/cli/main.js experiment summarize \
  --inputs sophia-runs/results/rabbit-ten-full.jsonl
```

## 串行 Suite

串行运行 `benchmarks/` 下所有 benchmark：

```bash
node dist/cli/main.js experiment run-suite \
  --suite benchmarks \
  --model qwen3.6:latest \
  --mode full \
  --max-design-revisions 2 \
  --max-repairs 2 \
  --ollama-timeout-ms 900000 \
  --out-dir sophia-runs/results/all-benchmarks-full
```

Suite runner 会写出：

- `results.jsonl`：每个 task run 一条记录，包含 `design_revisions_used`、`repairs_used`、workspace paths 和 hidden verification 状态。
- `summary.md`：与 `experiment summarize` 相同的汇总表。

默认情况下，`experiment run` 和 `experiment run-suite` 会覆盖目标 JSONL 文件，避免历史记录污染本次结果。只有显式传入 `--append` 时才追加写入。

如果任一 task 失败，命令最终以非零状态退出，但会先尝试完整个 suite。

## Direct TypeScript Baseline

使用同一组命令并加上 `--mode direct-ts` 运行 direct TypeScript baseline：

```bash
node dist/cli/main.js experiment run-suite \
  --suite benchmarks \
  --model qwen3.6:latest \
  --mode direct-ts \
  --ollama-timeout-ms 900000 \
  --out-dir sophia-runs/results/all-benchmarks-direct-ts
```

## 2026-05-26 Full Suite 说明

命令：

```bash
node dist/cli/main.js experiment run-suite \
  --suite benchmarks \
  --model qwen3.6:latest \
  --mode full \
  --max-design-revisions 2 \
  --max-repairs 2 \
  --ollama-timeout-ms 900000 \
  --out-dir sophia-runs/results/all-benchmarks-full
```

基准任务数量会随着合并而变化。使用 `experiment list --suite benchmarks` 查看当前套件，使用 `experiment summarize` 汇总结果与模式对比。

失败分类（节选示例）：

- `first_five_squares`：design 阶段直接失败，错误为 `.pseudo` 含 formal Sophia syntax。根因不是模型“抄错”，而是 public design goal 和 design prompt 仍暴露了 scaffold/formal 语法片段，并且 design prompt 还把 `.pseudo` 写成 `program { ... }` 风格的伪 DSL。这导致 workflow 一边喂 formal 信息，一边禁止 `.pseudo` 写 formal 信息。这是边界错误，已修正为 design goal 只含语义任务和公共约束；design prompt 只要求 JSON 结构承载算法伪代码，不再包含实现标签、scaffold contract、formal type、formal effect、source syntax 或伪 DSL 模板。
- `zero_or_positive_label`：同样在 design 阶段被 formal syntax validator 拒绝，根因同上。已为 design/revise LLM 调用启用 validation retry，但这只是兜底；主要修复是移除 design 输入中的 formal 语法污染。
- `state_status_label`：implementation validation retry 后仍失败，错误为
  `Implementation output state TaskStatus must preserve values Pending, Done.` 这里 state 文件和值集合属于 explicit scaffold contract，可以确定性保留；已改为在 implementation 输出校验时确定性覆盖显式 state scaffold 文件，只保留公开 state contract，不生成或修复业务 `match` body。后续若 body 中仍使用错误 state pattern，应进入 checker/repair，而不是在 JSON validation 阶段直接丢失候选。

修复成功案例：

- `build_three_numbers`：初始候选有 3 个 checker diagnostics，repair 后通过 hidden verifier，属于可修复语法/结构错误，不是最终失败。
- `item_delta_pipeline`：初始 implementation 将 `.pseudo` 的 `if ... then / else / end if` 伪代码分支形式直接复制进 `.sophia` body，checker 报 6 个 `CHECK-BODY-004`。repair 只把分支语法改为 Sophia v0 的 `if condition { ... } else { ... }`，保留 helper action 边界、`Item` entity contract 和 `delta > 0 && item.is_active` 算法，随后 check、audit、4 个 hidden case 全部通过。
- `zero_or_positive`：同样是 `if count == 0 then / else / end if` 被直接复制到正式 body，checker 报 3 个 `CHECK-BODY-004`。repair 只改 body 语法，没有改变 `zero`/`positive` 打印逻辑和 `Console.Write` effect contract，随后 3 个 hidden case 全部通过。
- 手动核对成功样本：`account_pipeline` 保留 validation/update 两个 helper action；`optional_label_default` 使用 exhaustive `Some(value)`/`None` match 且无 catch-all；`zero_or_positive_label` 纯返回分支无 effect；`rabbit_ten` 的 hidden verifier 确认返回序列和打印序列一致。当前可疑点集中在 implementation 偶尔复制伪代码分支语法，已在 implementation prompt 中显式禁止。

边界修复：

- 伪代码生成阶段绝不接收 Sophia type/effect syntax、scaffold contract、source paths、implementation labels 或 implementation hints。benchmark public goal 只保留语义任务和公共约束。
- design prompt 删除了 `count: Int`、`Console.Write`、`implementation_hints`、`program { ... }`、`subaction { ... }`、`main_flow { ... }` 等反向示例；这些即使作为“不要写”的例子，也会污染弱模型输出。
- implementation 阶段仍接收 deterministic structure plan 和 scaffold，因为这里的职责是把 `.pseudo` 降为可编译 Sophia-Core；但 scaffold 只保护显式 contract，不生成业务算法。

后续重跑建议：用修复后的代码至少单独重跑 `first_five_squares`、
`zero_or_positive_label` 和 `state_status_label`，再跑完整 `benchmarks` suite。
