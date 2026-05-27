# Sophia 启发式工作流

本文档定义 Sophia 的 LLM 编程工作流。它不是 Sophia-Core 的语言语义，而是围绕 Sophia-Core 构建的探索、生成、检查、修复、选择和物化协议。

核心边界：探索过程可以非确定，正式源码和编译结果必须确定。LLM 负责三项不可替代的启发式能力：生成结构化 `.pseudo`、把 `.pseudo` 实现为可通过确定性检查的 `.sophia` 候选源码、进行探索图的下一步节点动作选择。LLM 也可以参与 goal 分析和 repair；LLM 不参与 `sophia check`、`build`、`run`、constraint audit、artifact diff 或 materialize preflight 的正确性判断。

## 1. 两层系统

Sophia 系统分为两层：

| 层           | 性质                     | 产物                                                                                              | 职责                                                  |
| ------------ | ------------------------ | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| 启发式探索层 | 非确定性、可分叉、可失败 | GoalNode、DecisionNode、PseudocodeNode、CodeNode、CheckResultNode、SelectionNode、MaterializeNode | 让 LLM 在受控空间中提出候选方案，并保留版本和失败路径 |
| 确定性编译层 | 确定性、可复现、可测试   | `.sophia`、ASG index、diagnostics、TypeScript artifact、runtime result                            | 解析、检查、审计、生成、构建和运行正式源码            |

因此，Sophia 不是“让 LLM 直接写最终代码”，也不是用确定性规则放弃 LLM 编程能力。Sophia 的目标是把 LLM 的探索过程从对话历史中抽出来，变成可审计、可回放、可裁剪上下文的图结构。

## 2. 两阶段编程

本地或代码预训练较弱的模型通常可以理解任务语义，但容易一次性输出错误正式代码：漏 effect、漏 capability、漏 errors、发明语法、命名不一致，或把自然语言塞进 body。

Sophia 使用两阶段：

```text
用户目标
  -> .pseudo          # 结构化伪代码，不可执行，不可编译
  -> .sophia          # 确定性 Sophia-Core 候选源码
  -> sophia check     # 确定性检查，不调用 LLM
  -> repair / revise  # 生成新节点，不改历史节点
  -> select
  -> materialize
  -> build / run
```

`.pseudo` 是从需求到 `.sophia` 的中间语义层。它可以使用 JSON 结构承载内容，但内容必须是算法伪代码：写清步骤、分支、循环、状态更新、输出和约束；它不承担完整类型系统、capability/effect、error algebra、正式 body 语法或任何 Sophia 语法。

两阶段编程仍然依赖 LLM 的语义能力：`.pseudo` 由 LLM 设计或修订，`.sophia` 候选源码由 LLM 根据 `.pseudo`、上下文和 scaffold 实现。确定性检查只负责发现结构、类型、能力边界和可构建性问题，不负责发明任务逻辑。

铁律：

- `.pseudo` 可以指导 `.sophia` 生成，但不能直接执行、编译或作为最终行为依据。
- `.sophia` 是唯一进入 checker、build 和 runtime 的源码。
- `.pseudo -> .sophia` 是 LLM-assisted implementation，不是编译。
- 如果 `.pseudo` 缺少关键逻辑，implementation 阶段必须要求 revise，而不是猜测。
- 如果 `.sophia` 与 `.pseudo` 预期不一致，以 `.sophia` 为语义源，并生成诊断或 audit failure。

## 3. `.pseudo` 契约

`.pseudo` 是结构化伪代码，不是自由散文，也不是自定义编程语法。它的目标是让 LLM 先稳定任务逻辑，尤其是循环、分支、状态更新和预期行为。JSON 结构是允许的，因为它只是承载算法计划的容器；JSON 里面的内容不能写成 Sophia 代码或类似 `program { ... }` 的伪 DSL。

必须写清：

- 输入与输出语义。
- record-like 数据的语义定义，当任务需要结构化对象时，用普通语言描述字段含义和形状。`.pseudo` 不承载 scaffold contract，也不写显式 v0 类型；公开 scaffold contract 由 workflow 在 implementation 阶段单独提供。
- 算法步骤。
- 循环次数或循环条件。
- 分支条件。
- 变量或状态如何更新。
- 明确的逻辑步骤边界，当任务需要验证、更新、编排或复用步骤时。
- 副作用意图，例如打印。
- 禁止事项。
- 预期输出或关键验收条件。

最小结构：

```json
{
  "purpose": "任务目标",
  "inputs": [{ "name": "input_name", "meaning": "输入语义" }],
  "outputs": [{ "name": "result", "meaning": "输出语义" }],
  "definitions": [{ "name": "RecordLikeConcept", "meaning": "普通语言描述字段含义和形状" }],
  "algorithm": [
    "create empty list result",
    "repeat N times: set next to the next computed value, append next to result",
    "return result"
  ],
  "effects": ["可观察输出：打印每个值"],
  "constraints": ["实现必须保留的语义约束"],
  "forbidden": ["不得引入的行为"]
}
```

