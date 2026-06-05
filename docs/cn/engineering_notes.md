# 工程笔记与决策日志

用途：记录设计文档未完全规定、但需要在工程内长期延续的小型工程决策，防止实现漂移并降低新成员上手成本。

## 约定
- 条目保持精简；优先“更新”而非“重复新增”。
- 每条决策包含：日期、范围、决策、动机（理由）、影响、状态。
- 如影响到公共 API 或跨 crate 的模式，应在 PR/提交信息中引用本条目。

## 全局原则：单一路线，拒绝多路径与向后兼容负担
- 本项目在任何层面均不允许多条实现路径（包括但不限于：可切换的特性开关、双栈实现、临时 fallback 逻辑、历史 API 适配层等）。
- 设计变更一经确认，采取“直接迁移、移除旧路径”的策略，不背负向后兼容成本。
- 暂时性的占位仅用于保证构建运行，不应形成替代路径；占位应位于“唯一代码路径”内，并清晰返回未实现错误，而非提供功能性 fallback。
- 该原则适用于所有 crate（core、workflow、tools、lsp、cli、runtime、stdlib）。
- 违反该原则将优先被拒绝合入；必要时在工程笔记新增条目说明取舍与迁移计划。

## 决策日志

- 2026-05-28 — 错误处理基线
  - 决策：库（lib）crate 采用 `thiserror` 定义类型化错误；二进制（bin，如 `cli`）在需要时可用 `anyhow` 进行应用层聚合。
  - 理由：核心 crate 保持清晰的错误分类；在边界处具备更友好的错误呈现。
  - 影响：相关 crate 引入 `thiserror`；公共 lib API 避免直接暴露 `anyhow::Error`。
  - 状态：Accepted

- 2026-05-28 — SQLite 后端选择
  - 决策：`workflow/graph-db` 使用 `rusqlite`，启用 `bundled` 特性。
  - 理由：零外部系统依赖，API 简洁。
  - 影响：开启 `bundled` 功能；CI/本地构建不依赖系统 SQLite。
  - 状态：Accepted

- 2026-05-28 — 格式化配置
  - 决策：纳入 `rustfmt.toml`（edition=2021，max_width=100，Unix 换行）。
  - 理由：多 crate 统一代码风格。
  - 影响：本地与后续 CI 执行 `cargo fmt` 进行约束。
  - 状态：Accepted

- 2026-05-28 — Git 与分支命名
  - 决策：当前仅本地 git，无远端；默认分支重命名为 `main`。
  - 理由：全新 Rust 工作区；避免与旧 TS 仓库耦合；遵循通用命名。
  - 影响：工作流假定主分支为 `main`。
  - 状态：Accepted

- 2026-05-28 — Workspace 与分层纪律
  - 决策：crate 布局遵循 `engineering_architecture.md`；`core/*` 禁止 IO，且不得依赖 `workflow/*`。
  - 理由：提升可测试性、确定性，并保留 WASM 可能性。
  - 影响：在依赖新增与评审中强制执行。
  - 状态：Accepted

- 2026-05-28 — CLI 范围
  - 决策：使用 `clap`，仅保留最小子命令占位；只接稳定接口（如 `syntax::parse_file`），避免后续大规模改动。
  - 理由：减少占位代码反复与后续重构成本。
  - 影响：子命令随子系统成熟逐步扩展。
  - 状态：Accepted

- 2026-05-28 — 依赖策略
  - 决策：依赖按需引入（just-in-time）；优先精简且维护良好的 crate；跨切面选择在本文记录。
  - 理由：控制工程表面积，避免在语义未稳定前过早绑定。
  - 影响：新增依赖需在 PR 评审中明确，并在此记录形成先例。
  - 状态：Accepted

- 2026-05-28 — Parser 基线（单一路线）
  - 决策：语法层强制采用 Tree-sitter，取消多路径与特性开关；`parse_file` 唯一路径经由 Tree-sitter。
  - 理由：与设计文档一致，反对多路径以避免实现漂移与分叉。
  - 影响：`core/syntax` 直接依赖 `tree-sitter`；待 Sophia 语法 grammar crate 可用后，补上 `set_language` 绑定，并在此记录。
  - 状态：Accepted

- 2026-05-28 — 注释与文档语言统一
  - 决策：工程内注释与文档统一使用中文；如需英文专用术语，采用“中文（英文术语）”的形式首次出现时注明。
  - 理由：降低沟通歧义，保持项目风格一致，便于团队协作与评审。
  - 影响：评审中如发现中英混用，要求统一；现存混用逐步清理。
  - 状态：Accepted

- 2026-05-28 — 推进方式与提交策略（大步推进、一次成型）
  - 决策：采用“大步推进、一次完成”的策略，禁止围绕同一文件的小步反复提交；严禁以“最小接口/占位”替代正面功能推进。
  - 理由：减少漂移与上下文切换，避免无效反复与后续返工。
  - 影响：在评审中对碎片化改动直接驳回；里程碑级修改要求单次合并完成并可运行验证。
  - 状态：Accepted

- 2026-05-28 — 代码结构边界（core/syntax 职责）
  - 决策：`core/syntax` 仅负责解析能力与稳定 API；禁止混入临时诊断、I/O、CLI 相关逻辑；必要时将辅助能力拆分模块，保持 `lib.rs` 内聚与简洁。
  - 理由：职责清晰，便于维护与演进，避免“大杂烩”。
  - 影响：将临时诊断与无关逻辑从 `core/syntax` 清理，CLI 层承担 I/O 与呈现。
  - 状态：Accepted

- 2026-05-28 — 版本与依赖策略（最新稳定且对齐）
  - 决策：在确保兼容前提下优先使用“最新稳定版本”，并保证“工具链/生成物/Crate”三者一致（如 Tree-sitter CLI、生成的 parser.c ABI 与 Rust crate 版本对齐）。严禁凭经验或模型记忆拍断版本。当前对齐：tree-sitter crate 0.26 + tree-sitter-cli 0.26.x + ABI 15。
  - 理由：避免因 ABI/接口不一致导致的构建/运行失败，减少未来迁移成本。
  - 影响：在引入/升级前明确兼容矩阵；提交中记录对齐关系与验证方式。
  - 状态：Accepted

- 2026-05-28 — 临时与诊断代码
  - 决策：禁止将临时诊断/调试打印合入主干；本地验证后必须清理。提交中不携带临时代码路径。
  - 理由：确保主干清洁，避免噪声与行为不确定。
  - 影响：评审中发现临时代码将被要求移除或退回。
  - 状态：Accepted

- 2026-05-28 — Vendor 与外部仓库策略
  - 决策：仅 vendor 必要头文件与生成产物；禁止将外部 git 仓库嵌入为子目录（embedded repo）。如需，使用 submodule 并在文档中说明；默认不启用 submodule。
  - 理由：避免仓库污染与拉取复杂度，保证可复现构建。
  - 影响：移除已嵌入的外部仓库索引；构建仅依赖本地 vendor 与生成目录。
  - 状态：Accepted

- 2026-05-28 — 文档同步纪律
  - 决策：每次大步功能合入后，立即同步当前进展 checklist（现为 `dev_checklist_v1.md`；v0 阶段为
    已归档的 `dev_checklist_v0.md`）与本工程笔记；禁止在对话或提交中重复陈述已完成项。
  - 理由：单一事实来源（SSOT），降低沟通开销。
  - 影响：评审检查提交是否包含相应文档同步。
  - 状态：Accepted

- 2026-05-28 — 输出确定性与遍历顺序
  - 决策：所有目录与文件遍历使用字典序；对外 JSON 使用稳定 key（如 `BTreeMap`）；路径统一正斜杠。
  - 理由：保证工具输出可比对、可快照、可复现。
  - 影响：接口与实现一律遵循该要求，偏离需在此备案。
  - 状态：Accepted

- 2026-05-29 — 工作流编排层 `workflow/engine` 与「注入报告」分层模式
  - 决策：新增 `workflow/engine` crate 承载工作流编排（`run_llm_step` / loop_steps / implement_loop /
    scheduler / select_materialize）；它是唯一同时依赖 `workflow` 与 `tools` 的 crate。`tools/*`
    确定性分析器不依赖 Development Graph，只产出结构化报告（`CheckReport`/`AuditReport`/`GateReport`/
    `VerifierOutcome`）；图副作用（emit `DiagnosticNode`、连 `checks→` 边）由编排层负责。需执行 check
    的编排件通过注入回调（`CodeChecker`）拿确定性结果，不自行 `use` checker。
  - 理由：把非确定与图副作用收敛到 `workflow`/`cli`，让 `core`/`tools` 保持纯确定性、易测试、无图感知；
    避免分层环（tools 反依赖 workflow 图）。
  - 影响：新增编排件遵循"注入报告/回调"而非直接耦合；`graph-db`/`llm`/`prompt` 互不依赖。
  - 状态：Accepted

- 2026-05-29 — LLM 结构化 schema 必须是忠实契约
  - 决策：每个工作流 LLM 步骤的 JSON Schema 字段约束必须与服务端反序列化目标类型严格对齐
    （`additionalProperties:false` + 完整 required），不留宽松缝隙。
  - 理由：`decision_node.json` 曾只 require `state_assessment.kind`，比 Rust 判别联合宽松，导致
    "schema 通过但反序列化失败"。schema 是 strict 契约（workflow_graph_spec 1.3），不可弱于类型。
  - 影响：新增/修改 schema 时与目标类型对照；判别联合用 `oneOf` 按 tag 各自 required 全字段。
  - 状态：Accepted

- 2026-05-29 — 共享测试工具集中化
  - 决策：集成测试的公用件（mock client、schema 取用、节点 seed、临时目录）集中到 `tests/common/mod.rs`，
    `#![allow(dead_code)]`（各测试二进制只用子集）；测试 schema 必须复用产物的权威来源
    （`sophia_prompt::schema_for`），禁止手写副本。
  - 理由：消除跨测试文件的复制；防止手写 schema 副本与产物漂移导致"测错对象"。
  - 影响：新增测试复用 common；不再就地手写 schema。
  - 状态：Accepted

- 2026-05-29 — stdlib 阻塞于语言设计（node/effect 顶层语法）
  - 决策：stdlib 内置节点（prompt/router/aggregator/tool/stream）与 effect 契约文件的实现，前置于
    新增 `node` / `effect` 两类顶层构造（设计已完成，见 `language_design.md` 第十三节）；在该语言
    设计落地前 stdlib crate 保持空壳，不用现有 9 类节点硬凑。
  - 理由：现 grammar 无 `node`/`effect` 顶层构造、effect 集合封闭，硬凑会偏离"内置节点"本意且制造
    多路径，违反单一路线原则。
  - 影响：stdlib 在 checklist 标注"阻塞于语言设计"；落地时一次性迁移硬编码 effect 为 stdlib 预声明
    （effect 三元组表示、`Family.Op(args)` 统一引用语法），移除 grammar 硬编码分支，不留双栈。
  - 状态：Superseded（2026-05-30 彻底移除 node/agent，见末尾「彻底移除 agent 编排 / node 构造」条）

- 2026-05-29 — 调度器 prompt 在调用时刻渲染（`StepPrompts` 提供者，取代静态 `StepRequests`）
  - 决策：`workflow/engine` 的 goal 推进调度器与 implement-loop **不再**接收预渲染的静态
    `CompletionRequest`（`StepRequests`），改为接收一个由协调层实现的 **prompt 提供者** trait
    `StepPrompts`：调度器在每步即将调用 LLM 时回调它，传入"该步骤源自当前图状态的输入"
    （decision/design 传 active context + focus；implement 传**本轮 design 产出的伪代码正文**；
    repair 传上一候选文件 + 诊断），由提供者**当场渲染**请求。建 `ContextSnapshot` 与回调提供者
    **必须用同一份 active-context 计算结果**（同源）。
  - 理由：`language_design.md` §10.7/§10.8——prompt 是 LLM 看到的全部世界，必须由调用时刻的 active
    context 渲染，且这正是 `consumed→ ContextSnapshot` 所快照/审计的那份。静态预渲染请求导致：
    ① 状态不演进、LLM 无法自主多步推进（启发式编排名存实亡）；② snapshot 与 LLM 实际所见不一致
    （破坏 10.7 复现保证与 anti-cheat 前提）；③ 实现步骤拿不到刚 design 出的伪代码。这是与"prompt =
    调用时刻 active context 投影"根本设定的冲突，非局部可补。
  - 影响：`StepRequests` 整体被 `StepPrompts` trait 取代（单一路线，不保留双栈）；`run_goal_loop` /
    `run_implement_loop` 签名改为收 `&impl StepPrompts`；schema 仍由调度器按固定步骤选取
    （`prompt::schema_for`）。engine 仍不含 prompt 模板/抽取逻辑（§3.3 分层不变），模板渲染与防泄漏
    纪律留在协调层提供者实现。设计见 `engineering_architecture.md` §8.4。
  - 状态：Accepted（已实现：engine `prompts` 模块 + `run_llm_step` 渲染闭包；CLI / e2e harness 适配；
    G3-01 实测 LLM 2 轮自主决策推进到候选）

- 2026-05-30 — 目标树遍历层独立于 spine（decompose / backtrack）
  - 决策：把 `decompose`（动作 6）/ `backtrack`（动作 7）这两个非线性树操作放进 spine
    （`run_goal_loop`）**之上**的独立遍历层 `engine::run_goal_tree`（`traversal` 模块），而非塞进
    spine。decompose 的图构造交确定性 helper `graph-db::build_decomposition`（LLM 只给拆解结构）；
    遍历层执行 `decompose_goal` 后递归把 spine 驱动到每个子目标（深度优先）；backtrack 仅记录放弃。
  - 理由：spine 是"单目标线性推进"的可独立测试 / 复用能力（CLI implement-loop 即只用 spine）；把树
    操作塞进 spine 会让它退化为塞满分支语义的大杂烩（违反单一职责）。分两层后各自职责单一、可分别
    测试，且 design 10.9"不在 spine 内臆造树语义"得以兑现。
  - 影响：新增 `graph-db::{build_decomposition, ChildGoal, DecompositionNodes}`、engine
    `decompose_goal` 步骤 + `traversal` 层（`run_goal_tree` / `GoalResolution` / `TreeBudget`）、
    prompt `decompose` 模板 + `decompose_result` schema；`StepPrompts` trait 新增 `decompose` 方法
    （所有 impl 适配）。诚实性硬约束：`Decomposition` 是 decompose 的 **LLM 执行产物节点**（承载 LLM
    生成的拆解结构），故**自身** `consumed→ ContextSnapshot`（I6）——锚定在产出本次拆解的 LLM 调用
    快照上，而非触发它的 DecisionNode（"该不该拆"是另一次调用，§10.8 动作选择 / 执行分离）；
    `build_decomposition` 因此接收并校验 snapshot 参数。子 Objective 是结构性派生节点（类比 assessment
    协议 FirstSlice / Constraint），经 member_of 间接锚定，不单独 `consumed→`。spec 的 I6 集合与
    `consumed` 边目录已纳入 `Decomposition`。backtrack **不伪造 `WithdrawalEvent`**、binding 不伪造
    （撤销 / 接受是人类权威 N4，LLM 派生子目标默认未绑定，人类接受 Decomposition 后才沿 member_of
    继承）。设计见 `language_design.md` §10.9、`engineering_architecture.md` §8.5、
    `workflow_graph_spec.md` §I6 / 4.1.4 / 6.1。
  - 状态：Accepted（已实现：6 项 graph-db decomposition + 5 项 engine traversal + 1 项 prompt
    render 测试；全工作区测试全绿）

- 2026-05-30 — Trace 投影与 verifier 执行器的分层归属
  - 决策：① 执行 Trace（impl §9.4）落在 `runtime`：`core/exec-ir` 为边引入稳定 `ExecEdgeId`，
    `runtime/trace` 在解释器执行时收集 `ExecutionSpan`（投影回图的 node_id / edge_id），`run_action`
    返回 `run_action(..., host) -> (Outcome, Trace)`。② constraint audit 的 verifier **执行器**落在 `runtime`
    （`verify` 模块 `run_hidden_case`），**不**放进 `tools/audit`；audit 仍是纯判定层（消费注入的
    `VerifierOutcome`），由协调层把 runtime 的 `VerificationResult` 零损耗映射注入。
  - 理由：解释器是 v0 唯一执行后端（architecture §3.2），凡「真正跑代码」都属 runtime；tools 层
    必须保持纯确定性判定、不依赖执行后端（§3.3 注入报告模式）。把执行器塞进 audit 会让 tools 反向
    依赖 runtime、破坏分层与可测试性。Trace 同理属解释器可观测性（§9.2），不在 core/tools。
  - 影响：`run_action` 签名变更（`Execution` 结构体取代二元组），全调用点同步迁移（单一路径，
    无双栈）。Trace 确定性优先：不记墙钟时长，只记图投影 + 进入序。verifier 执行器与 audit 判定经
    协调层接缝相连；图 gate 内自动驱动随后由「隐藏验证用例存储」条目落地（见下条）。
    设计见 `language_implementation.md` §9.4、`dev_checklist_v0.md` runtime / tools 两项。
  - 状态：Accepted（已实现：exec-ir ExecEdgeId + runtime trace/verify；4 trace + 6 verify + 1 CLI
    闭环测试；全工作区 292 passed / 0 failed）

