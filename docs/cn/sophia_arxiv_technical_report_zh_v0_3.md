# Sophia：代码预训练之外的 LLM-native 图编程路径（v0.3）

**技术报告 v0.3 版**  
**日期：2026-05-28**  
**状态：early technical report / working prototype report**

---

## 摘要

当前主流 AI 编程路径依赖大量代码库预训练。Sophia 探索一条互补路线：把编程能力部分外部化到一种 LLM-native 图编程语言（Sophia-Core）与启发式节点工作流上，让具备较强通用语义理解、但代码预训练较弱的模型，仍可在外骨骼式工具链辅助下完成有用的自动编程任务。

v0.3 在 v0.2 的语言与工具基底之上，引入“目标图工作流（Goal Graph Workflow）”：将目标、阶段、变更与验收过程以图节点表示，并将其材料化（materialize）为可回放与可审计的记录。与此同时，基准套件重整为按难度与测试特征分组的 L1–L5 梯度（移除旧的 category_a），并将 L4/L5 的 workflow 场景语义转化为可纳入统一基准套件的单任务契约。本文不以基准结果为中心论证点；benchmark 仅用于可运行性与语言面覆盖的记录。本报告的 benchmark 结果留空，后续统一在运行记录中维护。

---

## 1. 引言

大量代码库预训练是当下最快、最高效的 AI 编程路径。但“编程能力”不必完全内化为模型参数。Sophia 的问题设定是：若编程主体是 LLM，我们是否应继续沿用为人类设计的线性源码范式，还是可以设计一种更适合 LLM 理解与操控的图编程语言与工作流，让编程能力部分外部化，并以确定性工具作语义裁判？

---

## 2. 核心定位

- Sophia 探索“强语义但弱代码模型”在外部化语言与工具链上的编程能力。
- 以 ASG node + append-only development graph 为主载体，而非人类线性源码为主。
- 自然语言目标下沉为结构化 `.pseudo`，再下沉为可检查的 Sophia-Core。
- LLM 负责理解、分解、结构化表达、候选实现与启发式节点选择；checker/compiler/audit/build 负责裁判。

---

## 3. 设计原则

- LLM-native，而非 human-first。
- 自然语言仅作语义辅助，不决定程序语义（strip-assist 等价）。
- Formal Core 必须确定、可检查、可复现。
- 边界显式化：输入/输出/副作用/能力/错误/存储意图/调用关系均为语言事实。
- 程序是图，而非聊天记录；失败路径保留，可回放与审计。
- 启发式节点工作流，而非固定单向流水线。
- 特性纳入以“机器可证价值”为准。

---

## 4. v0.3 边界与新增（相对 v0.2）

- 目标图工作流（Goal Graph Workflow）
  - 节点与操作：`ObjectiveNode`、`MilestoneNode`、`ChangeRequestNode`、`ImpactAnalysisNode`、`AcceptanceNode`，以及分解/接受/失效/重新拆解/阶段激活/变更记录/影响分析/变更接受/验收记录等操作。
  - Active context：`buildGoalContext` 计算当前有效目标、阶段、已接受变更、out-of-scope 与 regression constraints，输入到决策 prompt。
  - 材料化：目标图的演化轨迹可被完整回放与审计。
- 基准套件统一分组（L1–L5）
  - L1：线性纯函数任务（basic_pure）。
  - L2：单循环/列表/副作用（list/loop/effect）。
  - L3：分支/match/optional/state，以及 orchestration/pipeline 的纯任务。
  - L4：目标/流程语义转换为单任务契约（goal_workflow_translation）。
  - L5：变更应用类单任务契约（change_application）。
  - 旧的 `category_a` 已移除，其任务并入 L3（orchestration/pipeline）。
- 场景语义到单任务契约的桥接
  - 将 L4/L5 的 workflow 场景（scenario.json）对应的关键语义，转化为统一套件下的基准任务（task.json），避免分裂统计口径。
- 工程与一致性
  - 版本升级至 v0.3（package.json、CLI 显示、默认 sophia.toml 模板一致）。
  - 代码清理与可读性提升：最小 TOML 解析抽取、未使用导入清理、公共替换策略梳理、生成器命名与临时变量一致性等。

---

## 5. `.pseudo` 与 `.sophia` 的边界