不应放入 `.pseudo` 的内容：

- 完整 Sophia-Core 类型签名。
- 正式 Sophia effect 名称，例如 `Console.Write`、`DB.Write("Todos")`。
- scaffold contract、文件路径、capability 绑定或 implementation hints。
- 完整 capability 和 error algebra。
- 正式 action body。
- `program { ... }`、`subaction { ... }`、`main_flow { ... }` 这类伪 DSL 或任何类似程序代码的结构。
- “handle properly”、“do the calculation”、“process safely” 这类模糊句子。
- 依赖隐藏 verifier expected output 的硬编码提示。

`pseudocode_check` 不证明正确性，只判断该 `.pseudo` 是否足够清楚，能否安全进入 implementation。

## 4. Implementation 规则

`.pseudo -> .sophia` 由 LLM 完成，因此是非确定性的。implementation 输出必须是候选文件集合，而不是单段聊天文本。确定性 scaffold 只固定文件路径、命名、公开 override、显式 v0 类型签名、显式 state/effect contract 和可验证结构，降低 LLM 负荷；它不能通过关键词从自然语言描述中决定类型或业务 contract，也不能替代 LLM 把算法语义写成可编译 Sophia-Core body。

规则：

- 每个 algorithm step 必须转换为 Sophia-Core body 中的确定性语句；implementation 不能修改、替换或补写新算法。
- `repeat`、`if`、`return`、`print` 等结构必须保留为正式语法，不得复制自然语言进 body。
- inputs/outputs 必须在 `.sophia` 中补齐类型。
- effects 必须根据 `.pseudo` 的语义意图补齐为 formal effects，例如将“打印”降为 `Console.Write`；`.pseudo` 本身不写 formal effect。
- 公开 scaffold contract 可以要求生成 state 文件和值集合，但不能生成业务 `match` body。
- scaffold placeholder 不是 contract。若 `structure_plan.action_contract_hints` 没有显式输入、输出或字段类型，LLM 必须根据 `.pseudo` 语义自行实现，checker/verifier 再判断结果。
- `.pseudo definitions` 中的 record-like 语义定义应在需要时 implementation 为正式 `entity` 文件。
- `.pseudo algorithm` 中明确命名的逻辑步骤边界表示应保留的 helper action 边界；对应 helper action 应存在，并由主 action 显式调用。
- forbidden 应转换为 capability deny、audit constraint 或 implementation 检查约束。
- expected 应转换为测试、verifier 或 audit 条件，不直接改变程序行为。

implementation 后必须立即创建 CodeNode，并运行 `graph check` / `graph verify`。未经确定性 gate 的 CodeNode 不能 materialize。

## 5. 探索图

线性流程适合小任务：

```text
goal -> pseudocode -> code -> check -> repair
```

大型或不确定任务容易卡死在错误路径上，因此 Sophia 使用 append-only Development Graph：

```text
GoalNode
  -> DecisionNode
      -> PseudocodeNode
      -> CodeNode
      -> CheckResultNode
      -> RepairCode -> CodeNode(v+1)
      -> ReviseDesign -> PseudocodeNode(v+1)
      -> Backtrack -> ancestor / sibling node

Accepted CodeNode
  -> SelectionNode
  -> MaterializeNode
  -> domains/<Domain>/...
```

图规则：

- 节点不可变。修改必须产生新节点。
- 失败路径不删除，只标记为 failed、abandoned 或 superseded。
- `revise_design`、`repair_code`、`merge` 都必须生成新节点，并通过边连接来源节点。
- 选择由 SelectionNode 表达。
- 物化由 MaterializeNode 表达。
- `domains/` 只保存已选中且通过 gate 的正式源码。
- `sophia-runs/graph/` 保存探索过程。

## 6. 节点与动作

核心节点类型：

| 节点类型        | 含义                                                       |
| --------------- | ---------------------------------------------------------- |
| GoalNode        | 用户目标或子目标                                           |
| DecisionNode    | 动作选择、状态评估和理由                                   |
| PseudocodeNode  | `.pseudo` 结构化伪代码版本                                 |
| CodeNode        | `.sophia` 候选文件集合                                     |
| CheckResultNode | 确定性检查结果                                             |
| AuditNode       | constraint、capability、artifact diff 或 strip-assist 结果 |
| SelectionNode   | 被选中的候选                                               |
| MaterializeNode | 写入正式源码目录的事件                                     |

核心动作：

| 动作               | 用途                                |
| ------------------ | ----------------------------------- |
| `design_solution`  | 先写结构化 `.pseudo`                |
| `implement_design` | 把 `.pseudo` 转换为 `.sophia`       |
| `repair_code`      | 根据结构化 diagnostics 修复候选代码 |
| `revise_design`    | 当错误反映概念问题时重写伪代码      |
| `decompose`        | 目标过大时拆成子目标                |
| `backtrack`        | 当前路径超预算或违反父约束时回退    |
| `select`           | 选择通过 gate 的候选                |
| `materialize`      | 把选中候选写入 `domains/`           |