- 2026-05-30 — 隐藏验证用例存储：复用 runtime 值模型，不设镜像类型
  - 决策：hidden case 隐藏存储（`sophia-runs/verifiers/hidden.json`）的值表示**直接复用**
    `runtime::Value` / `HiddenCase` / `ExpectedOutcome`（给它们加 serde 派生），而非另设一个
    `VerifierValue` 序列化镜像类型。
  - 理由：单一值模型——hidden case 的实参 / 期望最终要喂解释器（`runtime::Value`），若另设镜像
    就需双向转换、双处维护、易漂移（违反单一路线）。`runtime::Value` 本就是值的唯一权威表示，
    让它可序列化即可同时服务执行与存储。设计稿曾提 `VerifierValue` 镜像，落地时改为复用并更优。
  - 影响：`runtime` 增 serde 依赖；`Value` 序列化为外部标签判别联合（`{"Int":42}`）。CLI
    `verifier_store` 直接反序列化为 `Vec<HiddenCase>`。gate 侧从 ConstraintNode 原始 payload（非
    `ConstraintView`）读 `verifier.ref`——因为 anti-cheat 要求 `ConstraintView` 不投影 verifier，
    而 gate 又需要 ref 定位 hidden case，故走原始节点而非 active context 视图。分层：加载 hidden.json
    + 串联执行与判定属 CLI 协调层，tools/audit 与 runtime 都不感知存储与图。
  - 状态：Accepted（已实现：runtime serde + CLI verifier_store + run_constraint_audit 接线；
    3 项 CLI gate 集成测试；全工作区 296 passed / 0 failed）


- 2026-05-30 — 目标树遍历的人类授权检查点（拆解审查者 + 子目标 binding 继承）
  - 决策：`run_goal_tree` 新增**拆解审查者**回调 trait `DecompositionReviewer`（裁决
    `ReviewDecision::{Accept, Reject}`）。decompose 落图（建 `Decomposition` + 子 `Objective`）后、
    递归子目标**前**回调它：Accept → 遍历层创建**真实** human `AcceptanceEvent accepts→ Decomposition`，
    子目标随即沿 `member_of` 继承 binding、进入各自 active context，再递归；Reject → 不递归、不伪造
    `WithdrawalEvent`（记 `GoalResolution::DecompositionRejected`）。提供 `AutoAcceptReviewer`（调用方
    代表人类授权，仍走真实 AcceptanceEvent 落图）。
  - 理由：补 design 5.3 / N4 的真实能力缺口。此前 `run_goal_tree` 在 decompose 后直接递归 LLM 派生的
    子目标，而子目标是 LLM provenance、**默认未绑定**（`is_bound` 仅对 human provenance 隐式接受或链上有
    `AcceptanceEvent`）——其 active context 为空，真实 LLM 在子目标的 design/implement 拿不到自己的题面，
    无法实现。MockClient 单测因不看 prompt 内容而掩盖了该缺口。正解是把"子目标须经人类接受 Decomposition
    才获得 binding"（5.3）显式建模为授权检查点：引擎不伪造人类授权（接受 / 撤销是人类权威，N4），但提供
    注入授权的接缝（CLI 真人 / e2e harness 充当人类 / 无人值守策略）。
  - 影响：engine 导出 `DecompositionReviewer` / `ReviewDecision` / `AutoAcceptReviewer`；`run_goal_tree`
    / `drive_goal` / `drive_decompose` 增 reviewer 参数（单一路线，全调用点同步）；`GoalResolution` 增
    `DecompositionRejected`。e2e harness 的 prompt 提供者改为 **focus-aware**（按 focus id 从 active
    context 取目标题面，对根焦点逐字等价、无回归），新增 `CaseKind::Tree` + `tree_drive`（经
    `run_goal_tree` + `AutoAcceptReviewer` 合并子目标候选执行）+ G6 用例组。新增 2 项 traversal 单测
    （接受后子目标进入 active context / 拒绝不递归不伪造），既有 5 项适配 reviewer 参数后仍通过。
  - 状态：Accepted（已实现：engine reviewer 链路 + 2 单测；e2e G6-01 接线，真实 LLM 实跑待 API key；
    全工作区 307 passed / 0 failed）

- 2026-05-30 — 内置 node 解释执行：单 node 经 EffectHost 分派，多入/多出诚实留待装配语法
  - 决策：让声明的内置 `node` 可运行（v0 node 执行子集）——① exec-ir 把 node 也建为执行节点
    （`ExecNodeKind::Node`）；② runtime `EffectHost` 增 `invoke_node_effect`（family/op 分派），解释器
    `run_node` 把**单输入单输出 + 恰好一个非 Pure effect** 的 node 委派宿主执行；③ 多路输入 / 多路
    输出 / Pure 结构性 node（router / aggregator）以 `RuntimeError` **诚实硬错误**阻断，不实现其调度。
  - 理由：node 执行是 v0 最大语义空洞（agent-native 原语此前可声明不可运行）。但调查发现语言**没有
    "node 实例连成图"的表层语法**——grammar 里没有任何构造产生 node 间的 Data/Stream/Conditional/
    Fallback 边。因此多入/多出边调度器**没有表层来源**，现在实现它等于为"永不被产生的边"写投机性死
    代码，违反单一路线原则与不伪造原则。忠实姿态是：把"单 node 经 EffectHost 分派"这条有确定语义、
    有真实入口（`sophia run <NodeName>`）的子集做对、做完整；多入/多出调度显式留待"node 装配语法"
    这一前置语言设计落地后再做。
  - 影响：`core/exec-ir`（ExecNodeKind::Node + from_model + is_node）、`core/semantic`（NodeDecl 补
    multi_input/multi_output 标志）、`runtime`（EffectHost::invoke_node_effect + InMemoryHost 确定性桩
    后端 + Interpreter::run_node）。CLI 无需改动（run_action 经 runtime 路由 node）。InMemoryHost 的桩
    后端明确标注"非伪造真实 LLM/工具/流式服务"，真实后端是 v1 host import 职责。设计见
    `language_design.md` §13.5、`language_implementation.md` §20.1。
  - 状态：Superseded（2026-05-30 彻底移除 node/agent 编排，单 node 执行随之删除；见末尾
    「彻底移除 agent 编排 / node 构造」条）

- 2026-05-30 — 彻底移除 agent 编排 / `node` 构造，回归语言定位（撤销 2026-05-29 与 2026-05-30 两条 node 决策）
  - 决策：删除 `node` 顶层构造、`Llm`/`Tool`/`Stream` 内置 effect 族、五个内置节点契约、单 node 解释
    执行、以及整个 `sophia-stdlib` crate。**保留** `effect` 顶层构造与 `Family.Op(args)` 通用引用
    （内置族仅 `Console`/`DB`，由 `hir::builtins` 承载）。
  - 理由：经与作者复核确认——`node` + agent effect 是经由"stdlib 要内置 prompt/tool/stream 节点"这一
    **从未被独立论证的前提**从侧门引入的 **agent 编排**能力。它偏离 Sophia 的语言定位（§1：LLM-native
    确定性语义编程语言，LLM 是**程序员**而非程序内置能力；铁律之一是编译器不调用 LLM）。进一步审视
    发现：① 库构造不应有"作为执行入口"的需求，`node` 无 body 且不能被 action 调用，其唯一设想执行场景
    是"被装配进图由调度驱动"，而装配语法不存在 → 单 node 解释执行是为虚构需求造的假入口；② prompt/tool/
    stream 把外部不确定服务塞进语言标准库，与确定性核心立场冲突；③ `sophia-stdlib` crate 无任何消费者
    （编译器从 Rust 表读 effect，不从 stdlib 加载），只为镜像两个内置 effect 而存在，属过度设计残留。
    按单一路线 + 不要过度设计原则，整体彻底删除（非 disable）。`effect` 构造与 agent 无关、解决的是
    grammar 硬编码 effect 这一独立的债，保留。
  - 影响：grammar（删 node_def，重生成 parser.c）、AST（删 Item::Node/NodeDef）、HIR（删 NodeKind::Node、
    BUILTIN_EFFECT_OPS 仅留 Console/DB）、semantic（删 NodeDecl/model.nodes/check_node_contracts）、
    exec-ir（删 ExecNodeKind::Node）、runtime（删 run_node/invoke_node_effect）、删除 `sophia-stdlib`
    crate（workspace members/dependencies 摘除）、相关测试全删。文档：design §13 重写为 effect-only +
    §13.5 记录"不引入 node"决策、architecture §4 重写、impl §20、concepts.md、README 同步。全工作区
    298 passed / 0 failed。若未来确需 agent 编排，应作为**显式语言方向决策**重新设计（含 node 装配
    语法），而非标准库副产品。
  - 状态：Accepted（取代 2026-05-29「stdlib 阻塞于语言设计」与 2026-05-30「内置 node 解释执行」两条，
    后者标记为 Superseded）

- 2026-05-30 — 基准测试（benchmark）的外部解释器依赖与题目表示
  - 背景：`docs/benchmark_design.md` 建立 Sophia 工作流 vs「LLM 直接生成 Python」的横向对比基准。
    `sophia` mode 的判定**复用既有 `runtime::verify`**（零新增执行能力）；但 `baseline` 必须**真正
    执行 LLM 生成的 Python 代码**才能判定成功——当前纯 Rust 工作区不具备该能力。
  - 题目表示决策：题目用 **Rust `Problem` 结构 + 复用 `runtime::Value` / `runtime::verify::HiddenCase`**
    表示（与 e2e 把用例表示为 Rust `Case` 同构），**不引入外部配置文件格式**——初稿曾移植一个
    TypeScript 外部原型的 `task.json` / JSON 值 schema 等 TS 特有形态，因无实现可比性已重写纠偏。
  - 决策：`baseline` 经 **spawn `python3` 子进程**执行候选 + `value_to_json` 规约比对；按「依赖按需
    引入」原则，`python3` 作为**运行期外部工具**依赖、**不进 Cargo 依赖树**，缺失时 baseline mode 干净
    跳过。语言是 `baseline` 的参数；**只做 Python**（依赖最轻最普及），TypeScript 不做。
  - 安全：执行 LLM 生成的任意代码经**受限临时工作目录 + 5s 硬超时 + `DirGuard` 用后清理**（防失控 /
    恶意代码），与 `safety_guardrails` 一致。
  - 诚实性：baseline 子进程编译 / 执行硬错误 / 超时如实归因，绝不伪造通过；`sophia`（判定复用
    `runtime::verify`）与 baseline（从零搭子进程执行 + 跨语言比对）在「判定执行能力」上的不对称是
    **项目固有**的真实工程成本，文档正视不掩盖。防泄漏经 `Problem::public_brief()` **类型隔离**
    hidden cases（prompt 组装函数收不到答案），比运行时断言更硬。
  - 影响：实现于 `cli/examples/benchmark/`（6 文件，与 e2e 对称、不进 `cargo test`，无 key / 无 python3
    干净跳过）；产物落 `sophia-runs/benchmark/<label>/{runs.jsonl, summary.md}`（核心两指标）；题集
    L1–L4 共 6 题、与 e2e 用例刻意不重叠；`sophia` mode 自带精简闭环、**不复用 e2e harness**（先不
    抽象）但纪律一致。Python runner 协议经手工 smoke 验证。
  - 状态：Accepted（已实现：benchmark example 编译通过、clippy(-D warnings) 0 警告、`--list` 列出
    6 题；全工作区 298 passed / 0 failed；真实 LLM 端到端实跑待 API key）

- 2026-05-30 — 起步子集补一元算术取负 `-x`（语言层修复 benchmark abs_difference 失败的根因）
  - 背景：benchmark `abs_difference` 的 `sophia` mode 反复用 `-diff`（一元取负）实现绝对值、3 轮 check
    未收敛。根因分析：grammar 的 `unary_expr` **只有 `not`**（布尔），**无算术一元取负**。等价的
    `0 - diff` 在子集内可用（已验证 check + run 通过）但模型不易发现。§16.5 列"整数算术"在起步子集内、
    却无取负，且无任何排除取负的设计理由——判定为**意外缺漏**而非刻意边界。
  - 决策：把一元取负 `-x`（语义 Int→Int）补进起步子集**全链路**：grammar `unary_expr` 增 `-` 分支 →
    用对齐的 tree-sitter CLI 0.26.9 重生成 `parser.c`（ABI 15）→ AST 增 `Expr::Neg` → lower 按 op 字段
    分派 not / neg → 语义类型层（操作数须 Int、结果 Int）→ 解释器（`Value::Int(-i)`）。共享语法基线
    显式列出取负并标注**无除法 `/` / 取模 `%`**（仍刻意排除，避免除零 / 截断语义，留待扩展点）。
  - 理由：可泛化、非补丁——它为**所有**程序消除"无法表达取负"的缺口，而非只救这一题；符合"失败例子
    须找可泛化无作弊修复，语言层问题须补设计"。
  - 影响：6 个文件链路改动 + 共享语法基线 + 新增 `runtime` `unary_negation` 回归测试；全工作区
    299 passed / 0 failed。文档：`language_implementation.md` §16.5 更新。修复后 abs_difference sophia
    转 PASS（用 `-diff`，一次修复收敛）。
  - 状态：Accepted

- 2026-05-30 — 共享语法基线补"命名保真"规则（脚手架层修复 benchmark traffic_next 失败的根因）
  - 背景：benchmark `traffic_next` 题面命名 state 为 `TrafficLight`，但 `sophia` mode 的 design→伪代码
    步骤（刻意抽象掉 formal 名）把它泛化成 `Light`，implement 忠实实现 `Light`，确定性 check 通过（内部
    自洽），却在解释器**运行时校验**因 state 名不符判负（hidden case 调 `NextLight(TrafficLight.Green)`）。
  - 决策：在**单一共享语法基线**（`sophia_syntax_baseline`）加"命名保真"规则——题面 / 验收条件**显式
    给出**的名字（node / 字段 / 状态值 / error variant）必须**逐字照用**，不得改名 / 翻译 / 缩写 / 改
    大小写；并澄清与"勿照抄中立示例名"的区别。基线里用**与任何任务无关的中立名** `WidgetKind` 举例
    （不用任何真实任务名），防泄漏断言登记 benchmark 题集 token（AbsDifference / TrafficLight / …）。
  - 理由：防泄漏安全——该规则只要求忠于**已在公开题面里**的名字，**不透露任何 hidden case**；可泛化、
    非补丁——同时惠及 e2e 与 benchmark 的所有用例。属脚手架 / 工作流纪律，不是语言变更。
  - 影响：`sophia_syntax_baseline.md` + 其 snapshot 更新；防泄漏断言新增 benchmark token；修复后
    traffic_next sophia 保留 `TrafficLight` 命名、hidden case 全过。e2e 用例同步受益。
  - 状态：Accepted

- 2026-05-30 — 进入 v1：阶段定位校准（两个项目目标，WASM 为既定头等事项）
  - 背景：v0（解释执行）核心链路 + 工作流闭环 + e2e 六组 + benchmark 难度阶梯均已跑通真实 LLM。
    阅读旧项目技术报告（v0.3）后，明确项目有两个目标且**主次分明**：① 主——做一门真正可用的、
    面向 LLM 无人介入的编程语言与工具链（严肃工程，非为发论文的玩具）；② 次——发论文证明其价值。
  - 决策：**进入 v1**，v1 = "把玩具变严肃语言"，含两条并行工作流：**A WASM codegen**（既定、必经，
    把执行后端从仅解释器扩展为可部署 artifact；解释器不退役，转为 codegen 的等价 oracle / 差测试
    基线）+ **B 语言 / 标准库扩充**（`Result<T,E>` / error handle / `task` 执行 / `entity.with` /
    跨 domain intent 数据流 / 合约证明 —— 按机器可证价值准入，支撑更复杂的严肃程序与基准阶梯 L6+）。
    二者缺一不可：v1 完成判据 = WASM 输出与解释器逐一等价 + 语言能表达明显超出 v0 的严肃程序 +
    strip-assist 在 artifact 层成立。
  - 校准要点（曾一度误判，已纠正）：WASM **不是**可推迟的可选项、也不应被"论文价值在语义层、WASM 可
    缓"的论证降级——它是目标 1（让语言真正可用）的必经步骤。基准的"成功率/耗时"只是可运行性证据之一，
    不是项目中心价值；但这不改变 WASM 在 v1 的头等地位。
  - 影响：文档校准——`language_design.md` §1.1 新增"两个目标"；`engineering_architecture.md` §14.2
    重写为 WASM + 语言扩充两条工作流、§14.3 纳入演化能力（edit transition / 跨 domain / 更强
    strip-assist）；`language_implementation.md` §19.1 新增 v1 构建顺序；`benchmark_design.md` §3.1
    标注阶梯随 v1 向 L6+ 延伸；`dev_checklist.md` 概述改为"v0 收尾 / v1 启动"。本次为**文档校准**，
    不含 v1 代码。
  - 状态：Accepted