- `.pseudo`：结构化语义草图，承载任务意图、输入输出语义、算法步骤、循环/分支条件、状态更新、副作用意图、禁止事项与验收条件，不包含 formal type/effect/capability/错误代数/源码路径/实现标签/伪 DSL 模板。
- `.sophia`：唯一可编译语义源，承载类型、action、capability、effects、errors、body 与 ASG edges。
- `.pseudo -> .sophia` 为 LLM 辅助实现；不一致时以 `.sophia` 为准。
- v0.3 延续 v0.2 的关键修正：移除 design 阶段 formal syntax 污染；structure plan 仅固定显式 contract 与正式结构，不生成业务算法。

---

## 6. ASG 与 Action-Rooted / Goal-Rooted Context

- v0.2 的 action-rooted 语义闭包：从 action 出发确定相关实体/状态/能力/副作用/调用/错误传播等闭包，供 LLM 消费与工具检查。
- v0.3 在此基础上引入 goal-rooted active context：面向目标图的当前阶段、约束与回溯信息，向决策节点暴露裁剪后的上下文。

---

## 7. 外部化编程纪律示例：Intent Types（延续 v0.2）

- `Raw/Parsed/Validated/Sanitized/Verified/Authorized/Persisted/Secret/Redacted` 等 intent wrapper 的赋值与转换规则外部化，静态拒绝常见的“方便性越权”与信息流违规（例如 Secret 直出 Console、Raw 入 Sanitized storage）。

---

## 8. Capability / Effect / Error（延续 v0.2）

- action 声明 effects，绑定 capability；effect 必须被 allow，deny 覆盖 allow；调用传播被调用方 effects；`raise` 与 propagated error 一致。

---

## 9. Strip-Assist 等价（延续 v0.2）

- 自然语言辅助字段的剥离不改变 Formal Core 与生成工件（当前以 TypeScript artifact 等价近似；未来扩展为 IR hash）。

---

## 10. Development Graph（v0.3 扩展）

- 节点涵盖设计/实现/检查/修复/审计/选择/物化，以及 v0.3 的目标/变更类节点。
- 节点不可变、失败路径保留、可回放；支持对 LLM 决策与演化边界的后验分析与门禁（materialize gate）。

---

## 11. 基准与评估（本报告暂不列结果）

- 目的：证明原型“可运行、可记录、可复现”，并覆盖最小语言面与 v0.3 的 workflow 桥接能力；不是证明“基准胜过 direct-ts”。
- 任务分组：L1–L5（见第 4 节）。
- 运行方式：`experiment run --task ...` 为单任务基准；`experiment run-suite --suite benchmarks` 为串行套件；`--mode` 可切换 full / direct-ts，workflow 场景语义以单任务契约纳入统一统计。
- 结果：留空（待运行记录统一填充 JSONL 与 summary）。

---

## 12. 相关工作定位（与 v0.2 一致，略）

- Coding agents、代码预训练、flow engineering、program synthesis / sketching、effect/refinement types、结构化输出框架等方向的互补定位。

---

## 13. 局限（v0.3）

- benchmark 规模仍小，且不作为核心价值证明；
- `task` / `transition` / Evolution Boundary / Semantic Identity 等仍在演进；
- body-level storage read / runtime API 未完备；
- intent 信息流的跨域追踪与 formal IR 等价仍在推进；
- 目标图工作流聚焦于“可回放/可审计/可裁剪”能力，不承诺复杂产品级项目管理与协作。

---

## 14. 后续路线（v0.3 之后）

- Adversarial intent safety suite：将安全矩阵转化为对抗样本，验证静态拒绝能力；
- Edit transitions 与 Evolution Boundary：把编辑演化作为一等节点（新增字段、intent 收紧、错误扩展、action 拆分等）；
- 跨 domain/library 的 publish/consume，与 `sophia.lock` 与 formal-only 视图；
- 更强 strip-assist 等价：IR hash / formal-only hash。

---

## 15. 结论

Sophia v0.3 延续 v0.2 的主张：编程能力不必完全由代码预训练内化，亦可由 LLM 与外部化的图语言/工作流协作完成。v0.3 的目标图工作流把“目标—阶段—变更—验收”的真实开发节律纳入可回放与可审计的语义结构；基准套件统一分组与场景桥接，保证评估口径一致。下一步工作将围绕对抗式安全验证、编辑演化边界与跨域语义一致性推进，把“语言与工作流的机器可证价值”做实。