动作选择和动作执行必须分离：先生成 DecisionNode，再执行动作。DecisionNode 记录允许动作、评分、理由和选择结果；执行动作产生新的 graph 节点。

## 7. 决策策略

LLM 决策必须基于 action-space scaffold，而不是自由聊天。prompt 只提供当前节点摘要、祖先链、相关诊断、预算和 action-rooted semantic context，不提供 validation-only hidden expected output。scaffold 的职责是缩小安全动作空间、降低无关记忆负担并提供可校验 JSON 形状；它不负责替 LLM 选择下一步。

状态评估字段：

```text
state_assessment {
  goal_size: tiny | small | medium | large
  logic_clarity: low | medium | high
  has_pseudocode: true | false
  has_code: true | false
  compile_status: not_checked | pass | fail
  error_type: none | local | conceptual | integration
  repair_attempts: Int
  decomposition_needed: true | false
}
```

LLM 可参考的决策原则：

1. 已有 CodeNode 且 check/verify pass：`select`。
2. 已有 CodeNode 且错误 local 且未超预算：`repair_code`。
3. 已有 CodeNode 且错误 conceptual：`revise_design`。
4. 已有清晰 `.pseudo` 且无 CodeNode：`implement_design`。
5. 无 `.pseudo` 且目标 small/medium：`design_solution`。
6. 目标 large 或跨多个 domain：`decompose`。
7. 超预算或父约束被违反：`backtrack`。

这些原则不是确定性节点选择器。实现可以从图状态生成确定性的 action-space scaffold：它只列出当前节点允许执行的动作、前置条件和一个可复现的 baseline 排序，用来约束 prompt、校验 LLM 输出和做实验对照。真正的节点动作选择必须由 LLM 产生 DecisionNode；只运行 action-space scaffold 或 baseline 的结果不得计入“LLM 能进行启发式节点选择”的实验结论。放弃 LLM 节点选择能力会让 Sophia 退化为固定流程执行器，无法处理信息不足、概念性错误、预算取舍和回退路径。

## 8. 预算与评分

探索必须控制搜索爆炸。

```text
budget {
  max_depth: 6
  max_children_per_node: 3
  max_repair_attempts_per_code_node: 2
  max_pseudocode_versions_per_goal: 3
  max_total_nodes_per_goal: 40
}
```

评分字段：

```text
score {
  compile: 0.0..1.0
  tests: 0.0..1.0
  constraints: 0.0..1.0
  simplicity: 0.0..1.0
  locality: 0.0..1.0
  capability_minimality: 0.0..1.0
  pseudocode_clarity: 0.0..1.0
  overall: weighted_sum
}
```

如果 `compile = 0`，`overall` 最高不得超过 `0.49`，防止语义上看似合理但不可编译的候选被选中。

## 9. Materialize Gate

`graph materialize` 是唯一可以把候选 `.sophia` 写入 `domains/` 的 graph 命令。

必须满足：

- 候选 CodeNode 已被 SelectionNode 选中。
- 最近的 CheckResultNode 为 pass。
- Constraint audit 通过。
- Strip-assist / artifact diff gate 通过。
- 候选 TypeScript build 和 generated strict typecheck preflight 通过。
- runtime input/output validation 与 hidden verifier 不泄漏 prompt。

materialize 必须是原子操作：先写临时目录，preflight 通过后再替换目标文件。

## 10. 当前 CLI 映射

常用命令：

```bash
node dist/cli/main.js graph init
node dist/cli/main.js graph start "目标"
node dist/cli/main.js graph design N0001 --model qwen3.6:latest
node dist/cli/main.js graph pseudo-check fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-outline fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-scaffold fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph implement-loop N0002 --model qwen3.6:latest --max-repairs 2
node dist/cli/main.js graph check N0005
node dist/cli/main.js graph audit N0005
node dist/cli/main.js graph diff N0005
node dist/cli/main.js graph verify N0005
node dist/cli/main.js graph select N0005
node dist/cli/main.js graph materialize N0005
```

独立确定性命令：

```bash
node dist/cli/main.js check
node dist/cli/main.js index
node dist/cli/main.js context --action ActionName
node dist/cli/main.js build
node dist/cli/main.js smoke
node dist/cli/main.js run ActionName
```

如果 Ollama 未运行，依赖 LLM 的命令必须显式失败并保留失败产物，不得伪造成功 CodeNode。

## 11. 与其他文档的关系

- 当前实现状态见 `status.md`。
- Sophia-Core 语言语义见 `sophia_language_design.md`。
- 后续路线图见 `roadmap.md`。
- 诊断码约定见 `diagnostic_codes.md`。

本文档是当前有效工作流规范。旧实验日志和旧实现计划保留在 `docs/archive/`，不作为事实源。