- 2026-05-30 — 进展 checklist 按版本拆分（v0 归档冻结 + v1 活跃），工程笔记保持统一
  - 决策：把 `dev_checklist.md` 重命名为 `dev_checklist_v0.md` 并**冻结为只读归档**（v0 解释执行阶段
    已完成）；新建 `dev_checklist_v1.md` 作为**当前活跃 SSOT**，按 `language_implementation.md` §19.1 的
    v1 构建顺序（工作流 A WASM codegen + 工作流 B 语言/标准库扩充）组织，并结转 v0 未竟开放项。
    **`engineering_notes.md` 不拆分**——决策日志天然跨版本，继续统一。
  - 理由：v0 checklist 已基本是“完成记录”，继续往里追加 v1 条目会让“当前进展”与“历史归档”混在一处、
    难以一眼看清 v1 推进到哪。按版本切分让每个 checklist 聚焦一个阶段；决策日志则相反——它记录的是
    “为什么这么定”的跨阶段连续脉络，拆开反而割裂上下文。
  - 影响：`git mv` 保留历史；更新全仓引用——活跃指针（README 文档清单 / CONTRIBUTING / PR 模板 /
    CHANGELOG / concepts 速查表）指向 `dev_checklist_v1.md`，历史/已实现内容的引用（语言设计 score 项、
    实现 §19 v0 step、笔记内 trace/verify 条目、graph_cmd 注释）指向 `dev_checklist_v0.md`。文档同步
    纪律条目改为“同步当前进展 checklist”。本次纯文档 / 引用调整，无功能代码改动。
  - 状态：Accepted

- 2026-05-30 — v1 工作流 B 改为需求驱动 + 逐项设计评审（审核纠偏，立 `v1_demands.md`）
  - 背景：v1 checklist 初稿把 §16.6 的"起步子集外设计项"（`Result` / `entity.with` / 跨 domain / 合约
    证明 / `task` 执行）当成确定的 B7–B12 实现序列。审核发现这些是**扩展点标签、非已设计特征**（设计文档
    每项仅一行、无语法/语义/类型规则），把它们排成 build-order 属过度设计 / 臆造；且其中跨 domain（报告
    S3）、合约证明（独立子系统）是 v2+ 量级，放进 v1 会撑破边界。
  - 决策：v1 工作流 B 改为**需求驱动 + 逐项设计评审**。两条准入通道：① 演示需求驱动（缺什么补什么、需求
    封顶）；② 强论证的 LLM-native 特征（门槛更高）。立 `docs/v1_demands.md`：先定三个 v1 演示需求
    （D1 可失败结果建模 / D2 网络获取 + intent 安全〔旗舰，落地报告 §7/§8 accept/reject〕/ D3 严肃管线
    综合题），再反推**最小扩展集 F1（`Result<T,E>`）+ F2（`Http` 内置 effect 族，与 Console/DB 同构、
    复用 intent + capability、零新语法）+ S1（HTTP host 标准库，仅 workflow/runtime 层）**。每项先完成设计评审
    （单独设计 → 确认 → 实现），落地范式同 `effect` / storage。
  - 显式推迟 v2+（无 v1 演示需求触发）：`entity.with`、跨 domain / library intent 数据流、`requires`/
    `ensures` 合约证明子系统、`task` 执行入口。出现对应演示需求时再各自走设计评审流程。
  - 边界收紧：v1 完成判据 2 由"能表达明显超出 v0 的程序"（无界）改为"D1/D2/D3 三演示题端到端跑通 +
    D2 给出一条真实 accept/reject 矩阵条目"（有界、可检查）。
  - 影响：新建 `v1_demands.md`；改写 `dev_checklist_v1.md` 工作流 B + 完成判据、`language_implementation.md`
    §19.1 工作流 B、`engineering_architecture.md` §14.2 工作流 B；README / concepts 文档索引补 `v1_demands.md`。
    纯文档，无功能代码。F2/Http 的选择**强论证**：直接复用已实现的 intent + capability/effect + host 三套
    机制，新增面仅"一个 effect 族 + 一个 host 方法"，却能构造报告旗舰的 accept/reject 矩阵——是"扩展面小 /
    LLM-native 价值大"的范例，非架空特征。
  - 状态：Accepted

- 2026-05-30 — 语法设计准则：不仿造人类高级语言机制，优先语义直观 / 无省略 / 不惧繁琐
  - 决策：Sophia **不设计**用户可扩展的泛型系统、模板、宏、trait/typeclass、操作符重载、隐式转换、
    省略式语法糖（如 `?` 错误传播 / `unwrap`）。语法**优先语义直观、显式无省略**；繁琐不是缺点。
    已有的封闭内置 wrapper 集（`List` / `Optional` / `Result` / Intent）是固定一等构造、不可由用户
    参数化新建，**不构成泛型系统**，不在禁止之列。
  - 理由：直击 LLM 能力画像——**语义强、记忆弱**。LLM 擅长把意图就地表达成结构，不擅长记住并正确套用
    一套抽象展开规则（类型推导、宏卫生、隐式 trait 解析、糖的脱糖规则）。繁琐对 LLM 不是成本（不嫌写
    得多），隐式 / 省略才是真实成本（要靠记忆补全被省略部分、易错）。与"把需要记忆的变成需要表达的"
    核心哲学一致。
  - 影响：`language_design.md` §3 新增准则 + §3.2 取舍行 + §12 Non-goal 收紧；`v1_demands.md` F1 对齐
    （`Result` 走显式 `match`、不设 `?`/`unwrap`，`E` 仅 error variant）。后续任何语言扩展评审以此为
    硬尺：若主要收益是"更短 / 更像现有语言 / 复用抽象"，默认拒绝；若是"就地显式表达、局部可读、可机器
    检查"，才考虑。这是跨 v0/v1/v2 的长期语言纪律。
  - 状态：Accepted

- 2026-05-30 — 删除 `v1_demands.md`（折入 checklist）+ 标准库两点澄清（功能库 + 提示词脚手架）
  - 决策：① `v1_demands.md` 是**临时需求分析文档**，其实质（需求驱动方法论 + D1/D2/D3 演示需求 + 边界）
    **内联进 `dev_checklist_v1.md` §二**（使 checklist 自包含），文档删除；上游 `engineering_architecture.md`
    §14.2 / `language_implementation.md` §19.1 / README / concepts 的引用改指向 checklist。② **标准库范围
    = 功能库而非协议栈**：按需求只做用得到的**功能层**（如 `Http.Get`），**不**自建 TCP/IP / TLS / socket
    等底层协议（交给宿主 host 运行时的成熟实现）。③ 新增 **S2 标准库提示词脚手架**：LLM 对标准库**无
    先验知识**，必须为每个库提供**标准化、按需取用**的库介绍 prompt 资产（用途 / 用法 / intent 边界 /
    capability），复用 §8.3 `preamble` + `prompt/assets/` 机制，用到哪个库才注入哪份；属提示词工程，v1
    必须考虑。最小扩展集相应记为 **F1 + F2 + S1 + S2**。
  - 理由：①“临时分析”不应成为常驻文档，进展与边界归 checklist 单一来源，避免双处维护漂移；② 标准库若
    陷入造底层协议栈会无限扩张、偏离需求驱动；③ 没有库介绍 prompt，LLM 写不出用标准库的程序——S2 是 D2
    演示能跑通的前置条件，且“库知识当可裁剪上下文资产”契合 LLM-native 上下文裁剪立场。
  - 影响：删除 `docs/v1_demands.md`；`dev_checklist_v1.md` §二内联演示需求 + 新增 S2 任务分解
    （S2.0–S2.3）+ S1 功能库范围注；`engineering_architecture.md` §14.2、`language_implementation.md`
    §19.1 同步补 S2 与功能库范围、改引用；README / concepts 去 `v1_demands` 行。纯文档。
  - 状态：Accepted

- 2026-05-30 — F1 纠偏：废弃 Rust 式 `Result<T,E>`，可失败返回改 `one of {...}`，并统一全部类型语法
  - 决策：**否决**先前 F1 的 `Result<T,E>`（`Ok/Err`）方案，可失败 / 可空返回一律用 **`one of { 成员, ... }`**
    联合类型表达——成员**直接构造、直接 match**，无 `Ok`/`Err`/`Some`/`None` 包装子。并**顺势统一整套
    类型语法**：`<>` 形式**专属 Intent Type**；结构类型一律用 `of` 关键字族（`list of T` / `one of { ... }` /
    `schema of T`）；**废弃** `Optional<T>` / `List<T>` / `Schema<T>` 的 `<>` 形式、`Some`/`None`、
    `<optional>.exists` 伪字段；**新增** `Null` 内置单值类型（字面 `Null`）；match 引入**类型 pattern**
    （`Int x =>` / `Todo t =>` / `Null =>` / `V { f } =>`，禁止 `_`，穷尽性不变）。storage `get` 返回
    `one of { ValueTy, Null }`，`save` 仍返回 `ValueTy`（storage 不引入失败，待持久化后端）。判断"有值"在
    谓词上下文用 `!= Null`、在 body 用 `match` 的 `Null` 分支。设计定稿见 `docs/type_system.md`
    （取代已删的 `result_type.md`）。
  - 理由：① 用户指出方案 C 下 `Result<>` wrapper **无信息量**（成员已是具名 variant + tagged union，不需
    再套泛型容器），且 `Result`/`Some`/`None` 是**被 Rust 心智带偏**——语言用 Rust 实现不等于语言设计该偏
    Rust。② `<>` 同时承载 intent（`Raw<T>`）与容器（`List<T>`/`Optional<T>`）是 v0 的**隐性例外**，违背
    "强语义、少符号约定、**无例外**"——统一为"`<>` ⟺ intent、`of` ⟺ 结构"后，一条规则零特例，LLM 不必记
    "哪个 `<>` 是 intent"。③ "成员就是自己"运行时更省（无 discriminant wrapper），契合 LLM-native。
    单一路径、彻底重构、无兼容层、不留语法糖。
  - 影响：**全链路重构**——grammar.js + parser.c（`intent_type`/`list_of`/`one_of`/`schema_of`、type/variant
    pattern、`Null` 字面）、`syntax`（AST `TypeRef`/`Pattern`/`Expr`、lower）、`hir`（builtins `INTENT_WRAPPERS`
    + `Null`/`Unknown` 标量、resolve `one of` 成员解析 + 类型 pattern 绑定）、`semantic`（`Ty::OneOf`/`Null`/
    `ErrorVariant`、distinguishability + 穷尽性扩展、storage get 类型、assignability upcast）、`exec-ir`、
    `runtime`（`Value::Null`/`ErrorValue`、`one of` 值即成员自身、match 按 tag 分派）、`benchmark` value_json。
    文档：`type_system.md` 定稿、`language_design.md` §3/§6/§7 + TodoDomain 示例、`language_implementation.md`
    §7.1/§8.2/§16.1/§16.5/§16.6 + §14 路线图、`benchmark_design.md` / `e2e_test_design.md` / `architecture`
    §14.2 + `dev_checklist_v1.md` F1 任务块；共享语法基线 `sophia_syntax_baseline.md` 全面改写 + snapshot。
    全工作区 299 passed / 0 failed，clippy（`-D warnings`，含 example）0 警告，fmt clean；旗舰探针
    （`one of { Int, InsufficientFunds }` 直接返回 + match variant）端到端验证通过。
  - 状态：Accepted（**取代**上一条 F1 的 `Result<T,E>` 设定）

- 2026-05-30 — F1 重构完整审核：补 `one of` 可区分性检查 + match 类型 pattern 名称解析
  - 决策：审核 F1 全链路重构（代码 + 文档）后，落实两处**设计已规定但实现缺失**的检查：① **`one of` 成员
    可区分性**（设计 `type_system.md` §2.2/§七/§九.5）——新增 `core/semantic/src/union_check.rs`：以"match tag"
    为判据（标量按类型名 / `Null` 唯一 / entity·state·variant 按名；**intent 运行时擦除故取 inner**；嵌套
    `one of` 展开），对全程序所有类型位置（entity 字段 / storage / callable 签名 / error variant 字段 /
    effect param）遍历检查，重复 tag 报新诊断 `IndistinguishableUnion`（CHECK-TYPE-006）。② **match 类型
    pattern 的类型名解析**——HIR `resolve.rs` 新增 `resolve_pattern_type_name`，`match x { Bogus v => }` 的
    未知类型名现报 `UnresolvedReference`（此前静默，下游退化为误导性 NonExhaustiveMatch）。
  - 理由：可区分性是 `one of` match 分派确定性的**前提**——`one of { Int, Int }` / `one of { Raw<Text>, Text }`
    若放行，运行时只能首个匹配胜出，破坏"语义直观、确定性"的 LLM-native 立场；设计早已写明，属实现欠账而非
    新增范围。类型 pattern 名不解析则违反"所有引用必须可解析"的名称解析铁律。
  - 影响：`core/semantic`（新增 `union_check` 模块 + `IndistinguishableUnion` 诊断码）、`core/hir/resolve.rs`
    （类型 pattern 名解析）；新增 6 测试（analyze 4 + resolve 1 + lowering 1）。另清理全仓散落旧语法注释
    （`Optional`/`Schema<T>`/`match Some`/`Some(/* c */ 5)`）。全工作区 305 passed / 0 failed，clippy 0 警告，
  - 状态：Accepted

- 2026-05-30 — F2 落地：`Http` 内置 effect 族（D2 旗舰），零新语法、与 storage 同构
  - 决策：引入 `Http` 为**内置** effect 族（进 `hir::builtins::BUILTIN_EFFECT_OPS`），`Http.Get(url) -> Raw<Text>`
    经 body 级 `Http.Get(url)` 调用（与 `storage.X.get(k)` **完全同构**的"特殊根 method_call + host 委派"
    路径——**零新语法**：grammar/AST/lower 不动）。三个设计决策（`docs/http_effect.md` §八确认）：
    ① **effect 身份 `Http.Get` 不带 URL 实参**——capability 粒度到"能否发 GET"（`allow { Http.Get }`），
    理由是 URL 多为运行时绑定值、作 arg 只会被 `covered_by` 当通配，无管控收益（对比 storage 表名静态已知
    才作 arg）；② **返回裸 `Raw<Text>`** 而非 `one of { Raw<Text>, HttpError }`——D2 焦点是 intent 安全
    非网络失败建模，失败走 host 硬错误；③ **`Http` 与 `storage` 同列 body 特殊根**。intent 边界拦截
    （D2 reject）**完全复用**既有 intent 严格相等检查——零新检查代码。mock host（`InMemoryHost::seed_http`）
    未命中即 `Err` 阻断、**绝不伪造成功**；真实 `reqwest` host 是 S1，不进确定性测试。
  - 理由：F2 的准入是"扩展面小 / LLM-native 价值大"——新增面仅"一个 effect 三元组 + 一个 host 方法 +
    一个类型层特判（`infer_effect_op`）"，却能构造技术报告旗舰的 accept/reject 矩阵（Sophia 静态拒绝
    "fetch 字符串直接当可信值"，主流语言放行）。保持与 storage / `effect` 同一落地范式，语言与标准库
    风格一致。
  - 影响：`core/hir`（builtins + resolve 特殊根）、`core/semantic`（`type_layer::infer_effect_op`）、
    `runtime`（`EffectHost::http_get` + `InMemoryHost` mock + `interp::try_effect_op`）；9 测试（semantic 6 /
    runtime 2 / hir 1）。**F2.4 不改常驻 `sophia_syntax_baseline`**——Http 库知识归 S2 按需资产，避免污染
    无关任务 context（与 S2「按需取用 / 上下文裁剪」一致；这是对 http_effect.md §六 初稿"基线补形状"的
    纠正，已同步设计文档）。文档：`http_effect.md` 定稿、`language_design` §6.3/§13、`language_implementation`
    §7.3/§16.6/§20 同步。下一步 S1（真实 reqwest host）/ S2（Http 库提示词资产）。
  - 状态：Accepted

- 2026-05-30 — S2 落地：标准库提示词脚手架（按需取用），先于 S1
  - 决策：实现标准库 prompt 资产的**按需取用**机制——布局 `workflow/prompt/assets/stdlib/<lib>.md`（首份
    `http.md`）、prompt crate 加 `stdlib_asset` / `stdlib_libs` / `stdlib_preamble(&[libs])` API（字典序去重、
    未知忽略、空集空串）。选取信号是**任务显式声明的库集合**（e2e `Case.libs` / benchmark `Problem.libs`），
    **非文本嗅探**（确定、可测、符合「全部显式表达」）。三处 implement-system 组装（e2e harness /
    benchmark sophia_mode / CLI graph_cmd）拼入 `stdlib_preamble(libs)`，默认空集 = 零注入零回归。
    **推进顺序选 S2 先于 S1**：S2 是 D2 演示前置（LLM 对 `Http` 无先验则写不出网络程序），而 F2 mock host
    已能确定性跑通全链路，S1 真实网络只 live demo 需要、非 D2 前置。
  - 理由：确立**基线 vs 库资产的边界**——常驻 `sophia_syntax_baseline` 只承载核心语言语法（每个
    implement/repair 注入），库知识（`Http` 等）归按需注入的独立资产，无关任务不被污染 context。这正是
    F2 落地时确认的纠偏（`http_effect.md` §六注），契合「语义可恢复 / 上下文裁剪」的 LLM-native 立场，
    并为未来标准库提供可复用的脚手架范式。库资产同基线的防泄漏 + snapshot 纪律（不含任务 token）。
  - 影响：`workflow/prompt`（`STDLIB_ASSETS` + 3 API + `assets/stdlib/http.md`）、`render.rs`（http snapshot +
    库资产防泄漏断言〔提取共享 `FORBIDDEN_TASK_TOKENS`〕+ 选取单测）；`cli/examples/e2e`（`Case.libs` + harness
    注入接缝 + 12 用例补字段）、`cli/examples/benchmark`（`Problem.libs` + sophia_mode 接缝 + 7 题补字段）、
    `cli/src/graph_cmd.rs`（`system(libs)` 接缝，暂传 `&[]`）。全工作区 319 passed / 0 failed。文档：
    `stdlib_prompt_scaffolding.md` 定稿、`dev_checklist_v1` S2 标记完成。下一步 S1（真实 reqwest host）或
    D2 演示题（benchmark L6，需真实 LLM 实跑）。
  - 状态：Accepted

- 2026-05-30 — S1 落地：HTTP 客户端真实 host（协调层注入，runtime 零 IO）
  - 决策：`Http.Get` 的真实网络 host 放**协调层 CLI**，不进 `runtime`（解释器保持同步纯逻辑 + 零 IO，
    经 `EffectHost` 委派副作用）。`cli/src/http_host.rs` `CliHost` **组合委派**：console/storage 复用
    `InMemoryHost` 的内存实现，只把 `http_get` 覆盖为 sync `reqwest::blocking`（固定超时；非 2xx /
    网络失败 / 读取失败一律诚实 `Err` → 解释器物化为 `RuntimeError` 硬错误中止）。runtime 新增
    `run_action`（注入入口；`run_action` 仍是默认便捷入口、薄封装）。CLI `run` **据入口 action
    声明 effect 含 `Http.Get` 才注入 `CliHost`**，否则用默认内存 host——无网络程序零开销、零行为变化。
  - 理由：host import 的语义是"宿主提供副作用实现"（架构 §4），真实网络是宿主职责；与 LLM 真实后端
    `HttpLlmClient` 在 `workflow/llm`、core 零 IO 同纪律。sync `reqwest::blocking` 匹配 `http_get` 同步
    签名、不必把 tokio 穿进解释器（`run` 命令本就是同步路径）。网络失败走硬错误而非 `one of` 可恢复返回
    ——与 F2 一致（D2 焦点是 intent 安全；若日后演示需要可恢复网络失败，再按 F1 `one of` 扩展返回类型，
    走设计评审流程）。诚实性红线同 F2 mock。
  - 影响：workspace `reqwest` 加 `blocking` feature；`runtime`（`run_action`）、`cli`
    （`http_host.rs` + `commands::run_action` 按 effect 选 host + reqwest 依赖）。测试：runtime 注入接缝 1 +
    CLI 接缝单测 2（委派等价 / 非法 URL 诚实 Err，均不触网络）；真实网络不进 `cargo test`。全工作区
    322 passed / 0 failed。文档：`http_host.md` 定稿、`dev_checklist_v1` S1 标记完成、`language_implementation`
    §16.6 注真实 host 落点。至此 F1+F2+S1+S2 全落地。
  - 状态：Accepted

- 2026-05-30 — S2 纠偏：库选择从「任务预声明」改为「design 阶段 LLM 看目录自选」
  - 决策：**否决** S2 初版把库选择放进任务元数据（e2e `Case.libs` / benchmark `Problem.libs`）的做法。
    改为**两阶段**：① **design / revise** 注入**库目录**（`prompt::stdlib_catalog`，每库一行「名 — 用途」，
    无操作签名），LLM 在 `design_result.libraries` 字段**自选**要用的库；② **implement / repair** 据 design
    所选库注入**完整库资产**（`stdlib_preamble(selected)`）。`libraries` 经 `PseudocodeArtifact` → scheduler
    `current_pseudocode` → `run_implement_loop` → `StepPrompts::implement/repair` 全链路贯通。移除
    `Case.libs` / `Problem.libs` 及 `BenchPrompts.libs` / `HarnessPrompts.libs`。
  - 理由：用户指出任务预声明有两个根本问题——① **作弊嫌疑**：把"用哪些库"塞进题目，提前泄漏了解法方向
    （库选择本应是 LLM 在 design 阶段想清楚才浮现的决策）；② **必然推倒重做**：三方库一多，预声明会让题目
    元数据 / 伪代码阶段负担失控。正确模型是让 LLM 像真实程序员——design 时**看到有哪些库**（目录是任务无关
    的语言事实，如同程序员知道标准库存在，不泄漏解法）并自己选，implement 时才拿到所选库的完整用法。这也
    与「动作选择与执行分离」「按需 paging / 上下文裁剪」一致。
  - 影响：`prompt`（`STDLIB_ASSETS` 改三元组〔名/用途/资产〕+ 新增 `stdlib_catalog`；`design_result` schema
    增 `libraries`；design_solution / revise_design 模板注入目录 + 要求选库）、`engine`（`DesignResult` /
    `PseudocodeArtifact` 加 `libraries`；`StepPrompts::implement/repair` + `run_implement_loop` 加 `libraries`
    参数；scheduler `current_pseudocode` 携带库 + 三个 dispatch 贯通）、`cli`（e2e harness / benchmark
    sophia_mode / graph_cmd 三处 StepPrompts impl 同步；design 渲染注入 catalog；移除任务 `libs` 字段；
    graph design→implement 跨命令经伴生 `.libs` sidecar 持久化所选库〔与正文 sidecar 同模式，spec §4.4.3〕）。
    测试：design 选库传播 2 + catalog snapshot/防泄漏 1 + graph `.libs` sidecar 往返 1 +
    既有选取/资产单测。全工作区 326 passed / 0 failed。文档：`stdlib_prompt_scaffolding.md` §一/§三/§四/§六
    改写为两阶段、`benchmark_design` / `e2e_test_design` / `dev_checklist_v1` 同步。三条路径
    （e2e / benchmark / graph CLI）两阶段机制全部贯通，无遗留缺陷。
  - 状态：Accepted（**取代**上一条 S2 的任务预声明做法）

- 2026-05-31 — **技术债全面清查与修复**（用户专项指示：目录架构 / 文件命名 / 文件膨胀 / 代码重复冗余 /
    废弃代码 / 风格不一致 / 代码-文档漂移七维度）。经 context-gatherer 系统勘查后分三批落地：
  - 决策：① **去重（H1+H2）**——`code_check` 桥接（语法→HIR+语义三层+strip-assist）此前在 CLI `graph_cmd`、
    e2e harness、benchmark sophia_mode **三处逐字重复**（约 240 行），收敛到唯一实现 `sophia_engine::code_check`
    + `domain_of_path`（workflow 层，与 `run_implement_loop` 同层；可观测性打印留给调用方薄包装）；system prompt
    文案（语法基线 + 按需库资产 + 输出形状）同样三处重复，收敛到 `sophia_prompt::design_system_prompt()` /
    `implement_system_prompt(libraries)` 单一来源。② **文档漂移**——`CHANGELOG [Unreleased]` 补 F1/F2/S1/S2；
    `README` 补 v1 设计文档引用；`engineering_architecture` §8.3 prompt crate API + §8.4.2 骨架 `libraries` 参数
    对齐实际签名。③ **文件膨胀（H4）**——最大文件 `cli/src/graph_cmd.rs`（1351 行）按职责拆为模块目录：
    `graph_cmd/mod.rs`（757 行，确定性命令 + design/implement LLM 命令）+ `graph_cmd/gate.rs`（624 行，
    select/materialize 的 gate 重跑：code_check + constraint_audit + hidden-case 执行 + artifact_diff/runtime）；
    共享 helper（`open_store`/`parse_node`/`artifacts_dir`/`write_code_artifacts`）以 `pub(super)` 提供，
    `select`/`materialize` 经 `pub use` 重导出保持 `main.rs` 调用点不变。
  - 理由：长期增量开发积累的重复（同一桥接逻辑随 e2e/benchmark/CLI 三条路径各抄一份）会让缺陷修复必须三处
    同步、极易漂移；单文件膨胀超千行损害可读性与可维护性。收敛到单一来源 + 按职责拆分符合「单一路线、不重复」。
  - 影响：新增 `workflow/engine/src/code_check.rs`（engine 加依赖 `sophia-syntax`/`sophia-check`）、
    `workflow/prompt/src/lib.rs` 两个 system prompt 函数；`cli/src/graph_cmd/{mod,gate}.rs`（拆分）；
    `cli/examples/e2e/harness.rs`、`cli/examples/benchmark/sophia_mode.rs` 的 `real_check` 改委派薄包装。
    全工作区 326 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean。
  - 评估保留项（非债务，记录判定）：① **H3**（design→implement 闭环驱动 harness vs sophia_mode 形似）受
    `sophia_mode.rs` 既有决策「故意不复用 e2e harness（设计 §7.3，先不抽象）」约束，且二者上下文不同（基准评分
    vs 端到端断言），保留；② **M1** LSP `analysis.rs` 诊断收集与 `code_check` 同源不同层——LSP 需精确 `Span`
    （byte range）供 hover/goto/高亮且按文档/callable 分别解析以避免跨文档 span 碰撞，`code_check` 是批量
    字符串 gate 只产 `ok` 裁定（丢 span 仅留 `line N`），强行收敛会丢 LSP 所需精度，属恰当分层非重复；
    ③ **诊断 location 格式** `{path}:{row}`（语法层有文件归属）vs `line N`（`check_program` 无文件归属的限制）
    现集中在 `code_check.rs` 一处，保留；④ 废弃代码 / 注释语言 / 仅测试用 pub API（`dev_checklist_v0` 已注明
    有意保留）经查健康，无操作。
  - 状态：Accepted

- 2026-05-31 — **D1/D2/D3 集成验收演示题落地 + 修复 F2 `Http.Get` arity 潜伏缺陷**。
  - 决策：v1 完成判据 2 的三个演示题作为 F1/F2/S1/S2 的**端到端集成验收**（非语言扩充），落 benchmark
    阶梯顶端 **L6**：D1 `clamp_or_reject`（可失败返回 `one of { Int, OutOfRange }`，失败是返回值而非
    raise）、D2 `fetch_length`（`Http.Get → Raw<Text>` 经 intent_conversion 转 `Sanitized<Text>` 后取
    长度，经 mock host 确定性执行）、D3 `record_pipeline`（取→校验→落存储多 action 管线）。**D2
    accept/reject 矩阵拆两半**：accept 半（LLM 生成 + mock host 执行）落 benchmark L6；reject 半（不安全
    候选静态拒绝）落**确定性测试** `cli/tests/intent_matrix.rs`（不需 LLM/网络——「检查器对固定程序的
    裁定」本就该是确定性回归）。设计评审 `docs/integration_demos.md` 五决策点全部采纳。
  - 理由：D1/D2/D3 是判据 2「三演示题端到端跑通 + D2 真实 accept/reject 矩阵条目」的兑现。三题纯组合
    已落地能力、不引入新语言特征（符合需求驱动 + 有界）。reject 半落确定性测试而非 benchmark：它是
    静态裁定、与「LLM 生成 + 执行判定」形态正交，且可进 cargo test 守回归。TS「接受」半以可复现片段
    文档化、不引入 tsc 门禁（与「baseline 只做 Python」既有决策一致，benchmark_design §7.1）。
  - 影响：`runtime/src/verify.rs`（新增 `run_hidden_case_with_host` / `run_hidden_cases_with_host`——
    宿主工厂每 case 新建预置宿主，注入 D2 的 http mock；**不污染** `HiddenCase` 结构〔hidden.json 序列化
    格式，跨 graph/audit 复用〕）；`cli/examples/benchmark`（`NeutralTy` 增 `Text`/`OneOf`/`ErrorVariant`、
    `Level::L6`、`Problem.http_seed`〔两 mode 共享 mock url→body，绝不进 prompt〕、三题、sophia_mode 用
    host 变体、baseline 契约扩充 `one of` 失败成员 dict 对称 + runner 注入 mock `http_get`）；
    `cli/tests/intent_matrix.rs`（D2 reject 确定性矩阵）；`workflow/prompt/tests/render.rs`（L6 token）。
    **修复 F2 潜伏缺陷**：HIR `BUILTIN_EFFECT_OPS` 把 `Http.Get` 登记 arity=1，但按 F2 设计（`http_effect.md`
    §4.3）effect 身份**不带 URL arg**——声明形态 `effects { Http.Get }` / `allow { Http.Get }` 是 0 参
    （同 `Console.Write`），arity=1 导致 HIR `resolve_effect` 误判 `UnresolvedEffect`。改 arity=0；强化
    `core/hir/tests/resolve.rs::http_special_root_resolves_clean`（补断言无 UnresolvedEffect）+ 新增
    `http_get_capability_allow_resolves_clean`；修正 `http_effect.md` §4.2/§七表 arity 描述（标历史修正）。
    此缺陷潜伏原因：语义层测试绕过 HIR `resolve_effect`，且 D2 之前无对 Http 程序的**完整 code_check**
    （e2e/benchmark 此前无网络题）——D2 是第一个真正端到端跑 Http 程序的集成点，故此暴露。
  - 验证：三题手解验证均可表达、可执行（code_check 通过 + 解释器跑通成功 + 失败两路）；其间确认两条
    语言事实（非缺陷）——多参 `input` 须 `;` 分隔；`match` variant 字段绑定遵守禁 shadowing，与同名
    input 冲突时可用空字段 variant 模式 `Variant { } =>` 规避。全工作区 333 passed / 0 failed，clippy
    0 警告，fmt clean。真实 LLM 端到端实跑待 API key 环境。
  - 状态：Accepted

- 2026-05-31 — 标准库重定位：移除 `storage`/`DB`/`Persisted`，确立「I/O = 库」边界，规划 File 库
  - 决策：经讨论确立 **(B) 模型——文件 / 数据库 / 网络都是标准库，不是语言原语**（多数语言的传统）。
    据此：① **移除 `storage` 顶层节点 + `DB.Read/Write` 内置 effect 族 + `storage.X.get/save` body 语法**
    ——`storage` 语义不清（在 关系DB / KV / 持久化 / 内存映射 之间摇摆：有专属语法却只是按名分桶的
    内存 map、借道 `DB.*`〔关系型命名〕实现 KV、无任何持久化后端），是 v0 起步期"为有个有状态东西可
    演示"塞入的四不像，应移除而非续建。② **一并移除 `Persisted<T>` intent**（9→8）——它语义上属持久层，
    storage 移除后无任何 effect 产出/消费它，是死概念（含义不清就移除）。③ **`print` / `Console.Write`
    保留为语言内置**——输出是调试原语、几乎所有语言内置，且已走 effect/capability、无 ambient authority
    问题。④ **新增 File 库**（v1 内，优先级不低于 Http）：与 `Http` 同构的 effect+host 库
    （`File.Read(path) -> Raw<Text>` 等最小功能），走标准库设计评审流程。⑤ **DB 作为未来候选库**（语义清晰化
    后——明确 KV/关系/文档、后端、一致性——完成设计评审后引入），不在 v1。
  - 理由：核心洞察是区分**机制**（effect/capability/intent + 通用语法，属语言核心）与**具体 I/O 能力族**
    （Console/File/Http/DB，属库）。`Console`/`File`/`Http` 以 `BUILTIN_EFFECT_OPS`（Rust 常量表）承载是
    **实现事实**（`core` 零 IO、不能自举解析 stdlib 源码），不代表它们是语言原语。`storage` 之所以错位，
    是因为它被建模为带专属语法的**顶层节点**，而非库 effect 族——而它提供的能力（KV 存取）本就该是库。
    移除它消除一个语义不清的构造，并让"I/O = 库"的边界干净统一。
  - 影响（移除面，分文档方案 + 代码两阶段，本条记决策；代码重构随后）：
    grammar（`storage_def`/`storage_entry` + parser.c 重生成）、AST/lower（`Item::Storage`/`Storage`/
    `lower_storage`/`IncludeKind::Storage`）、HIR（`NodeKind::Storage`/`resolve_storage`/特殊根 `storage`
    放行/closure `Reads`/`Writes` 边 + `db_storage_of_effect`）、semantic（`StorageDecl`/`infer_storage_op`/
    `DB.Read/Write` effect/contract 层 storage 授权/union_check storage 分支）、runtime（`EffectHost::
    storage_get/save`/`InMemoryHost` storage 桶/`interp::try_storage_op`）、builtins（`DB.Read/Write` 出
    `BUILTIN_EFFECT_OPS`、`Persisted` 出 `INTENT_WRAPPERS`）、`IntentKind::Persisted`（semantic/ty.rs）。
    下游：共享语法基线删 §storage、`CompleteTodo.sophia`/`TodoDomain.sophia` 规范示例改写（去 storage /
    DB / Persisted）、删 e2e G5 组、benchmark D3（record_pipeline）改纯计算管线（落存储推迟到 File 库
    重做）、pipeline 集成测试去 storage、`Persisted<T>` 全文档替换为其它 intent（如 entity id 用裸 `Uuid`）。
  - 处置确认（用户决策）：Persisted **一并移除**；D3 **推迟到 File 库重做**（本轮先把 record_pipeline 改
    为不落存储的纯管线或暂移除，待 File 库落地重做）；执行方式 **先修文档方案、后重构代码**。
  - 落地（2026-05-31，R0 文档方案 → R1 移除 + R2 File 库代码）：R0 全截面文档改写；R1 全链路移除 storage /
    DB / Persisted（grammar→parser.c→AST→HIR→semantic→runtime→builtins + 下游示例 / 基线 / e2e G5 / benchmark
    record_pipeline / pipeline 测试 / snapshot 重生成）；R2 落地 **File 库**（`File.Read`/`File.Write` 与
    `Http` 同构：`infer_effect_op` 统一、`EffectHost::file_read/write` + `InMemoryHost` mock `seed_file` +
    `interp::try_effect_op`、CLI `CliHost` 真实 `std::fs`、`assets/stdlib/file.md` + catalog 行 + 回归测试）。
    手解验证 File 端到端可解。336 passed / 0 failed，clippy 0 警告，fmt clean。剩余 **R3**（benchmark D3 用
    File 重做 + baseline File mock 对称）待办。
  - 状态：Accepted（**取代** v0 起步期把 storage 作为内置顶层节点的设计；DB 重定位为未来候选库）

- 2026-05-31 — 标准库重定位 R3：D3 用 File 库重做 + e2e File 用例（标准库重定位收尾）
  - 决策：完成标准库重定位的最后一段——把 D1/D2/D3 集成验收的 **D3 严肃管线综合题**从历史 storage 版
    （`record_pipeline`，随 storage 移除已删）**用 `File` 库重做**为 benchmark L6 `archive_or_reject`，
    并**新增 e2e File 用例** G5-01（复用 storage 移除后空出的 G5 槽位）。
  - 理由：D3 是 v1 完成判据 2 的第三题（"能表达明显超出 v0 的程序"），storage 移除后唯一缺口就是把它
    的"落存储"步骤换成 `File` 库；e2e 此前 G5（storage）删除后留下文件读写能力的回归覆盖空缺，需补上。
    两者都是**演示验收**（非语言扩充），复用 R2 已落地的 `File` 全链路，不引入新特征。
  - 设计要点（D3 管线，复用 D2 的 intent 链路）：取（`File.Read(source)`→`Raw<Text>`）→ 校验
    （`CheckAmount` 的 `one of { Int, Rejected }` match）→ 经 `intent_conversion` 动作转
    `Sanitized<Text>` → 写出（`File.Write(dest, ...)`，写出边界只收 `Sanitized`）→ 读回
    （`File.Read(dest)`）返回字符数；校验失败原样返回 `Rejected` 失败结局。**关键语言事实**：裸 `Text`
    不能直接成 `Sanitized<Text>`（intent 严格相等），跨 intent 的唯一合法处是 `intent_conversion` 动作，
    故内容必须源自 `File.Read` 的 `Raw<Text>` 再经转换——与 D2 同构。
  - 影响：`cli/examples/benchmark/problem.rs`（`Problem` 增 `file_seed`〔path→content mock，两 mode 共享、
    绝不进 prompt〕）；`problems.rs`（`l6_archive_or_reject` + 注册）；`sophia_mode.rs`（verify 注入同时
    `seed_http` + `seed_file`）；`baseline_py.rs`（`baseline_request` 增 `has_file` + `file_clause`、写
    `file_seed.json`、runner 注入对称 mock `file_read`/`file_write`〔内存桶，read-after-write〕）；
    `cli/examples/e2e/cases/g5_file.rs`（新增 G5-01）+ `cases/mod.rs`（注册）；`workflow/prompt/tests/
    render.rs`（防泄漏 token 登记 `Archive`/`ArchiveCap`/`VaultCapability`/`StoreNote`）。文档：
    `integration_demos` / `benchmark_design` / `e2e_test_design` / `file_lib` 同步。
  - 验证：手解验证 D3 管线（clean check + 成功路径返回 Int 5 + reject 路径返回 `Rejected{amount}`）+
    e2e G5 骨架（write→read 往返返回 Int 5）均经临时测试确认可表达、可执行后删除。全工作区 336 passed /
    0 failed，clippy（`-D warnings`）0 警告，fmt clean。真实文件 IO / 真实 LLM 不进 `cargo test`。至此
    **R0–R3 标准库重定位全部完成** + **v1 完成判据 2（D1/D2/D3 + D2 accept/reject 矩阵）完整达成**。
  - 状态：Accepted

- 2026-05-31 — 工作流 A（WASM codegen）设计评审完成：七个决策点全部采纳
  - 决策：B 工作流全部完成（F1/F2/S1/S2 + 标准库重定位 R0–R3 + D1/D2/D3）后，按既定时序展开工作流 A
    （WASM codegen）。设计评审 `docs/wasm_codegen.md` 起草并经讨论**全部采纳**七个决策点：① body 不引入
    新 lowered IR（codegen 直接遍历 AST，与解释器同源，避免双真相源）；② 值 ABI = 标签化堆值 + i32
    句柄 + bump-only 线性内存（**无 GC / 无引用计数**）；③ 字符串 / 名字进 data section 常量池，动态值
    在 bump 区新建，字段按字典序（确定性，服务 strip-assist 字节稳定）；④ raise 用 `Outcome` 包装值
    （kind + handle）在返回通道冒泡，复刻解释器的跨调用边界冒泡，**不用 WASM 异常扩展**；⑤ 纯值运行时
    helper（拼接 / 相等 / Unicode 长度 / 值构造）生成进 module 自身（prelude 函数，自包含），**只有真正
    I/O effect 走 host import**；⑥ emit 用纯 Rust WASM 编码库、差测试用纯 Rust 解释执行器（均进
    `cargo test`），重型部署工具链（wasmtime / 浏览器）只作 artifact 下游消费者、**不进门禁**；⑦ 新建
    `tools/codegen` crate（确定性工具链层，依赖 core、禁 IO）；⑧ 差测试复用 benchmark/e2e 已被解释器
    跑通的参考解，不新造程序。
  - 理由：① 工作流 A 是 v1 完成判据 1（WASM 与解释器逐 hidden case 等价）+ 判据 3（strip-assist 推进
    到 artifact 字节级）的兑现，是「把语言从解释执行原型推进为可编译可部署严肃语言」的既定必经步骤
    （`language_design.md` §1.1 目标 1）。② **解释器是唯一语义真相源（oracle）**——所有决策都服务
    「WASM 输出与解释器等价、codegen 不反向改 IR」这条铁律；不引入新 body IR / 不引入 GC / 不引入 WASM
    异常扩展，都是为单一路线 + YAGNI（起步子集 body 极简、值生命周期简单、无并发）。③ 工具链选型遵循
    既有纪律：进 cargo test 的依赖必须纯 Rust、确定性、无重型系统依赖；真实 IO / 真实部署 host 不进门禁。
  - 影响（本条记决策；代码随 W1–W5 落地）：新建 `tools/codegen`（W1 骨架）；`tools/check` strip-assist
    扩展 artifact 字节比对（W5）；`cli/src/commands.rs::build` 从空操作改为 emit `.wasm`（W5）；差测试
    夹具复用 benchmark/e2e 参考解（W3–W4）。工具链具体库名 / 版本（WASM 编码库 + 纯 Rust 解释执行器）
    在 W1/W2 实现首步锁定并按「按需引入依赖」纪律登记。A6（增量查询架构，Salsa 思想）与 codegen 解耦、
    可并行 / 推后，不在本设计评审细化。
  - 验证：纯文档，无代码改动；全工作区测试基线不变（336 passed）。下一步进入 W1。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W1 落地：冻结 codegen 输入契约 + 新建 `tools/codegen` crate
  - 决策：按 `wasm_codegen.md` §九 W1 推进 codegen 第一阶段（A1）。新建 **`tools/codegen`** crate（tools
    层，依赖 `sophia-syntax` / `sophia-semantic` / `sophia-exec-ir`，**零 IO、不调 LLM、不改 IR**）。
    把 codegen 的三个**冻结输入**代码化为单一只读入口 `CodegenInput`：① `SemanticModel`（声明视图）；
    ② `ExecGraph`（callable 粒度执行图，由 `CodegenInput::new` 内部 `ExecGraph::from_model` 构建——与
    解释器 `Interpreter::new` **同源构图**，保证两后端看同一张图）；③ 全程序 AST（body 由 codegen 遍历
    AST 生成，与解释器同源；需静态分派时按 `ExprId` 经 `TypeChecker::check_callable` 重算 `TypeTable`，
    不要求 `analyze_program` 额外暴露它）。`emit_module` W1 阶段**诚实返回 `NotYetImplemented`**——不
    伪造空模块冒充产出（诚实性红线）。
  - 理由：A1 的本质是「codegen 消费 IR、绝不反向改 IR 形状」（语言实现 §12.1）——用一个**只读**的
    `CodegenInput` 把这条契约编码进类型（codegen 拿到的是 `&SemanticModel` / `&[&Ast]` / `&ExecGraph`，
    无可变入口）。`ExecGraph` 在 `CodegenInput::new` 内构建而非外部传入，确保 codegen 与解释器**同源
    构图**（差测试等价的前提之一）。不引入新 body IR、不要求语义 crate 改接口暴露 `TypeTable`（Table
    可重算，语义 6.2），均守「单一路线 + 不改 IR」。
  - 影响：新增 `tools/codegen/{Cargo.toml, src/lib.rs, src/contract.rs, src/error.rs, tests/contract.rs}`；
    根 `Cargo.toml` 注册 `tools/codegen` member + `sophia-codegen` 工作区依赖。无既有 crate 改动（纯新增）。
  - 验证：W1 契约测试（图↔模型 callable 一致含跨调用边 + emit 诚实占位）；全工作区 338 passed / 0 failed
    （336 + 2），clippy（`-D warnings`）0 警告，fmt clean。下一步 W2（最小 emit）。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W2a 落地：WASM 最小 emit（标量核心）+ 差测试夹具（A2 + A3 起步）
  - 决策：按 `wasm_codegen.md` §九推进 W2（A2 最小 emit）的第一增量 W2a（标量 + 控制流 + 跨调用核心），
    并顺带建立 W3（A3 差测试）夹具。① 工具链定版：`wasm-encoder` 0.243（emit）+ `wasmi` 0.40（差测试执行
    器，dev-dep）——均**纯 Rust、确定性、进 cargo test**；版本受 rust-version 1.80 约束（wasm-encoder 0.251
    需 1.85 / wasmi 1.0 需 1.86，故取兼容旧版）。重型部署工具链（wasmtime / 浏览器）**不进 cargo 树**
    （设计 §七）。② 值 ABI（`abi.rs`）：标签化堆值 + i32 句柄 + bump-only 线性内存（无 GC），`tag` 完整表
    （类比 exec-ir `EdgeKind` 保留完整词汇，W2a 仅产出 Unit/Bool/Int/Null，余者 `allow(dead_code)`）。
    ③ emit（`emit.rs`）：module = prelude（alloc / make_* / get_* / value_eq / wrap·outcome helper——纯值
    操作**生成进 module 自身**，不外置 host）+ 每 callable 一 function（i32 句柄×N → i32 Outcome 句柄）。
    覆盖标量值、字面量 / Ident / Not·Neg / 二元全算子（Add 按 `TypeTable` 重算静态分派 Int）/ if-else /
    let-set / return / 跨调用（Outcome kind 检查 + raised 原样上抛 + returned 取 value，复刻解释器错误冒泡）。
    ④ 未覆盖构造（match / repeat / raise / print / Text / List / Entity / State / effect）诚实返回
    `NotYetImplemented`，绝不伪造产出。
  - 理由：① **解释器是唯一 oracle**——emit 的每个指令序列与解释器 `eval` / `exec_stmt` 一一对应，差测试
    兜底等价（W2a 5 题全等价）。② 增量切分（先标量核心、后 Text/List/聚合/effect）让每步可独立差测试验证，
    避免一次性铺大面积难定位 bug；与"失败找根因 + 可泛化修复"纪律一致。③ 纯值 helper 进 module 自身而非
    host import（决策 ⑤）——保持 artifact 自包含、字节确定（服务 W5 strip-assist），只有真正 I/O effect
    才走 host import（W4）。④ `And`/`Or` 用 i32 and/or 而非短路控制流：起步子集二元操作数已是纯 Bool 值
    （无副作用表达式可因短路被跳过的语义差异），与解释器 `as_bool() && as_bool()` 结果一致；差测试确认。
  - 影响：`tools/codegen` 新增 `abi.rs` / `emit.rs`，`lib.rs` `emit_module` 接 `emit::emit`；`Cargo.toml`
    加 `wasm-encoder`（dep）/ `wasmi`·`sophia-runtime`（dev-dep）；根 `Cargo.toml` workspace.dependencies
    加 `wasm-encoder` 0.243 / `wasmi` 0.40。新增 `tests/diff.rs`（差测试夹具）；`tests/contract.rs` 的 W1
    占位测试更新为 W2 现实（标量程序 emit 出字节 + 未覆盖构造〔match〕诚实占位）。无既有 crate 改动。
  - 验证：codegen 8 测试（3 契约 + 5 差测试）全绿；差测试 emit→`wasmi` 执行→与解释器 oracle 逐 case 比对
    一致。全工作区 **344 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W2b（聚合值
    + match/repeat/raise）。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W2b 落地：WASM emit 错误代数 + `one of` 返回 + `match`
  - 决策：在 W2a 标量核心之上扩展 emit 覆盖**错误代数**——这是 benchmark L4（`raise`）与 D1（`one of`
    可失败返回 + 调用方 match）所需。① **ErrorValue 值布局**：具名记录 `[tag][name_ptr][name_len][nfields]`
    + 各字段 `[key_ptr][key_len][val_handle]`，**字段按 key 字典序**（与解释器 `BTreeMap` 一致 → 结构相等
    逐位可比 + emit 字节确定，服务 W5）。② **常量字符串区**：variant 名 + 字段名 intern 进 data section
    （按名字典序），bump 堆移到常量区之后。③ **新 prelude helper**（`str_eq` / `rec_field` / `rec_name_eq`）
    仍生成进 module 自身（纯值操作不外置，决策 ⑤）。④ emit 扩展 `raise`（→ ErrorValue → Raised Outcome →
    return）/ 返回的 variant `Construct` / `match`（subject 暂存 + 逐臂 if 链，支持 `Bool`/`Null`/标量
    `Type`/`Variant` pattern，含记录名比较 + 字段按名绑定）。⑤ `Eq`/`Ne` 按 `TypeTable` 守标量操作数
    （`value_eq` 仅覆盖标量，非标量相等待后续增量，避免误判）。
  - 理由：① 解释器仍是唯一 oracle——`match` 分派、ErrorValue 字段绑定、raise 冒泡的指令序列都与解释器
    `exec_stmt` / `match_pattern` 一一对应，差测试兜底（3 新题全等价）。② 字段字典序 + 常量池字典序 + 段
    顺序固定 → 同程序 emit 字节确定（W5 strip-assist artifact 比对的前提）。③ 增量切分继续：W2b 只做错误
    代数（值布局相对简单、无 Unicode/动态长度），把 Text/List/Entity/State/`repeat`/`Field` 留给 W2c，
    每步独立差测试验证。④ 诚实占位贯穿：未覆盖构造一律 `NotYetImplemented`，绝不伪造。
  - 影响：`tools/codegen/src/abi.rs`（具名记录布局常量）；`src/emit.rs`（`StrInterner` + data section +
    3 helper + `raise`/`match`/variant `Construct`/`Eq`·`Ne` 守卫 + `emit_variant_value`/`emit_match`/
    pattern test·bindings + `scalar_type_tag`）；`tests/diff.rs`（`ScalarOutcome` 增 ErrorValue/Raised 带
    字段 + 从线性内存读回记录 + 3 新差测试）；`tests/contract.rs` 的"未覆盖"测试改用 State match（仍占位）。
  - 验证：codegen 11 测试（3 契约 + 8 差测试）全绿；benchmark L1–L4 + D1 形态均经差测试等价。全工作区
    **347 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W2c（聚合值 + repeat +
    Field/entity Construct + entity·state match pattern）。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W2c 落地：WASM emit 聚合值（Entity + State）
  - 决策：在 W2a/W2b 之上扩展 emit 覆盖**结构化建模**——benchmark L2（entity 字段访问 rectangle_area /
    state match traffic_next）与 L5（entity 入参 + 跨调用 + 错误代数 checkout_limit）所需。① **State 值
    布局**：`[tag][state_ptr][state_len][value_ptr][value_len]`，state/value 名指向常量字符串区。
    ② **常量串区扩充**：entity / state 名 + 字段名 / 值名 intern 进 data section。③ **新 prelude helper**
    `make_state` / `state_name_eq` / `state_value_eq`（纯值操作仍进 module 自身，不外置）；把
    `emit_variant_value` 泛化为 `emit_record_value`——ErrorValue 与 Entity **共用具名记录布局**（tag 不同），
    字段按 key 字典序。④ emit 扩展 entity `Construct`、`Field`（`StateName.Value` → State 值 /
    `entity.field` → `rec_field` 取值，按 `TypeTable` 判 base 为 entity）、`match` 增 entity `Type`
    （tag==Entity + 记录名）/ state `Type`（tag==State + state 名）/ `State` 值 pattern（state 名 + value
    名双匹配）。⑤ 诚实占位：`repeat` / Text 值 / List / `Text.length` 伪字段 / 标准库 I/O / 嵌套记录构造
    字段 → `NotYetImplemented`。
  - 理由：① 解释器仍是唯一 oracle——entity/state 构造、字段访问、state 值 match 的指令序列都与解释器
    `eval` / `eval_field` / `match_pattern` 一一对应，差测试兜底（4 新题全等价，覆盖 L2 + L5）。② Entity 与
    ErrorValue 复用同一记录布局是自然的（都是"具名 + 有序字段"），避免重复代码；tag 区分二者的语义（被
    返回的失败成员 vs 普通实体）。③ 差测试夹具的 State 入参经"预留高地址区写名 + `make_state`"注入——
    这是把 State 值喂进 WASM 的确定性手段，不污染 bump 堆（高地址区远离低地址 bump）。④ 增量切分继续：
    Text（动态长度 + Unicode）/ List / `repeat` 留给 W2d。
  - 影响：`tools/codegen/src/abi.rs`（State 布局常量）；`src/emit.rs`（interner 扩充 + `make_state` /
    `state_*_eq` helper + `emit_record_value` 泛化 + `emit_entity_value` / `emit_field` + match entity/state/
    State pattern + `make_state` 导出）；`tests/diff.rs`（`Arg` 枚举 Int/State + `ScalarOutcome` 增 State/
    Entity + 从内存读回 + 4 新差测试 + `assert_equiv_args`）；`tests/contract.rs` 的"未覆盖"测试改用 `repeat`。
  - 验证：codegen 15 测试（3 契约 + 12 差测试）全绿；**benchmark L1–L5 + D1 全部形态经差测试与解释器
    等价**。全工作区 **351 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W2d
    （`repeat` + Text/List 值）。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W2d 落地：WASM emit Text 值 + `repeat`
  - 决策：在 W2a–W2c 之上扩展 emit 覆盖 **Text 值与有界循环**——补齐起步子集纯逻辑面。① **Text 值布局**
    `[tag][bytes_ptr][byte_len]`，bytes 指向常量串区（字面量）或 bump 堆（拼接结果）；字符串字面量经预
    emit 遍历 intern 进 data section。② **新 prelude helper** `make_text` / `text_length` / `text_concat` /
    `get_text_*`（纯值操作仍进 module 自身）；`text_length` 实现 **UTF-8 Unicode 标量计数**（统计非延续
    字节 top2bits != `10`），与解释器 `chars().count()` 一致——**非字节数**；`value_eq` 增 Text 字节比较。
    ③ emit 扩展 `Str` / `Add` 按 `TypeTable` 静态分派（Int 加 / Text 拼接）/ `Text.length` / match Text
    `Type` pattern / `Eq`·`Ne` 放行 Text / `repeat`（倒计数 i32 循环，body 内 return/raise 经 WASM `return`
    提前退出，与解释器一致）。④ 诚实占位：`print` / `to_text`〔Int→十进制串待后续〕/ `List` / 标准库 I/O
    → `NotYetImplemented`。
  - 理由：① 解释器仍是唯一 oracle——Text 拼接 / 长度 / 相等、repeat 循环的指令序列都与解释器
    `eval_add` / `eval_field` / `Stmt::Repeat` 一一对应；差测试 4 新题全等价（含多字节 Unicode "世界"=2
    标量、repeat 早退）。② `text_length` 的 Unicode 计数是**等价红线**（设计 §2.2 明列）——字节里存 UTF-8、
    length 走标量计数循环，不用字节数。③ `to_text`（Int→十进制）/ `List` 无 v1 演示需求触发（D1–D3 都不
    用），按 YAGNI 留占位、不投机实现。
  - 影响：`tools/codegen/src/abi.rs`（Text 布局）；`src/emit.rs`（interner 字符串字面量预 pass + 4 Text
    helper + `value_eq` Text 分支 + `Str`/`Add` 分派/`Text.length`/match Text/`repeat`/`Eq`·`Ne` 放行 +
    `loop_scratch` + Text helper 导出）；`tests/diff.rs`（`Arg`/`ScalarOutcome` 增 Text + 读回 + 4 新题）；
    `tests/contract.rs` 的"未覆盖"测试改用 `list`。
  - 验证：codegen 19 测试（3 契约 + 16 差测试）全绿；**全部 8 类值 + 全部纯逻辑形态经差测试与解释器
    等价**。全工作区 **355 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W4
    （effect host import）。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W4 落地：WASM effect host import（Console / File / Http）
  - 决策：emit 把副作用映射为 WASM **host import**（设计 §六）——benchmark D2（Http）/ D3（File）/ G2
    （Console）/ G5（File）的 effect 部分所需。① **5 个 `sophia_host` import**：`console_write` /
    `file_write` / `file_read`（→ len）/ `http_get`（→ len）/ `read_copy`，**字节级 ABI**（host 只收发
    线性内存字节缓冲、不识 Sophia 值布局——保持 host 简单 + 语言无关；File.Read/Http.Get 经 import 返回
    长度后由模块 `alloc` + `read_copy` + `make_text` 物化为 Text 值）。② **函数索引前移** `IMPORT_COUNT=5`
    （WASM 把 imports 排在最低索引）——`helper` 常量统一 `+IMPORT_COUNT`，53 处 call 站零改动；type 段同序
    前置 import 类型（`type_idx == func_idx` 不变）。③ **所有 module 统一声明这 5 个 import**（结构确定、
    字节确定，服务 W5），**真实 vs mock host 由实例化方提供**——capability 边界在编译期语义层已兑现，emit
    不重复检查、不按 effect 裁剪 import（裁剪是优化、非正确性，留作后续）。④ host 失败 → trap（解释器为
    硬错误阻断，差测试 mock 命中才返回，绝不伪造成功）。
  - 理由：① 解释器仍是唯一 oracle——`print` / `File.*` / `Http.Get` 的 emit 经 host import 委派，与解释器
    `EffectHost` 同一份委派语义；差测试用纯 Rust mock host（`Store<HostState>` + `Linker::func_wrap`，
    seed_file/seed_http 复刻 `InMemoryHost`、未命中 trap）跑通 G2/D2/D3 全等价。② 字节级 ABI 而非传值 ABI：
    host 跨 Node/Python/浏览器/wasmtime 异构，只认字节最通用；值的构造 / 解析留在 module 内（prelude
    `make_text` 等），与"纯值操作进 module、只有 I/O 走 host"（决策 ⑤）一致。③ 统一声明 import 而非按需
    裁剪：emit 确定性优先（W5 strip-assist 字节比对），裁剪是后续优化。④ 真实 host（`std::fs`/`reqwest`）
    不进差测试（与 S1/File 库"真实 IO 不进 cargo test"纪律一致）——差测试只用确定性 mock。
  - 影响：`tools/codegen/src/emit.rs`（`IMPORT_COUNT` + `imports` 模块 + `helper` 偏移 + import 段 +
    `push_import_types` + `emit_method_call`/`emit_io_read` + `print` emit + `io_a`/`io_b` scratch）；
    `tests/diff.rs`（`HostState` + `link_host` 5 import 桥接 + `Seeds` + `run_interp` 改 seeded
    `run_action` + 3 新 effect 差测试）。`A2`（最小 emit）随纯逻辑面全覆盖**关闭**。
  - 验证：codegen 22 测试（3 契约 + 19 差测试）全绿；**benchmark L1–L6（D1/D2/D3）+ G2/G5 全部形态经差测试
    与解释器等价**。全工作区 **358 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W5
    （strip-assist artifact 字节比对 + `sophia build` emit）。
  - 状态：Accepted

- 2026-05-31 — 工作流 A · W5 落地：strip-assist artifact 层门禁 + `sophia build` emit（A5；W1–W5 收尾）
  - 决策：完成工作流 A 的最后阶段。① **artifact 层 strip-assist 门禁**放在 `tools/codegen`（而非
    `tools/check`）——codegen 拥有字节 emit，门禁就近：`emit_from_sources(sources, strip)` +
    `check_artifact_strip_equivalence`（移除 assist 前后 `.wasm` 逐字节相等）。`tools/check` 继续管 IR 层
    （指纹 + 诊断），artifact 层是其在字节层的延伸（判据 3 = 两层都成立）。② **`sophia build`** 从 v0 空
    操作改为：check → artifact 门禁 → emit `sophia-runs/build/program.wasm`；codegen 未覆盖构造
    （`to_text`/`List`）**诚实报 `NotYetImplemented`、不伪造产出**（解释执行仍是可用后端）。③ `tools/codegen`
    加 `sophia-hir` 依赖（`emit_from_sources` 需 `resolve_program` 重建 index）——codegen 仍零 IO、确定性。
  - 理由：① 解释器仍是唯一 oracle——build emit 的字节经差测试（W3–W4）已证与解释器逐 case 等价；artifact
    门禁验证的是「assist 不泄漏进字节」（判据 3），与 IR 层门禁同源、字节层兜底。② artifact 门禁依赖 emit
    确定性（W2 起的值布局字典序 / 常量池稳定序 / 段顺序固定 / 统一声明 import）——这些早期决策此刻兑现为
    「同程序 emit 字节确定」，使 strip 前后逐字节比较成为可能。③ build 对未覆盖构造诚实失败而非降级
    （如静默回退解释器）——"待接入"诚实标注红线；`to_text`/`List` 无 v1 演示需求（YAGNI）。
  - 影响：`tools/codegen/src/build.rs`（新增 `emit_from_sources` + `check_artifact_strip_equivalence` +
    `ArtifactDiffOutcome`）+ `lib.rs` 导出 + `Cargo.toml` 加 `sophia-hir`；`tools/codegen/tests/diff.rs`
    （artifact 门禁 + 确定性 2 测试）；`cli/src/commands.rs::build` 重写 + `default_sophia_toml` target=wasm +
    smoke 注释；`cli/Cargo.toml` 加 `sophia-codegen`；`cli/src/main.rs` 注释；`cli/tests/pipeline.rs`
    （build emit + 未覆盖诚实报告 2 测试）；`engineering_architecture` §9.1 命令表。
  - 验证：全工作区 **362 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。**工作流 A 的
    W1–W5 全部落地——v1 完成判据 1（WASM 与解释器逐 case 等价，起步子集全覆盖）+ 判据 3（strip-assist
    artifact 层）达成**。剩余 A6（增量查询，与 codegen 解耦）待独立设计评审；真实部署 host 随部署需求接入。
  - 状态：Accepted

- 2026-05-31 — CI 流水线接入 + 修正失真的 MSRV 声明
  - 决策：把工程纪律固化进 CI（`.github/workflows/ci.yml`）。两 job：① **主门禁**（stable）—— fmt +
    clippy(`-D warnings`) + test + release build，全部 `--locked`（可复现：锁文件过期即失败，而非静默
    重解析）。`cargo test --workspace` **已含**工作流 A 的 A3 差测试（`tools/codegen/tests/diff.rs`）+
    A5 artifact 门禁，故无需为差测试加独立 job——它本就是确定性单测。② **MSRV 守护 job**——从 `Cargo.toml`
    读 `rust-version`、装该 toolchain 跑 build + test（fmt/clippy 版本无关，不重复）。**接入时发现 MSRV
    声明失真并据实校正**：`rust-version = "1.80"` 不可构建（逐版验证 1.80–1.94 均失败、1.95 通过），真实
    下限是 **1.95**——校正之 + 注释说明项目跟随最新稳定版。
  - 理由：① CI 把已有的本地纪律（fmt/clippy/test 全绿）变成不可绕过的门禁，A3 差测试（解释器 oracle vs
    WASM 等价）作为普通确定性单测自动被覆盖，无需特殊处理。② `--locked` 保证 CI 用提交的 Cargo.lock，
    避免「本地锁 + CI 重解析」的漂移。③ MSRV 守护：声明的 `rust-version` 若无 CI 验证必然腐烂——本次正是
    发现它早已失真（W1 时「为兼容 1.80 压低 wasm-encoder/wasmi 版本」的前提本就不成立，因 `sha2 0.11`/
    `rusqlite`/`reqwest` 的 transitive 依赖早已要求远高于 1.80 的工具链）。据实校正而非维持虚假声明，
    符合诚实性纪律。④ 不为修正 MSRV 而升级 wasm-encoder/wasmi（现版本可用、升级无收益，YAGNI）。
  - 影响：`.github/workflows/ci.yml`（重写为 check + msrv 两 job，全 `--locked`）；`Cargo.toml`
    `rust-version` 1.80 → 1.95 + 注释。无功能代码改动。
  - 验证：本地逐版确认 MSRV 下限 1.95（1.94 失败、1.95 通过 build+test）；1.95 上 362 passed / 0 failed；
    stable 上 `--locked` 的 fmt/clippy/test/build 全绿。真实 LLM e2e/benchmark 仍是 example、不进 CI。
  - 状态：Accepted

- 2026-05-31 — 测试三类化：去 mock 的 e2e/benchmark + 文档整理为三篇 test guide
  - 决策：确立 Sophia 测试**只有三类**，**不允许第四类**——① **单元测试**（进 `cargo test` 门禁、
    确定性、可离线，**唯一可 mock 的一类**，mock 用于隔离不完整 / 不确定的依赖）；② **e2e**（验证真实
    行为：真实 LLM + 真实 IO，**禁 mock**）；③ **benchmark**（与 Python 比成功率·耗时，**禁 mock**，纯
    逻辑题集）。原「集成演示 D1/D2/D3」**不是第四类**——它与 e2e / benchmark 概念重叠，按能力维度并入
    e2e（D1→G4-03 可失败返回 / D2→G2-03 网络+intent / D3→G5-01 文件管线）。Http 用稳定公开站点
    （`example.com`）真实访问、File 用真实临时文件，不 mock。
  - 理由：① **mock 会掩盖错误**——e2e / benchmark 的目的就是验证真实行为，用 mock 让题「确定」是「为
    通过测试走捷径」（AI 尤须避免的倾向），之前 benchmark D2/D3 用 mock host / 注入 `http_get` 即此类
    捷径。mock 仅在单元测试正当（隔离不完整代码的不得已手段）。② **不允许第四类**：新增测试类型只会
    混淆重叠的测试边界——「集成演示」与 e2e（验证正确性）、benchmark（横向对比）概念重叠，作为 v1 验收
    标准就该并入 e2e。③ 真实 IO 不进 `cargo test` 是关于「确定性门禁」的，而 e2e / benchmark 本就是
    example、不进门禁，故真实 IO 与确定性纪律不冲突。
  - 影响：**代码** ① `cli/src` 拆 bin→lib+bin（新建 `lib.rs` 导出协调层供 example 复用 `CliHost`）；
    e2e harness `execute_and_check` 据入口 `Http.Get`/`File.*` effect 注入真实 `CliHost`；新增 e2e
    G2-03（`FetchNonEmpty` 真打 example.com，断言可信文本非空）/ G4-03（`ClampOrReject` 返回 `one of`
    失败成员）；G5-01 改打真实临时文件。② benchmark 删 D2 `fetch_length`（http mock）+ D3
    `archive_or_reject`（file mock），留纯逻辑 `clamp_or_reject`；删 `Problem.http_seed`/`file_seed` +
    baseline runner mock 注入 + sophia_mode host 分支 + `NeutralTy::Text`（YAGNI）。③ 删
    `runtime::{run_hidden_case_with_host, run_hidden_cases_with_host}` + re-export + 3 个 host 变体单测
    （去 mock 后无调用方）。**文档** 删 `integration_demos.md` / `e2e_test_design.md` / `benchmark_design.md`，
    整理为一致格式的三篇 test guide `unit_test.md` / `e2e_test.md` / `benchmark_test.md`（精简设计篇幅、
    每用例说明、偏 test guide）；更新全仓引用（README / concepts / http_lib / file_lib / workflow_graph_spec
    截面 + Cargo.toml + example 注释 + intent_matrix）；防泄漏 token 增 `IngestCapability`/`FetchNonEmpty`、
    删已移除题 token。
  - 验证：全工作区 **359 passed / 0 failed**（362 − 3 个删除的 host 变体单测），clippy（`-D warnings`）
    0 警告，fmt clean。真实 LLM / 真实 IO 不进 `cargo test`（e2e/benchmark example、无 key 干净跳过）。
  - 状态：Accepted

- 2026-05-31 — 库插件模型：清单驱动 + 注册表 + 路线 B host + 标准库 crate（P1 落地）
  - 决策：把「库」从散落 6 个 crate 9 处硬编码切片，重构为 **清单（`library.toml`）= 单一真相源 +
    `LibraryRegistry` = 各层只读数据源**（倒转索引方向 `层→{库切片}` ⇒ `库→清单→注册表→各层`）。设计评审
    `docs/library_plugin.md` 经讨论全部确认采纳——① crate 拆分（`sophia-library` 契约类型 + `sophia-stdlib`
    内容）；② 解释器内嵌 wasmi 执行三方 WASM host（保 oracle 不变量，P2 接线）；③ 三方根目录
    `./sophia_libs/` + `$SOPHIA_LIB_PATH`；④ 复用 codegen 字节级 `sophia_host` ABI 作统一 host 契约；
    ⑤ `HostFn`/`HostRegistry` 落 `sophia-runtime`（需 `Value`）、`sophia-library` 只放无 `Value` 契约类型
    （避免环）；⑥ Sophia 源码库登记到「库名即 domain」隔离 domain；⑦ TypeDesc 先 `Scalar|Unit|Intent<Scalar>`；
    ⑧ 不支持 `abi_version` 启动报错。host 分派采**路线 B**（彻底——`HostRegistry: (family,op)→Box<dyn HostFn>`，
    非固定方法集 trait）。**标准库抽为 crate**。两条正交维度 **surface（Sophia 源码 / effect-op）× host
    （none / native / WASM）** 使纯 Sophia 库与 WASM 库在解释 / VM 两模式对称可用。
  - 理由：① 旧结构里「库」不是实体（散落硬编码 + 渗透语言核心），三方库无样板可循、无从入手——倒转索引让
    一份清单声明库在各层全部切片，各层消费注册表，核心去渗透（判定标准：`core/hir`/`core/semantic`/`runtime`
    不再出现 `File`/`Http` 字面量，`Console` 唯一例外）。② 路线 B 让 native 与 WASM host 同构为
    `Box<dyn HostFn>`，是跨模式对称的实现支点；早期追求最优设计，不取保守的「trait + 内部 match」（路线 A）。
    ③ 不能按模式割裂库能力（如「解释→Sophia 库 / VM→WASM 库」）——违背「解释器是唯一 oracle」铁律（用了某
    库的程序须两模式都能跑才有差测试基准）；故 surface × host 两维都不与模式绑定。④ intent 词汇属核心安全
    红线：清单 TypeDesc 的 intent 名经 `IntentKind::from_head` 解析，未知名保守恢复 `Ty::Error`，绝不放行
    库自定义 intent。⑤ 确定性门禁不受影响：注册表启动后冻结、确定性测试只用标准库注册表（不碰文件系统）。
  - 影响：**新增** `core/library`（`sophia-library`）+ `stdlib`（`sophia-stdlib`）两 crate；`runtime`
    新增 `host.rs`（`HostRegistry`/`HostFn`）、删 `effect_host.rs`（`EffectHost`/`InMemoryHost`）；
    `core/hir`（`builtins` 仅留 Console、`index` 经 registry 参数注入库契约，保留 `library_op`/`is_library_family` +
    `library_families`/`library_ops` 派生符号表、`resolve` 去 `File`/`Http` 字面量、`lib.rs` 加
    `resolve_program`）；`core/semantic`（`type_layer` 表驱动 `infer_effect_op` +
    `typedesc_to_ty`、`TypeChecker::new` 加 `&AsgIndex`、`analyze_one_callable` 加 `index`）；`runtime`
    （`interp` 用 `HostRegistry` + `try_effect_op` 按 `has_op`/`call` 分派、`lib.rs` `Execution.host` 改
    `HostRegistry`）；`prompt`（去 `STDLIB_ASSETS`/`stdlib_*` API、`implement_system_prompt` 收预渲染
    `stdlib_block`、删 `assets/stdlib/`）；`tools/check`（`check_program` 用 `standard_registry`）；
    `tools/codegen`（`CodegenInput` 持 `lib_index`、`emit_callable` 加 `lib_index`、`build` 用
    `standard_registry`）；`cli`（删 `http_host.rs`、`commands`/`graph_cmd`/`gate` 用 `library_registry()`
    + `register_native_hosts`）；`lsp`（`analysis` 用 `standard_registry`）；e2e harness / benchmark
    sophia_mode（registry catalog/preamble + native host）；codegen diff 测试（stdlib mock host）；
    各 core 单测（内联清单夹具 / 中性 Vault 库）。文档：`stdlib_design`/`stdlib_implementation` 重写吸收
    `library_plugin`（后者删除）、`engineering_architecture` §3/§4.1/§4.2/§8.3 更新、README 库文档行、
    `http_lib`/`file_lib` 加库插件重构横幅。新增 `toml` workspace 依赖（清单解析）。
  - 验证：纯重构、**零行为变化**（标准库 File/Http 端到端语义不变）。全工作区 **366 passed / 0 failed**，
    clippy（`-D warnings`）0 警告，fmt clean，`--locked` 一致。**新增库零改语言核心**（除扩 TypeDesc 须
    完成设计评审）。P2（三方动态发现 + 解释器内嵌 wasmi）待真实三方需求触发。
  - 状态：Accepted

- 2026-05-31 — 库插件 P2：三方动态发现 + 两个演示库（hash_sophia / hash_wasm）
  - 决策：实现 P2（三方库启动时一次性发现 + WASM host 内嵌 wasmi），用两个演示库验证两条干净的三方
    形态——`hash_sophia`（纯 Sophia 源码库，host=none）+ `hash_wasm`（WASM-effect 库，host=WASM），二者
    计算**同一确定 digest**（`acc=acc*31+value` ×3，起步子集可表达、小输入不溢出、确定）。**否决 sqlite
    作为第一个 P2 例子**:① WASM 沙箱 host 天然无文件系统/syscall,持久化 IO 需额外"沙箱授 host imports"
    机制;② 其 `query→rows` 是 `list of record`,TypeDesc 故意不支持;③ 与保留的 `DB` 标准库槽位撞车;
    ④ 把机制验证淹没在 C 库/VFS/持久化语义里。设计评审 `library_plugin_p2.md` 经讨论全部确认后消化并入
    `stdlib_design`/`stdlib_implementation`,原文删除。确认子决策:发现逻辑放 `sophia-stdlib`;`wasmi` 提
    `sophia-runtime` 正式依赖;VM 差测试一步到位(ABI 已统一);demo 做成 `cargo test` 集成测试(确定→进门禁)。
  - 理由：① P2 目的是验证**插件机制**而非上重量级真实库——"同一 digest 两种交付形态"对比点最干净
    （可观测行为相同、surface 不同:普通跨调用 action vs 特殊根 effect-op）。② 两 demo 全确定（纯计算 +
    fixture 发现 + wasm 确定执行）→ 进 `cargo test`,比 example 回归更强。③ 跨 domain 豁免（库 domain 对
    用户放行）是唯一触及语言核心的改动,与 task include 同类(显式可用的外部能力);库节点本身仍受全套静态
    检查,豁免只影响可见性。④ WASM host 复用 codegen `sophia_host` 字节 ABI(方向相反:host.wasm export)、
    标量 i64 直传——单一 ABI、无新增。⑤ `host.wasm` 用 wasm-encoder 测试时生成(不引入交叉编译工具链、不
    塞不透明二进制;ABI 契约真实,demo 目的达成)。
  - 影响：`core/hir`（`AsgIndex.library_domains` + `is_library_domain`、registry 参数填充、`resolve`
    跨 domain 豁免、`LibrarySources`〔库源码 owned AST〕、`HirError::LibrarySourceParse`）；`sophia-runtime`
    （`WasmHostFn` + `wasmi` 正式依赖 + `wasm-encoder` dev-dep）；`sophia-stdlib`（`discover` 模块:
    `full_registry`/`full_registry_from`/`third_party_roots`/`DiscoverError`、`stdlib_contents` 复用、
    fixture `tests/fixtures/sophia_libs/{hash_sophia,hash_wasm}` + 集成测试 `tests/library_demo.rs` + dev-dep
    hir/semantic/syntax/wasm-encoder）；`.gitignore`（忽略测试生成的 `host.wasm`）。文档:`stdlib_design`
    §五.1/§六 + §六.1 ABI + 变更记录、`stdlib_implementation` §2.3/§2.4/§三 + 变更记录。
  - 验证：发现 + 跨 domain 豁免 + 纯 Sophia 库执行 + WASM 库经 WasmHostFn 执行 + 两库逐位相等,全确定进
    `cargo test`。全工作区 **369 passed / 0 failed**,clippy（`-D warnings`）0 警告,fmt clean。host.wasm
    删后重跑自再生成(自包含)。**CLI 生产接线**（`full_registry` + 库源码并入命令 inputs + `sophia run`
    注册三方 WASM host）列为后续项——机制 + 确定性 demo 已就位。
  - 状态：Accepted

- 2026-05-31 — 库插件 P2 收尾：CLI 生产路径接线（发现 + 库源码并入 + WASM host 注册）
  - 决策：把三方发现机制接入 CLI 生产路径。① CLI `library_registry(root)` 由 `standard_registry` 改为
    `full_registry_for(root)`（新增,以**项目根**解析 `<root>/sophia_libs/`,而非 CWD 相对的 `third_party_roots`
    ——CLI 各命令以显式 `--root` 定位项目）;② 各命令经新增 `library_context(root)` 把库随附 Sophia 源码
    （`LibrarySources`）并入 program inputs + asts;③ `sophia run` 经新增 `register_wasm_library_hosts` 据
    注册表 `host.wasm` 注册三方 WASM host（`WasmHostFn`）。
  - 理由：① 库的 native-vs-WASM 由**装载方式**决定（标准库无 host.wasm → native;三方 effect-op 库 ship
    host.wasm → WASM）,而非清单声明——故 `register_wasm_library_hosts` 遍历注册表 `host.wasm` 字段判定,与
    native host 互补不重叠;三方 WASM op 多 `effectful=false`、不经声明 effect 体现,故无条件注册（native 仍
    按入口 effect 按需,纯逻辑零开销）。② 纯 Sophia 库节点（`SophiaDigest`）须并入 model 才可解析/执行,故
    库源码并入是必要而非可选;owned AST 在命令作用域持有,活到 run 全程。③ **strip-assist 门禁须 registry-aware**
    （`check_strip_assist_equivalence(sources, registry, index)`）:original/stripped 两侧用同一 registry +
    对称并入同一批库源码,否则用户引用库节点会让 strip 前后名称解析不对称、误判不等价——这是接线时发现的隐藏
    耦合。④ 确定性子门禁（`check_program`/`codegen`/LSP/graph gate 与 LLM 工作流命令）仍用 `standard_registry`:
    三方发现是协调层启动行为,不进核心确定性门禁（门禁 #7）。⑤ ABI 子集外签名 / host.wasm 装载失败诚实 `Err`
    阻断（不静默跳过、不伪造 host）——诚实性红线。
  - 影响：`stdlib/src/discover.rs`（`project_roots` / `full_registry_for` / `env_roots` 重构）;
    `stdlib/src/native_host.rs`（`register_wasm_library_hosts` + `ensure_i64_i64_i64_abi` + 3 单测）;
    `stdlib/src/lib.rs`（导出 + `stdlib_contents` 补 `host_wasm: None`）;`stdlib/src/discover.rs::read_library_dir`
    （读 `host.wasm`）;`core/library/src/registry.rs`（`LibraryContent.host_wasm` + registry 字段 + 访问器,
    已于前序提交准备）;`cli/src/commands.rs`（`library_registry`→Result、`library_context`、各命令并入库源码、
    `run_interpreter_action` 替换两旧 host helper）;`cli/src/graph_cmd/{mod,gate}.rs`（3 站点改 `standard_registry`）;
    `tools/check`（`check_strip_assist_equivalence` registry-aware、`check_program` 并入库源码）;5 个测试文件
    补 `host_wasm: None`;`stdlib/tests/library_demo.rs`（host.wasm 原子写,避免发现层并发读半写文件）。文档:
    `stdlib_design` §五.1/§五.3/变更记录、`stdlib_implementation` §2.3/§三/变更记录、`dev_checklist_v1`。
  - 验证：手动 smoke（项目带 `./sophia_libs/{hash_sophia,hash_wasm}`）:`check` 通过、`run ViaSophia`/`run ViaWasm`
    均得同一 digest 210523。全工作区 **372 passed / 0 failed**,clippy（`-D warnings`）0 警告,fmt clean。
  - 状态：Accepted

- 2026-06-03 — `sophia build` 上下文一致性修正（registry-aware artifact emit 单一路径）
  - 决策：把 `tools/codegen` 的源码 artifact API 直接迁移为显式接收 `LibraryRegistry`：
    `emit_from_sources(sources, registry, strip)` 与 `check_artifact_strip_equivalence(sources, registry)`。
    不保留无 registry 的旧便捷入口，也不新增 `*_with_registry` 双路径。CLI `sophia build` 不再调用
    `check(root)` 后丢弃上下文，而是在 build 唯一路径内组装 `library_context(root)`，用同一份 full registry
    + `LibrarySources` 完成名称解析、语义诊断、IR strip-assist 门禁、artifact strip-assist 门禁与最终 emit。
  - 理由：前一阶段 CLI `check` / `run` 已接入 full registry，但 `build` 在 artifact 阶段退回
    `standard_registry()` 且不并入库源码，导致 `check` / `run` 可通过的纯 Sophia 三方库项目在 `build` 阶段
    报未知调用目标。按全局单一路线原则，修复不采用兼容层 / fallback，而是让所有 artifact emit 调用点显式
    声明 registry 来源：确定性测试传 `standard_registry()`，CLI 生产 build 传 `full_registry_for(root)`。
  - 影响：`tools/codegen/src/build.rs`（并入 `LibrarySources`、`resolve_program`、`CodegenInput::new`）；
    `cli/src/commands.rs::build`（一次性组装完整上下文并复用 registry-aware IR / artifact 门禁）；
    `tools/codegen/tests/diff.rs`（显式 registry + 纯 Sophia 库 source emit 覆盖）；
    `cli/tests/pipeline.rs`（项目内 `sophia_libs` 纯 Sophia 库 build 覆盖）；
    `docs/cn/wasm_codegen.md` / `dev_checklist_v1.md` 同步 API 形态。
  - 验证：`cargo test -p sophia-codegen`、`cargo test -p sophia-cli` 通过；全工作区验证见提交 / PR 记录。
  - 状态：Accepted

- 2026-06-03 — `*_with_*` 接口审核与单一路径收敛
  - 决策：清理项目自有的兼容式 `*_with_*` / 双入口接口。① `CodegenInput::with_registry` 删除，
    `CodegenInput::new(model, asts, registry)` 成为唯一构造入口；② `resolve_program_with_libraries` 删除，
    `resolve_program(inputs, registry)` 成为唯一 HIR 解析入口；③ `AsgIndex::with_libraries` 删除，库契约通过
    `AsgIndex::new(registry)` / `AsgIndex::build(inputs, registry)` 显式注入；④ `runtime::run_action_with_host`
    与空 host 便捷 `run_action` 合并为 `run_action(model, asts, name, args, host)`，调用方必须显式传
    `HostRegistry`。CLI 私有 `run_with_host` 改名为 `run_interpreter_action`，避免把 host 组装误读为并行入口。
  - 理由：这些接口的实现都只是用默认空上下文 / 标准库上下文委托到“带上下文”版本，属于容易形成漂移的
    兼容分支。按单一路线原则，上下文来源必须由调用点显式表达：无库传 `LibraryRegistry::empty()`，标准库
    门禁传 `standard_registry()`，CLI 生产路径传 full registry；纯逻辑执行也显式传空 `HostRegistry`。
  - 保留项：`anyhow::with_context`、`Vec::with_capacity`、tracing builder 等外部 / 标准库 API 不属于项目接口；
    `with_base_url` / `with_repair_hint` 是 LLM 请求对象的 builder / 派生方法，无并行兼容入口；example 层
    `with_retry` 是 retry wrapper 构造器，不参与生产路径。
  - 验证：`cargo test --workspace` 与 `cargo check --workspace --examples` 通过。
  - 状态：Accepted

- 2026-06-03 — 三方 `host.wasm` 迁移为 ValueWire provider ABI（补齐 WASM runner 缺口 3）
  - 决策：删除解释模式三方 WASM host 的 direct i64 功能路径：`runtime::WasmHostFn::new_i64_i64_i64`
    不再保留，`WasmHostFn::new(wasm_bytes, op_contract)` 成为唯一入口。provider 必须导出 `memory`、
    `sophia_alloc(len)->ptr`、`sophia_read_copy(dst)` 与清单 `host_fn(args_ptr,args_len)->result_len`；
    实参与返回统一经 ValueWire 编码（Unit/Bool/Int/Text，intent 擦除为内层标量）。`runtime` 新增内部
    `value_wire` 模块，VM runner 与三方 provider 共用同一 encode/decode，避免双协议。
  - 理由：WASM 运行方案的缺口 3 要求解释模式与 VM 模式都能接入三方 `host.wasm`
    provider；旧 `(Int,Int)->Int` 直传 ABI 虽可演示 digest，但与 VM import 的 ValueWire ABI 分叉，违反
    “单一路线，拒绝多路径与向后兼容负担”。正确做法是直接迁移 fixture 与注册路径，不提供 fallback。
  - 影响：`runtime/src/wasm_host.rs`（ValueWire provider 调用、导出校验、trap/内存错误硬错误）；
    `runtime/src/wasm_program.rs`（复用 `value_wire`）；`stdlib/src/native_host.rs`（移除 ABI 子集签名拒绝，
    仅按装载方式注册 provider）；`stdlib/tests/library_demo.rs`（`hash_wasm` fixture 改生成 provider ABI）；
    `tools/codegen/tests/diff.rs`（新增 VM 动态 import → HostRegistry → 三方 provider 差测试）；
    文档同步 `stdlib_design` / `stdlib_implementation` / `wasm_codegen`。
  - 验证：`cargo test -p sophia-runtime`（含 Text→Text provider 单测）、`cargo test -p sophia-stdlib`、
    `cargo test -p sophia-codegen` 通过。失败语义保持硬错误：装载失败、导出缺失、签名不符、wasm trap、
    ValueWire 类型不匹配均不伪造成功。
  - 状态：Accepted

- 2026-06-03 — WASM build bundle manifest 与运行前漂移校验
  - 决策：在不引入第二条运行模型的前提下，采纳运行方案中优于当前实现的 bundle 审计能力：
    `sophia build` 除 `program.wasm` 外写 `program.sophia-build.json`，记录 `wasm_sha256`、
    `registry_fingerprint`、动态 import 清单、provider 类型，并把三方 `host.wasm` 拷贝到
    `sophia-runs/build/hosts/<library>/host.wasm` 记录 hash。`sophia run --backend wasm` 运行前校验 manifest
    存在、program.wasm hash、当前 registry fingerprint 与 bundle 内 host.wasm hash；不一致即硬错误提示重建。
  - 理由：文档指出的“裸 `program.wasm` 不是完整部署单元”确实比当前实现更优。此前 runner 只依赖当前项目
    registry，无法发现 build 后三方 host 资产漂移。manifest/hash gate 可以补上最关键的审计与防漂移能力。
    同时保留现有 `run` 单一路径：仍先加载项目源码并做语义检查来获得 `SemanticModel`，不做完全离线 bundle
    loader，也不提供 `--allow-registry-drift` fallback，避免形成第二套执行语义。
  - 影响：`cli/src/commands.rs` 新增 registry fingerprint / used-op 扫描 / manifest 写入与校验 helper；
    `cli/Cargo.toml` 直接依赖工作区 `sha2`；`cli/tests/pipeline.rs` 覆盖 manifest 写入、缺 manifest 拒绝、
    registry fingerprint drift 拒绝；`docs/cn/wasm_codegen.md` / `dev_checklist_v1.md` 同步落地状态与未来边界。
  - 验证：`cargo test -p sophia-cli --test pipeline` 通过。
  - 状态：Accepted

- 2026-06-04 — v2 D0 设计冻结（JSON MVP / Text / while / validator 返回模型）
  - 决策：冻结 v2 首批实现前提。JSON validator MVP 支持 object / array / string / int / bool / null /
    whitespace，暂缓 float / exponent / `\uXXXX` / JSON Schema；`Text.char_at` / `Text.slice` / `Text.starts_with`
    作为纯值操作进入 F1，其中索引采用与 `.length` 一致的 Unicode scalar index，`char_at` 负数或越界返回空
    `Text`，`slice` 对 start/length 做确定性夹取；`while condition { ... }` 进入 F2，condition 必须为
    `Bool`，body 复用 block scope，MVP 不含 `break` / `continue`，checker 不证明终止；`ValidateJson`
    返回 `one of { JsonValid, JsonInvalid }`，非法 JSON 是普通返回值而非 runtime failure。新增 `json.md`
    prompt asset 草案。
  - 理由：v2 的主线是让 Sophia 用纯 Sophia 三方库处理真实外部文本。`.length` 已经按 Unicode scalar
    计数，Text 新操作沿用同一单位可避免 parser 边界漂移；越界返回空 `Text` 降低游标探测样板，同时不放宽
    类型 / effect / host 失败的诚实边界；validator 返回 union 能把“输入非法”与“程序失败”区分开。
  - 影响：`docs/cn/json_lib_design.md` 写入冻结语义与 prompt asset 草案；`docs/cn/dev_checklist_v2.md`
    标记 D0.1-D0.5 完成并记录下一步进入 F1/F2。
  - 状态：Accepted

- 2026-06-04 — F1 Text 原语首片落地（semantic / interpreter）
  - 决策：利用既有 method-call syntax / AST 表达 `text.char_at(index)`、`text.slice(start, length)`、
    `text.starts_with(prefix)`，不新增语法节点；semantic 层按 Text receiver 表驱动校验 receiver、arity 与参数
    类型，并推导 `char_at` / `slice` 为 `Text`、`starts_with` 为 `Bool`；解释器实现 D0.2 冻结语义。WASM
    helper 与差测试保持为 F1.4 独立下一步，不伪造已完成。
  - 理由：现有 AST 已能稳定表达 receiver method call，新增专用语法会扩大表面积而无语义收益；先让 checker
    与解释器成为可测 oracle，再改 WASM prelude 函数表，可以把错误定位收窄。
  - 影响：`core/semantic/src/type_layer.rs`、`runtime/src/interp.rs`、`core/semantic/tests/analyze.rs`、
    `runtime/tests/interpret.rs`；`dev_checklist_v2.md` 标记 F1.1-F1.3 完成，F1.4/F1.5 仍待办。
  - 验证：`cargo test -p sophia-semantic text_parser_methods`、`cargo test -p sophia-runtime text_`。
  - 状态：Accepted

- 2026-06-05 — F1.4 Text 原语 WASM codegen 落地
  - 决策：在现有 WASM prelude 单一路径内新增 `text_char_at`、`text_slice`、`text_starts_with` helper，并由
    `emit_method_call` 根据静态 `TypeTable` 将 Text receiver 方法直接分派到这些 helper；库 op 仍走 registry
    host import 分派，不为 Text 原语引入 host op 或备用路径。`char_at` / `slice` 返回新的 Text handle，指向原
    Text 的 UTF-8 字节区间；空结果统一为 `make_text(0, 0)`。
  - 理由：Text 原语是确定性纯值操作，应与 `Text.length` / Text 拼接一样属于 core WASM prelude，而不是库
    host。按 UTF-8 非延续字节定位 Unicode scalar 边界，可与解释器 `chars()` 语义保持一致；返回区间 handle
    避免不必要复制，并符合现有字符串字面量指针 + 长度模型。
  - 影响：`tools/codegen/src/emit.rs` prelude 函数表与 method-call emit；`tools/codegen/tests/diff.rs` 新增
    `diff_text_parser_primitives` / `diff_text_starts_with`，覆盖 Unicode、空文本、越界、负数、slice 组合和空前缀。
  - 验证：`cargo test -p sophia-codegen`、`cargo clippy -p sophia-codegen --all-targets -- -D warnings`、
    `cargo fmt --all -- --check`。
  - 状态：Accepted

- 2026-06-05 — F1.5 Text 原语文档与 prompt baseline 同步
  - 决策：把 `text.char_at(index)`、`text.slice(start, length)`、`text.starts_with(prefix)` 及其 Unicode scalar
    / 越界语义同步到 `language_design` 的 body 子语言说明和 `sophia_syntax_baseline` prompt asset，并更新对应
    snapshot。F1.5 完成后，v2 F1 的 Text 最小解析能力全链路完成。
  - 理由：Text 原语是后续 JSON 库和 LLM 生成 parser 的前置能力；只在 checker/runtime/codegen 支持而不进入
    prompt baseline，会让 implement 阶段继续生成旧的 `repeat + length` 规避样板或误用不存在的字符串 API。
  - 影响：`docs/cn/language_design.md`、`workflow/prompt/assets/sophia_syntax_baseline.md`、
    `workflow/prompt/tests/snapshots/render__sophia_syntax_baseline.snap`、`docs/cn/dev_checklist_v2.md`。
  - 验证：`cargo test -p sophia-prompt syntax_baseline_preamble_is_stable`、`cargo test -p sophia-syntax
    documented_examples_parse_without_errors`。
  - 状态：Accepted

- 2026-06-05 — F2 while 控制流全链路落地
  - 决策：新增唯一语法形态 `while condition { ... }`，AST/HIR/semantic/runtime/WASM codegen 全链路直接支持；
    condition 必须为 `Bool`，body 复用现有 block scope 与禁止 shadowing 规则，解释器沿用 `Signal` 传播
    `return` / `raise`，WASM 以 `block` + `loop` + `br_if` 表达同一语义。MVP 不引入 `break` / `continue`，
    也不做终止性证明。
  - 理由：JSON parser 需要基于游标状态的同步循环，`repeat N times` 无法自然表达“读到分隔符 / 结束符为止”。
    直接把 while 接入现有语句、作用域、flow 与 WASM emit 路径，可以保持解释器为 oracle，避免专用 parser
    helper 或 host fallback。
  - 影响：`core/syntax` grammar / AST lowering 与生成 parser，`core/hir` resolve/closure，`core/semantic`
    type/effect/contract traversal，`runtime` interpreter，`tools/codegen` statement emit 与差测试，`cli` used-op
    扫描，`docs/cn/language_design.md` 与 prompt baseline。
  - 验证：新增 syntax / semantic / runtime / codegen 差测试覆盖 0 次、多次、状态提前结束、嵌套 while、
    while 内 `return` / `raise`；全量验证见本次 F2 收口检查。
  - 状态：Accepted

- 2026-06-05 — L1 JSON validator 作为纯 Sophia 三方库落地
  - 决策：JSON MVP 不做 host op、不进入标准库内建清单，而是在 `stdlib/tests/fixtures/sophia_libs/json/`
    以三方纯 Sophia 源码库实现。公开 API 为 `ValidateJson(text: Raw<Text>) -> one of { JsonValid,
    JsonInvalid }`；内部 helper 使用 `Text.char_at` / `Text.slice` / `while` 推进 cursor，返回 `JsonParseOk`
    或 `JsonInvalid` 传递位置。`JsonValid` / `JsonInvalid` 均为 entity，非法 JSON 是普通返回值而非
    runtime failure。
  - 理由：v2 的证明点是“库插件模型 + Sophia 自身表达 parser/validator”，host JSON 解析会绕开语言能力验证并
    迫使 `TypeDesc` 过早表达复杂返回。纯 Sophia fixture 还能同时验证三方 discovery、库 domain 豁免、解释器、
    WASM codegen 和 CLI build/run 的同一路径。
  - 影响：新增 json fixture 的 `library.toml` / `json.md` / `src/*.sophia`；`stdlib/tests/library_demo.rs`
    覆盖 discovery、semantic 和 interpreter hidden cases；`tools/codegen/tests/diff.rs` 覆盖 interpreter/WASM
    等价；`cli/tests/pipeline.rs` 覆盖项目三方库下的 `check`、interpreter `run`、`build` 和 WASM `run`。
  - 验证：`cargo test -p sophia-stdlib --test library_demo`、`cargo test -p sophia-codegen
    diff_json_validator_fixture`、`cargo test -p sophia-cli json_library_check_run_and_wasm_backend`。
  - 状态：Accepted

- 2026-06-05 — 工作流 LLM 单次调用墙钟上限
  - 决策：在 `workflow/llm` 的 HTTP 后端增加 `call_timeout_secs`，与连接 / 响应读取空闲超时分离；CLI
    graph `design` / `implement-loop` / `drive` 暴露 `--call-timeout-secs`，并支持
    `SOPHIA_LLM_CALL_TIMEOUT_SECS`。超时后返回 `BackendUnavailable`，由既有 RawLlmNode 失败路径落图，
    不伪造 Decision / Pseudocode / Code。
  - 理由：本地模型实现复杂目标时可能持续 stream token 但长期不发送结束标记；`read_timeout` 只限制空闲，
    无法中止“有进展但不结束”的单次生成。scheduler / repair 预算只能在调用返回后生效，因此必须在 LLM
    后端边界提供单次调用墙钟上限，避免人工等待成为流程的一部分。
  - 影响：`workflow/llm/src/backend.rs`、`cli/src/main.rs`、`cli/src/graph_cmd/mod.rs`、
    `docs/cn/custom_lib_usage.md`；e2e / benchmark 文档同步 timeout 语义。`0` 表示显式关闭墙钟上限。
  - 验证：`cargo test -p sophia-llm backend::tests::call_timeout_stops_non_finishing_future`、
    `cargo test -p sophia-cli graph_implement_loop_rejects_non_pseudocode_source`。
  - 状态：Accepted

- 2026-06-05 — Ollama 结构化输出接入 provider-native schema
  - 决策：`workflow/llm` 的 Ollama chat 请求在 `complete_with_schema` 路径携带 `format: <schema>`；
    普通 `complete` 不发送 `format`。后验仍走原有 JSON 提取 + `jsonschema` 校验，不新增第二套 schema。
  - 理由：OpenAI-compatible 路径已有可选 schema response_format，但 Ollama 路径此前只依赖自由文本后处理；
    真实 JSON validator implement 调试显示本地模型容易长篇输出或不收尾。把同一 schema 传给本地后端是
    泛化约束，不包含任务答案，也不改变 DecisionNode 自主选择动作的原则。
  - 影响：`workflow/llm/src/backend.rs`；`design_solution` / `revise_design` / `decompose` prompt 使用语言无关
    表述，避免在伪代码阶段泄漏后续实现形式、文件扩展名或构造词；`implement_design` prompt 增补“最小完整候选 /
    不输出 Markdown 或 schema 外字段”的通用输出纪律；`docs/cn/custom_lib_usage.md` 说明 Ollama structured 路径。
  - 验证：`cargo test -p sophia-llm ollama_schema_requests_use_native_format`。
  - 状态：Accepted

## 记录模板（供后续条目使用）

- YYYY-MM-DD — <简短标题>
  - 决策：<做了什么选择>
  - 理由：<为何如此选择>
  - 影响：<受影响的代码/流程>
  - 状态：Accepted | Superseded | Proposed
