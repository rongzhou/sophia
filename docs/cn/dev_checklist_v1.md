# Sophia 工程进展 · v1（dev_checklist_v1）

> **当前活跃的工程进展单一事实来源（SSOT）。** 每次大步功能合入后立即同步（见
> `engineering_notes.md` “文档同步纪律”）。v0（解释执行）阶段已冻结归档于 `dev_checklist_v0.md`
> （只读）；工程决策日志统一在 `engineering_notes.md`（跨版本，不分 v0/v1）。
>
> 本文档按 `language_implementation.md` §19.1 的 **v1 构建顺序**组织（两条并行工作流：A WASM codegen /
> B 语言 / 标准库扩充），并纳入从 v0 结转的开放项。状态：**已完成** / **进行中** / **尚未开始** /
> **路线图（v2+）**。
>
> v1 阶段定位与路线见 `engineering_architecture.md` §14.2；目标与价值主张见 `language_design.md` §1.1。

---

## 一、概述

**阶段目标（v1）**：把 Sophia 从“仅解释执行的原型”推进为“可编译、可部署的严肃语言”
（`language_design.md` §1.1 目标 1）。两条并行工作流缺一不可：

- **工作流 A — WASM codegen**：执行后端从解释器扩展到可部署 WASM artifact；解释器**不退役**，
  转为 codegen 的**等价 oracle / 差测试基线**；strip-assist 等价扩展到 artifact 字节级比对。
- **工作流 B — 语言 / 标准库扩充（需求驱动）**：**不是**固定特征清单，而是由具体演示需求触发、逐项
  过设计门的最小扩展。v1 范围由三个演示需求封顶（见 §二「v1 演示需求」）：**D1** 可失败结果建模、
  **D2** 网络获取 + intent 安全（旗舰 LLM-native 演示）、**D3** 严肃管线综合题；反推出最小扩展集
  **F1**（类型语法统一 + 可失败返回 `one of {...}`，**已完成**）+ **F2**（`Http` effect 族）+ **S1**
  （HTTP host 标准库）+ **S2**（标准库提示词脚手架）。其余起步子集外项（`entity.with` / 跨 domain 数据流 /
  合约证明 / `task` 执行）**显式推迟 v2+**，无 v1 需求触发。

**v1 完成判据（三者缺一不可）**：
1. 起步子集程序可经 WASM 后端编译，且与解释器结果**逐 hidden case 等价**（差测试全绿）；
2. **三个 v1 演示需求（D1 可失败返回 / D2 Http+intent / D3 严肃管线，见 §二）在 sophia mode 端到端跑通**
   （benchmark L6+ 题），其中 D2 能给出**一条真实 accept/reject 矩阵条目**（Sophia 静态拒绝不安全
   候选、TS baseline 接受）；
3. strip-assist 等价在 **artifact 层**成立（字节级比对）。

> 判据 2 是**有界**的：v1 语言扩充范围由 D1/D2/D3 三个演示需求封顶（= F1 类型统一 + 可失败返回 +
> F2 Http effect + S1 HTTP host + S2 stdlib prompt 脚手架），不做需求未触发的扩展。

**起点状态**：v0 核心链路（`parse → HIR → semantic → exec-ir → 解释器 run`）+ 工作流图闭环 +
e2e 六组 + benchmark 难度阶梯 L1–L5 均跑通真实 LLM；全工作区 299 passed / 0 failed。v1 尚未开始写码。

---

## 二、工作清单

### v1 演示需求（需求驱动的来源；工作流 B 由它封顶）

> 方法论：**需求 → 扩展，而非扩展 → 找用途**。两条准入通道，只有这两条：① **演示需求驱动**——为做
> 某个有说服力的演示 / benchmark 题，语言缺了某能力 → 引入**最小**扩展满足它，范围由需求封顶；
> ② **强论证的 LLM-native 特征**——门槛更高，须给出"为何专门服务 LLM 自动编程"的论证 + accept/reject
> 或可度量收益。不满足任一通道者一律 v2+，不进 v1 边界。

| 需求 | 展示什么 | 为何有说服力 | 触发扩展 |
| --- | --- | --- | --- |
| **D1** 可失败结果建模 | 会失败的计算 / storage 流程用**显式互斥结局**（`one of { T, SomeError }`）表达可恢复失败，调用方**强制 match** 全部成员 | v0 只有 `raise`（不可恢复）与"可空但丢失原因"；"可恢复带原因的失败"是真实硬缺口；`one of` 成员即自己、无包装样板 | F1 ✅ |
| **D2** 网络获取 + intent 安全（旗舰） | action 经 `Http.Get` 取回**不可信**数据（类型即 `Raw<Text>`），下游使用**必须经显式 intent 转换**否则静态拒绝；对照 TS+tsc 会接受"fetch 字符串直接当可信值" | 落地技术报告 §7/§8 旗舰主张：把"数据经历过什么"变成机器可检查的语言事实，构造真实 accept/reject 矩阵；**复用已实现的 intent + capability/effect + host 三套机制**，扩展面小、说服力大 | F2 + S1 + S2 |
| **D3** 严肃管线综合题 | 3–5 action 协作、带可失败步骤、带 effect 边界的端到端 pipeline（取→校验→计算→落存储） | 检验"语言扩充后能否表达明显超出 v0 的程序"这条完成判据；**不引入新特征**，纯组合 F1 + 既有能力 | 无（集成验收） |

> **暂不纳入 v1（留 v2+，无 v1 演示需求触发）**：跨 domain / library 协作演示（需 boundary +
> `sophia.lock`，报告 S3）、多轮演化演示（需 edit transition / Evolution Boundary，报告 S2）。价值高但
> 扩展面大、属独立子系统，放进 v1 会撑破边界。

### 工作流 A — WASM codegen

> 对应 `language_implementation.md` §19.1 工作流 A（step A1–A6）、§12.2 WASM emit 形态、
> `engineering_architecture.md` §14.2。原则：解释器是等价 oracle，codegen **不得**反向要求改 IR 形状。
>
> **执行时序（2026-05-30 决定）**：A 与 B 虽**同属 v1、缺一不可**，但**先做 B（F1→F2→S1→S2）、后做 A**。
> 理由：A1 要"冻结 IR 输入契约"，而 F1（类型系统统一）会动 Semantic IR / Execution Graph IR / `Value`——
> 若先冻结再被 B 改写，契约就白冻了。待 F1/F2/S1/S2 落地、**语言规范更稳固**后再展开 A，契约一次冻准。
> **WASM 仍在 v1 范围内、不可省略**（`language_design.md` §1.1 目标 1）。F1 已完成（IR 形态已含 `OneOf`/
> `Null`/`ErrorValue`），F2/S1/S2 仍可能动 IR，故 A 仍待 B 全部完成后展开。
>
> **设计门（2026-05-31，已定稿）**：B 全部完成（含标准库重定位 R0–R3、D1/D2/D3）后，工作流 A 的实现
> 计划落 `docs/wasm_codegen.md`（七个决策点全部采纳，已定稿）——定输入契约冻结 / 值 ABI / 函数 ABI /
> effect ABI / 工具链 / 差测试与 strip-assist artifact 门禁 / W1–W5 实现阶段。**解释器为唯一 oracle**
> 贯穿始终。

- [x] **A1 冻结 IR 输入契约**：文档化 v0 的 Semantic IR / Execution Graph IR 作为 codegen 输入契约
      （codegen 消费而非改写；契约稳定后再 emit）。设计见 `docs/wasm_codegen.md` §三。**已完成**——契约
      代码化冻结为 `tools/codegen` 的 `CodegenInput`（W1）。
- [x] **A2 最小 WASM emit**：标量 / 算术（含一元取负）/ 控制流 body → WASM function；
      entity / state / error → 值布局 + metadata。**已完成**——W2a–W2d emit 全部 8 类值
      （Unit/Bool/Int/Null/Text/ErrorValue/Entity/State）+ 全算子 + 全控制流（`if`-`else`/`match`/`repeat`）+
      跨调用 + entity·variant `Construct` + `Field`，经 `wasmi` 差测试与解释器逐 case 等价；`to_text`/`List`
      无 v1 演示需求触发（YAGNI 占位）。
- [x] **A3 差测试（differential testing）**：同一 `.sophia` 经解释器与 WASM 后端执行，逐 hidden case
      比对结果一致（解释器为 oracle）；接入 CI 的确定性部分。**已完成（确定性核心）**——差测试夹具
      `tools/codegen/tests/diff.rs`：emit + `wasmi` 执行 + 与解释器 oracle 逐 case 比对，19 个等价测试
      **覆盖全部 8 类值 + 全部语句/表达式形态 + 全部 effect**（benchmark L1–L6〔D1/D2/D3〕+ G2/G5 的程序
      形态：纯逻辑 / 错误代数 / `one of` / match / entity / state / Text / repeat / Console·Http·File +
      intent 转换）。在 `cargo test --workspace` 内、已接入 CI 确定性门禁。
      **可选增强（未做，非确定性门禁范围）**：把 WASM 差测试接进 benchmark/e2e 的**真实 LLM 候选**循环
      （`sophia_mode.rs::verify_candidate` 让每个 LLM 生成候选同时过解释器 + WASM 比对）——能在真实模型
      产物上守 codegen，但仅在有 API key 时跑（example，不进 cargo test）。差测试现用**手写程序覆盖
      上述形态**，非字面复用 LLM 生成的候选（后者无静态 `.sophia` 源、无法直接复用）。见下「从 v0 结转」。
- [x] **A4 effect / I/O 经 WASM imports**：副作用通过 host import 暴露；capability 边界在 host import 层
      兑现（与解释器 `EffectHost` 同一份语义）。**已完成**——5 个 `sophia_host` import（console_write /
      file_write / file_read / http_get / read_copy，字节级 ABI）；所有 module 统一声明、真实 vs mock host
      由实例化方提供；差测试经纯 Rust mock host（seed_file/seed_http 同 `InMemoryHost`、未命中 trap）跑通
      G2 Console / D2 Http+intent / D3 File 往返。
- [x] **A5 strip-assist artifact 比对**：strip-assist 等价门禁扩展到 WASM artifact 字节级比对；
      `sophia build` 从 v0 空操作变为真正 emit。**已完成**——`sophia-codegen::check_artifact_strip_equivalence`
      （移除 assist 前后 emit 的 `.wasm` 逐字节相等，判据 3）；CLI `build` check 通过 → artifact 门禁 →
      emit `program.wasm`；`smoke` 串通；未覆盖构造诚实报告不伪造。
- [ ] **A6 增量查询架构**（Salsa 思想）：支撑 LSP 低延迟；与 codegen 解耦，可并行推进。**尚未开始**


### 工作流 B — 语言 / 标准库扩充（需求驱动 + 逐项设计门）

> **纪律**：B 不是固定的实现序列，而是**需求驱动**（见上「v1 演示需求」）——每项扩展由一个具体演示
> 需求触发，且**先过设计门**（单独设计文档 → 讨论确认 → 再实现全链路 + 回归测试 + 文档），落地范式同
> `effect` / storage。不满足两条准入通道者一律 v2+，**不进 v1 边界**。
>
> **语法准则（`language_design.md` §3）**：不仿造泛型系统 / 模板 / 宏 / trait；语义直观、无省略、不惧
> 繁琐。`<>` 专属 Intent wrapper；结构类型用 `of` 关键字族（`list of`/`one of`/`schema of`，均为封闭内置
> 构造、非泛型系统）；可失败返回的 `one of` 成员直接构造 / 直接 `match`，不设 `?` / `unwrap` 等省略糖。
>
> **标准库范围准则（需求驱动 / 功能库）**：标准库是**功能库**，按演示需求增量——**只做用得到的功能层，
> 不做底层协议栈**（如：D2 只需 `Http.Get` 这一**功能**，就只提供它；**不**自建 TCP/IP、TLS、socket 等
> 底层协议——那些交给宿主语言 / host 运行时的成熟实现）。无需求不加。
>
> 每个 feature 三段式推进：**设计门 → 分层实现 → 演示验收**。

#### F1 — 类型语法统一（`one of` / `list of` / `<>`专属 intent）+ 可失败返回〔来源 D1/D3〕 ✅

> 准入：形成"失败分支必须显式处理"的可检查不变量 + 减少 LLM 漏处理失败路径。**否决了**最初的
> Rust 式 `Result<T,E>` 方案（被 Rust 心智带偏、wrapper 无信息量），改为 **`one of {...}` 联合类型**
> 直接表达可失败 / 可空返回（成员即自己、无包装子），并**顺势统一全部类型语法**。设计定稿见
> `docs/type_system.md`（取代已删的 `result_type.md`）。
>
> **核心规则（一条无例外）**：`<>` 专属 Intent Type；结构类型用 `of` 关键字族
> （`list of T` / `one of { M, ... }` / `schema of T`）。废弃 `Optional<T>`/`List<T>`/`Schema<T>`/
> `Some`/`None`/`<optional>.exists`，新增 `Null` 内置类型，match 引入类型 pattern。

- [x] **F1.0 设计门**：`docs/type_system.md` 定稿（核心规则 / `one of` 语义 + distinguishability /
      match 类型 pattern / `Null` / storage get 类型 / 全链路落点 §六 / 决策记录 §九）。**已完成**
- [x] **F1.1 syntax**：grammar.js `intent_type`（`<>`仅 intent）+ `list_of` / `one_of` / `schema_of` 类型
      规则；pattern 增 `type_pattern`（`<ty> <binding>`）/ `variant_pattern` / `Null`；表达式移除
      `some_expr`/`None`、新增 `Null` 字面；`tree-sitter generate --abi 15` 重生成 parser.c；AST
      `TypeRef`（`Named`/`Intent`/`ListOf`/`OneOf`/`SchemaOf`）、`Pattern`（`Bool`/`State`/`Null`/`Type`/
      `Variant`）、`Expr`（移除 Some/None、加 `Null`）+ lower。**已完成**
- [x] **F1.2 hir**：`SCALAR_TYPES` 加 `Null`/`Unknown`；`INTENT_WRAPPERS`（仅 9 个 intent）；`one of`
      成员逐个解析（类型 / state / error variant）；match 类型 pattern / variant pattern 绑定入 scope；
      `Null` 字面解析。**已完成**
- [x] **F1.3 semantic**：`Ty` 移除 `Optional`、加 `Null`/`OneOf(Vec<Ty>)`/`ErrorVariant`；`one of`
      distinguishability 静态检查；match 穷尽扩展到 `one of` 成员（类型 / variant / Null）；assignability
      成员 → 联合 upcast；storage get 返回 `one of { ValueTy, Null }`。**已完成**
- [x] **F1.4 runtime**：`Value` 移除 `Optional`、加 `Null`/`ErrorValue{variant,fields}`；`one of` 值
      **就是成员自身**（无包装变体）；match 按值实际 tag 分派；storage get 命中返回值本身、未命中
      `Value::Null`。**已完成**
- [x] **F1.5 storage**：`storage.X.get(key)` 返回类型 `one of { ValueTy, Null }`；`save` 仍返回 `ValueTy`
      （storage 不引入失败，待持久化后端）；type/effect/runtime + 既有用例同步。**已完成**
- [x] **F1.6 全仓改写 + 测试 + 文档**：共享语法基线 `sophia_syntax_baseline.md` 全面改写 + snapshot；
      例子 `.sophia`（TodoDomain / CompleteTodo / Control）、lowering / resolve / analyze / interpret /
      pipeline 测试、CST snapshot、benchmark value_json 同步；`language_design` §3/§6/§7 + 示例、
      `language_implementation` §7.1/§8.2/§16.1/§16.5/§16.6 + §14、`benchmark_design` / `e2e_test_design` /
      `architecture` §14.2 同步。全工作区 299 passed / 0 failed，clippy 0 警告，fmt clean；旗舰探针
      （`one of { Int, InsufficientFunds }` 直接返回 + match variant）端到端通过。**已完成**

#### F2 — `Http` 内置 effect 族 + host import〔来源 D2，旗舰 LLM-native〕 ✅

> 准入：落地技术报告 §7/§8 旗舰主张，可构造 accept/reject 矩阵（不可信网络数据经 intent 边界静态管控）。
> **零新语法**——`Http` 与 `Console`/`DB` 同构，复用现有 effect/capability + intent 机制。**设计与契约
> 已归拢到标准库文档**：语言契约见 `docs/http_lib.md` §二（effect / 类型 / capability / intent 边界，
> 三决策点），框架见 `docs/stdlib_design.md`。

- [x] **F2.1–F2.4**：hir（`Http.Get` 进 `BUILTIN_EFFECT_OPS`〔arity=0，见 `http_lib.md` §2.5〕、特殊根
      `Http` 放行）/ semantic（`infer_effect_op` → `Raw<Text>` + 并入 effect + url:Text 校验；intent 边界
      reject 由既有严格相等检查兑现，零改动）/ runtime（`EffectHost::http_get` + `InMemoryHost` 确定性 mock，
      未命中 `Err` 阻断）/ 回归测试（semantic 6 + runtime 2 + hir 1）。**`Http.Get` 形状不进常驻语法基线**
      （库知识归提示词脚手架按需资产）。**已完成**

#### S1 — 标准库：HTTP 客户端 host〔来源 D2〕 ✅

> 标准库 = host import 实现 + 语言侧约定，**严格按演示需求增量**，无需求不加；仅 workflow/runtime 层，
> `core` 零 IO。**功能库而非协议栈**：D2 只需 `Http.Get` 这一功能 → 只做它（基于 `reqwest`），不自建
> TCP/IP / TLS / socket 底层协议。**设计与机制已归拢**：`Http` 真实 host 见 `docs/http_lib.md` §3.3
> （CLI 协调层组合委派 / sync `reqwest::blocking` + 超时 / 网络失败硬错误 / 按入口 effect 判定注入），
> host 注入通用机制见 `docs/stdlib_implementation.md` §三。

- [x] **S1.1–S1.2**：workspace `reqwest` 加 `blocking` feature；`runtime::run_action_with_host` 注入入口
      （`run_action` 仍为默认）；`cli/src/http_host.rs` `CliHost`（组合委派 + 真实 `http_get`，非 2xx /
      网络失败 / 读取失败均诚实 `Err`）；CLI `run` 据入口 effect 含 `Http.Get` 才注入。回归测试覆盖接缝
      （注入路径 mock seed / console·storage 委派等价 / 非法 URL 诚实 `Err`，均不触网络）；真实网络人工
      验证、不进 `cargo test`。**已完成**

#### S2 — 标准库提示词脚手架（按需取用）〔来源 D2；提示词工程〕 ✅

> **问题**：LLM 对标准库**没有先验知识**，要它写出用到标准库的 `.sophia`，必须在 prompt 里告知库的用途
> 与用法。**设计与机制已归拢到标准库文档**：基线 vs 库资产边界、两阶段（design 看目录 `stdlib_catalog`
> 自选 → implement 注入 `stdlib_preamble`）、防泄漏 + snapshot 守护见 `docs/stdlib_design.md` §三；
> 资产布局 / prompt API / 全链路贯通见 `docs/stdlib_implementation.md` §一/§二。

- [x] **S2.1–S2.3**：`assets/stdlib/http.md` + `STDLIB_ASSETS` 表 + `stdlib_asset`/`stdlib_libs`/
      `stdlib_catalog`/`stdlib_preamble` API；两阶段按需注入（`design_result.libraries` LLM 自选 → 经
      `PseudocodeArtifact`/scheduler/`run_implement_loop`/`StepPrompts` 全链路贯通到 implement）；**不在
      任务元数据预声明库**（纠正初版 `Case.libs`/`Problem.libs`——泄漏解法 + 随三方库增长需推倒重做）；
      e2e / benchmark / graph CLI 三处接缝同步，graph 跨命令经伴生 `.libs` sidecar 持久化；回归测试
      （snapshot + 防泄漏断言 + 选取单测 + design 选库传播）。**已完成（含纠偏 + graph 缺陷修复）**

> 准入理由：标准库无 prompt 介绍则 LLM 无法使用，是 D2 演示能否跑通的**前置条件**；标准化布局 +
> 按需取用是"把库知识当可裁剪上下文资产管理"，契合 Sophia「语义可恢复 / 上下文裁剪」的 LLM-native 立场。

#### 集成验收 — 可失败返回 / 网络+intent / 文件管线（判据 2）

> 三项是 F1/F2/S1/S2 的**端到端集成验收**（非语言扩充）。**测试三类化后**（见
> `unit_test.md` / `e2e_test.md` / `benchmark_test.md`）：可失败返回 `one of` 的纯逻辑验收落
> benchmark L6（`clamp_or_reject`）；网络 / 文件验收一律落 **e2e 用真实 IO**（禁 mock）；intent
> reject 半落确定性单元矩阵 `cli/tests/intent_matrix.rs`。

- [x] **D1 可失败返回**：用 `one of { Int, OutOfRange }` 显式返回失败结局（非 raise），调用方 match
      全部成员（F1）。**已完成**——benchmark L6 `clamp_or_reject`（纯逻辑、确定）+ e2e G4-03
      `ClampOrReject`（真实 LLM 闭环，期望返回失败成员 `OutOfRange{value:15}`）。
- [x] **D2 网络 + intent（旗舰）**：`Http.Get → Raw<Text>` 经 intent_conversion 转换后使用。
      **accept 半**落 **e2e G2-03**（`FetchNonEmpty`，harness 注入真实 `CliHost` 打稳定站点 `example.com`，
      断言取回可信文本非空）；**reject 半**落确定性单元矩阵 `cli/tests/intent_matrix.rs`（Sophia 静态
      拒绝"Raw 直接当可信值" CHECK-INTENT-001、接受经转换候选；TS baseline 接受半文档化，不引入 tsc
      门禁）。**已完成**（含修复 F2 `Http.Get` arity 潜伏缺陷）。
- [x] **D3 文件管线**：组合 F1 `one of` match + `File` 库 effect/capability + intent 边界，**不引入新
      特征**。**已完成**——e2e **G5-01** `StoreNote`（`File.Write` → `File.Read` → intent 转换的
      write→read 往返，harness 注入真实 `CliHost` 打真实临时文件）。原 benchmark mock 文件题
      `archive_or_reject` 随测试三类化移除（网络 / 文件题不入 benchmark——禁 mock）。

#### 标准库重定位：移除 `storage`/`DB`/`Persisted` + `File` 库〔(B)「I/O = 库」模型〕

> 确立 **文件 / 网络 / 数据库都是标准库**（非语言原语），`Console`（`print`）保留为内置输出原语。
> 设计见 `stdlib_design.md`（框架）/ `file_lib.md`（File 库契约）；决策见 `engineering_notes.md`
> 2026-05-31 条目。**先修文档门、后重构代码**（用户既定执行方式）。

- [x] **R0 设计门（文档）**：`engineering_notes` 立移除 + File 库决策；`stdlib_design` 改写为「I/O = 库」
      模型 + §六库清单（Http 已落地 / File 设计门 / DB 未来候选）；`file_lib.md` 设计门草案；全仓截面文档
      （language_design / language_implementation / engineering_architecture / concepts / type_system /
      benchmark_design / e2e_test_design / integration_demos）去 storage/DB/Persisted、改 Http/File 库表述。**已完成**
- [x] **R1 移除 `storage`/`DB`/`Persisted`（代码）**：grammar（`storage_def` + parser.c 重生成）/ AST·lower
      （`Item::Storage` 等）/ HIR（`NodeKind::Storage`、特殊根 `storage`、closure Reads/Writes、`db_storage_of_effect`）/
      semantic（`StorageDecl`、`infer_storage_op`、`DB.Read/Write`、contract 授权、union_check）/ runtime
      （`storage_get/save`、storage 桶、`try_storage_op`）/ builtins（`DB.*` 出 `BUILTIN_EFFECT_OPS`、`Persisted`
      出 `INTENT_WRAPPERS`、`IntentKind::Persisted`）；下游：基线删 §storage、改写 `CompleteTodo`/`TodoDomain`
      示例、删 e2e G5、benchmark 移除 record_pipeline（D3 待 File 重做）、pipeline 测试去 storage、各 snapshot
      重生成。**已完成**
- [x] **R2 `File` 库（代码）**：按 `file_lib.md` 落地 `File.Read`/`File.Write`（hir `BUILTIN_EFFECT_OPS` +
      特殊根 `File` / semantic `infer_effect_op` File 分支 + intent 边界 / runtime `EffectHost::file_read/write`
      + `InMemoryHost` 内存桶 + `seed_file` + `interp::try_effect_op` 统一 File/Http / CLI `CliHost` 真实
      `std::fs` + 按 effect 判定注入）+ `assets/stdlib/file.md` + `stdlib_catalog` 行 + 回归测试
      （semantic clean/reject + runtime 往返/诚实 Err + CLI host 接缝 + 资产 snapshot）。**已完成**
- [x] **R3 D3 用 `File` 库重做**：benchmark L6 D3 改为"取 → 校验〔`one of` match〕→ 经 intent 转换 →
      `File.Write` 写出 → `File.Read` 读回"管线（`archive_or_reject`，含 baseline 的 File mock 对称支持
      `file_seed`）；e2e 补一个文件读写 + intent 边界用例（G5-01 `g5_file.rs`，复用 storage 移除后空出的
      G5 槽位）。**已完成**

> **显式推迟到 v2+（无 v1 演示需求触发，见上「v1 演示需求」末尾的"暂不纳入 v1"）**：`entity.with`、
> 跨 domain / library intent 数据流、`requires`/`ensures` 合约证明子系统、`task` 作为执行入口。出现
> 对应演示需求时再各自走设计门。

### 从 v0 结转的开放项（非 v1 头等，但仍在视野内）

- [ ] **`graph design` 的 `context_files` 接入**：graph Objective ↔ 项目 action root 的关联尚未建模，
      design 的 `context_files` 暂诚实留空（不臆造）。结转自 v0 §2.2。
- [ ] **`graph` 工作流子命令补全**：`decision` / `assess` 等（init/start/context/nodes/design/
      implement-loop/select/materialize 已贯通）。结转自 v0 §2.3。
- [x] **CI 流水线接入**：自动跑 fmt / clippy / test（含 A3 差测试 + A5 artifact 门禁，均在
      `cargo test --workspace` 内）+ release build + **MSRV 守护 job**（按声明的 `rust-version` build+test）。
      `.github/workflows/ci.yml`，全部 `--locked` 可复现。**已完成**
- [ ] **WASM 差测试接入真实 LLM 候选循环**（A3 可选增强）：让 benchmark/e2e 的 LLM 生成候选同时过
      解释器 + WASM 后端比对（`sophia_mode.rs::verify_candidate` 等），在真实模型产物上守 codegen 等价。
      仅在有 API key 时跑（example，不进确定性 CI）。A3 的确定性核心（手写程序覆盖全形态）已完成。
- [ ] **LSP 扩展**：rename / autocomplete / semantic navigation（增量分析见 A6）。结转自 v0 §2.3。
- [ ] **Execution Graph IR 远期调度愿景**：并发 / await / retry / cancellation / checkpoint 边语义
      仅作为 v4/v5 级远期方向保留；当前 Sophia 语言 / runtime / WASM codegen 均为**同步确定性执行**
      （起步子集仅 Control 调用边；其余边无表层来源，不是 v1/v2 近期补齐项）。结转自 v0 §2.3。
- [ ] **Tokio substrate 远期愿景**：仅在未来语言层明确引入异步执行语义、Execution Graph 可生成对应边、
      且 WASM/host ABI 有明确路线时重新进入设计门；当前 Rust async 只属于 LLM / LSP 等工具链 IO 实现细节，
      不代表 Sophia runtime 有异步执行目标。结转自 v0 §2.3。



### 路线图（v2+，不与 v1 争优先级）

> 见 `engineering_architecture.md` §14.3。

- [ ] 可选 backend：native（cranelift / LLVM lowering）；按需的具名语言 emit（TS / Python）。
- [ ] **演化能力**：edit transition 成为图一等动作 + Evolution Boundary；Semantic Identity；
      跨 domain / library protocol（`sophia.lock` / publish-consume / formal-only 视图）；
      更强 strip-assist（独立 IR / formal-only hash）。
- [ ] MessagePack 序列化（graph snapshots / runtime state / semantic cache）。
- [ ] Formatter（AST/HIR → 确定性 pretty printer）。

---

## 三、验证方式

每个 v1 步骤独立可合入、独立可测试，合入前必须全绿：

- 构建：`cargo build --workspace`
- 测试：`cargo test --workspace`
- 格式：`cargo fmt --all -- --check`
- Lint：`cargo clippy --workspace --all-targets -- -D warnings`

工作流 A 额外要求：**差测试**（解释器 vs WASM）逐 hidden case 等价，纳入确定性门禁。
真实 LLM 的 e2e / benchmark 仍是 example（不进 `cargo test`，无 key 干净跳过）。

---

## 四、变更记录

- 2026-05-30 — 建立 v1 进展跟踪文档。v0 进展归档冻结为 `dev_checklist_v0.md`（只读），本文档接任活跃
  SSOT，按 `language_implementation.md` §19.1 的 v1 构建顺序（工作流 A WASM codegen + 工作流 B 语言 /
  标准库扩充）组织，并结转 v0 未竟开放项。工程决策日志继续统一在 `engineering_notes.md`（不分 v0/v1）。
  v1 尚未开始写码；起点全工作区 299 passed / 0 failed。
- 2026-05-30 — 工作流 B 改为**需求驱动 + 逐项设计门**（审核纠偏）。原 B7–B12 是把 §16.6 的"扩展点
  标签"当成确定的实现序列，属过度设计 / 臆造。确立需求驱动方法论：先立演示需求
  （D1 Result / D2 Http+intent 旗舰 / D3 严肃管线），再反推最小扩展集（F1 Result<T,E> + F2 Http effect
  族 + S1 HTTP host）；其余起步子集外项（entity.with / 跨 domain 数据流 / 合约证明 / task 执行）因无
  v1 演示需求触发，**显式推迟 v2+**。完成判据 2 由"能表达明显超出 v0 的程序"（无界）收紧为"D1/D2/D3
  三演示题端到端跑通 + D2 给出一条真实 accept/reject 矩阵条目"（有界、可检查）。每项扩展先过设计门
  （设计文档→确认→实现），不预写实现细节。本次纯文档。
- 2026-05-30 — 把 v1 需求展开为可跟踪 checklist（工作流 B 任务化）。将 F1/F2/S1 从 feature 级条目
  细化为**三段式任务**（设计门 F*.0 → 分层实现 F*.1–* → 演示验收 D1/D2/D3），每个 feature 的实现项在
  其设计门通过前不启动；明确各项落在 syntax/hir/semantic/runtime/标准库哪一层。同步语言设计准则
  （`language_design.md` §3 新增"语义直观·无省略·不惧繁琐"原则 + §3.2 取舍行 + §12 Non-goal）：不仿造
  泛型系统 / 模板 / 宏 / trait / 操作符重载 / 隐式转换 / 省略糖；`Result` 等为封闭内置 wrapper（非泛型
  系统），消费走显式 `match`、不设 `?`/`unwrap`。`v1_demands.md` F1 同步对齐该准则。纯文档。
- 2026-05-30 — 删除临时分析文档 `v1_demands.md`，其实质（需求驱动方法论 + D1/D2/D3 演示需求）**内联**
  进本 checklist §二「v1 演示需求」，使 checklist 自包含。补两点澄清：① **标准库范围 = 功能库而非协议栈**
  （需求驱动；如只做 `Http.Get` 功能、不自建 TCP/IP / TLS / socket 底层）——写进 S1 与工作流 B 范围准则；
  ② 新增 **S2 标准库提示词脚手架**（LLM 对标准库无先验知识，必须有标准化、按需取用的库介绍 prompt 资产，
  复用 §8.3 preamble 机制；属提示词工程，v1 必须考虑）——分解为 S2.0 设计门 / S2.1 HTTP 库资产 / S2.2 按需
  注入机制 / S2.3 回归测试。最小扩展集相应记为 F1 + F2 + S1 + S2。纯文档。
- 2026-05-30 — 启动 v1。① **执行时序定为先 B（F1→F2→S1→S2）后 A（WASM）**：A1 要冻结 IR 输入契约，
  而 F1（Result）会动 Semantic IR / Execution Graph IR / Value，故先做 B 让语言规范稳固、再一次冻准
  IR 契约展开 A；WASM 仍在 v1 范围内、不可省略（工作流 A 标注此时序）。② **F1.0 设计门起草**
  `docs/design/result_type.md`：基于现有 Optional/List wrapper + error algebra + match 穷尽 + RaisedError
  等既有机制，提出 `Result` 为**单参数封闭 wrapper（`Result<T>`）+ 失败侧复用 action 的 errors variant
  集**（方案 C，避免单参→多参泛型化、不造新错误体系），消费走显式 `match Ok/Err`、不设 `?`/`unwrap`
  省略糖，符合"不仿造泛型 / 语义直观无省略"准则；列出三个待确认决策点（语法形状 / Err 穷尽形式 /
  storage save 形态回归）。待确认后进入 F1.1–F1.6 实现。纯文档。
- 2026-05-30 — **F1 完成（含设计纠偏）**。经讨论**否决** Rust 式 `Result<T,E>`（被 Rust 心智带偏、方案 C 下
  wrapper 无信息量），删 `result_type.md`，改写 `docs/type_system.md` 定稿：可失败 / 可空返回用
  **`one of { 成员, ... }`** 联合（成员直接构造 / 直接 match、无 `Ok`/`Err`/`Some`/`None` 包装子），并**统一
  全部类型语法**——`<>` 专属 intent、结构类型用 `of` 族（`list of`/`one of`/`schema of`）、废弃
  `Optional`/`List<>`/`Schema<>`/`Some`/`None`/`.exists`、新增 `Null` 内置类型、match 引入类型 pattern。
  **全链路一次性彻底重构**（grammar+parser.c / syntax AST+lower / hir / semantic / exec-ir / runtime /
  benchmark value_json）+ 全仓测试 / snapshot / 示例 / 文档同步（见 `engineering_notes.md` 同日条目）。
  storage `get` 返回 `one of { ValueTy, Null }`、`save` 仍返回 `ValueTy`。单一路径、无兼容层、不留语法糖。
  全工作区 299 passed / 0 failed，clippy（`-D warnings`）0 警告，fmt clean；旗舰探针端到端通过。F1 关闭，
  下一步 F2（Http effect 设计门）。
- 2026-05-30 — **F1 重构完整审核 + 补两处检查器缺口**。全仓审核（代码 + 文档）确认重构彻底、无遗留旧
  类型构造（`Ty::Optional`/`Value::Optional`/`Expr::Some`/`TypeRef::Generic` 全 0 命中）；清理散落旧语法
  注释（`match Some(name)`/`Optional<T>`/`Schema<T>`/`Some(/* c */ 5)` 等）。审核发现并修复**两处设计-实现
  缺口**：① `one of` 成员**可区分性**（设计 §2.2/§九.5，曾列为 F1.3 交付项但未落地）—— `one of { Int, Int }`
  / `one of { Raw<Text>, Text }`（intent 运行时擦除）此前静默通过；新增 `union_check` 模块（按 match tag 判定，
  intent 擦除展开、嵌套联合展开）+ 诊断 `IndistinguishableUnion`（CHECK-TYPE-006），全程序类型位置遍历；
  ② match 类型 pattern 的**类型名未解析**——`match x { Bogus v => }` 此前无诊断，HIR 补 `resolve_pattern_type_name`
  （标量 / entity / state）。新增 6 个测试（4 distinguishability + 1 pattern 类型名 + 1 type-of 语法族 lowering）。
  全工作区 305 passed / 0 failed，clippy 0 警告，fmt clean。parser.c 重生成幂等校验通过。
- 2026-05-30 — **F2 落地（`Http` 内置 effect 族）**。设计门 `docs/http_effect.md` 定稿（§八 三决策点确认：
  ① effect 身份 `Http.Get` 不带 URL arg〔capability 粒度到"能否 GET"，URL 多为运行时绑定值〕；② 返回裸
  `Raw<Text>`，网络失败走 host 硬错误，不在 v1 建模为可恢复返回；③ `Http` 与 `storage` 同列 body 特殊根）。
  **零新语法**——`Http.Get(url)` 复用 storage 的"特殊根 method_call + host 委派"路径。落地 F2.1（hir：
  `Http.Get` 进 `BUILTIN_EFFECT_OPS`、`Http` 特殊根放行）/ F2.2（semantic：`infer_effect_op` → `Raw<Text>`
  + 并入 effect + url:Text 校验；intent 边界拦截复用既有严格相等检查，零改动）/ F2.3（runtime：
  `EffectHost::http_get` + `InMemoryHost` 确定性 mock〔`seed_http`，未命中 `Err` 阻断、绝不伪造〕+
  `interp::try_effect_op`）。**D2 旗舰的 accept/reject 拦截点已验证**：`Http.Get` 的 `Raw<Text>` 直接当
  `Sanitized<Text>` 用 → `IntentMismatch`（reject）；经 `intent_conversion` 转换后 → 通过（accept）。
  9 个新测试（semantic 6 + runtime 2 + hir 1）。**F2.4 明确不改常驻语法基线**——Http 库知识归 S2 按需
  资产（`prompt/assets/stdlib/http.md`），避免污染无关任务 context（与 S2「按需取用」一致）。
  `language_design` / `language_implementation` 同步内置 `Http` 族。
- 2026-05-30 — **S2 落地（标准库提示词脚手架）**。选 S2 先于 S1：S2 是 D2 演示的真正前置（LLM 对 `Http`
  无先验知识则写不出网络程序），而 F2 mock host 已能让全链路确定性跑通，**S1 真实网络非 D2 前置**（只
  live demo 需要）。设计门 `docs/stdlib_prompt_scaffolding.md` 定稿（§七四决策点确认：① 按任务**显式声明
  库集合**选取〔非文本嗅探，确定可测、符合「全部显式表达」〕；② 布局 `assets/stdlib/<lib>.md` + API
  `stdlib_asset`/`stdlib_libs`/`stdlib_preamble`；③ 库资产同防泄漏 + snapshot 纪律；④ S2 不依赖 S1）。
  核心确立**基线 vs 库资产边界**：常驻 `sophia_syntax_baseline`=核心语法（每次注入），`stdlib/<lib>`=库知识
  （按需注入），契合「上下文裁剪」。落地 S2.1（http.md + 三 API）/ S2.2（e2e `Case` + benchmark `Problem`
  加 `libs` 字段，三处 implement-system 接缝拼入 `stdlib_preamble`，默认空集零回归）/ S2.3（snapshot +
  防泄漏断言 + 选取单测）。全工作区 319 passed / 0 failed，clippy 0 警告，fmt clean。下一步 S1（真实
  reqwest host）或 D2 演示题（benchmark L6，需真实 LLM）。
- 2026-05-30 — **S1 落地（HTTP 客户端真实 host）**。设计门 `docs/http_host.md` 定稿（§七四决策点确认）。
  真实 host 在**协调层 CLI**（`runtime` 保持零 IO）：`cli/src/http_host.rs` `CliHost` **组合委派**——
  console/storage 复用 `InMemoryHost`，只把 `Http.Get` 覆盖为真实 `reqwest::blocking`（固定 10s 超时；
  非 2xx / 网络失败 / 读取失败均**诚实 `Err`**、绝不伪造成功）。runtime 暴露 `run_action_with_host`
  （`run_action` 仍为默认入口，薄封装化）；CLI `run` 据入口 action 声明 effect **含 `Http.Get` 才注入
  `CliHost`**（无网络程序零开销、零行为变化）。workspace `reqwest` 加 `blocking` feature。
  测试：runtime 注入接缝（`run_action_with_host` + mock seed）+ CLI `http_host` 单测（console/storage
  委派等价 + 非法 URL 诚实 `Err`，均不触网络）；**真实网络不进 `cargo test`**。全工作区 322 passed /
  0 failed。至此 F1+F2+S1+S2 全部落地；下一步 D1/D2/D3 集成验收演示题（benchmark L6，需真实 LLM），
  或工作流 A（WASM codegen，IR 契约现已含 one of / Http effect，可冻结展开）。
- 2026-05-30 — **S2 纠偏：库选择改为 design 阶段 LLM 看目录自选**。否决初版把库选择放进任务元数据
  （`Case.libs` / `Problem.libs`）——经讨论确认那（a）有作弊嫌疑（提前泄漏解法方向，库选择本应是 design
  决策）、（b）随三方库增长必然推倒重做。改为**两阶段**：design / revise 注入**库目录**（`stdlib_catalog`，
  每库一行用途、无操作签名），LLM 在 `design_result.libraries` 自选；implement / repair 据所选库注入
  **完整资产**（`stdlib_preamble`）。`libraries` 经 `PseudocodeArtifact` → scheduler → `run_implement_loop`
  → `StepPrompts::implement/repair` 全链路贯通；移除 `Case.libs`/`Problem.libs`/`*Prompts.libs`。graph 跨
  命令经伴生 `.libs` sidecar 持久化所选库（design 写、implement 读回）。325 passed / 0 failed，clippy 0 警告，
  fmt clean。设计文档 `stdlib_prompt_scaffolding.md` §一/§三/§四/§六 改写为两阶段；`engineering_notes` 立纠偏决策条目。
- 2026-05-30 — **修复 S2 graph 路径已知缺陷**（库选择跨命令丢失）。S2 纠偏后 graph design 已产出
  `PseudocodeArtifact.libraries`，但 `graph design` 只把伪代码正文落盘到 `.pseudo` sidecar、**丢弃了
  libraries**，`graph implement-loop` 读回正文后传空集——故 graph 路径上 LLM 在 design 选的库（如 `http`）
  到 implement 阶段拿不到，两阶段机制对 graph CLI 实际**断裂**（仅进程内 e2e/benchmark 路径有效）。修复：
  `graph design` 把所选库写入伴生 `<node>.libs`（每行一库；空集不写文件），`graph implement-loop` 经
  `read_pseudo_libraries` 读回并传入 `run_implement_loop`（缺文件视为无库，向后兼容旧产物）。与 spec
  §4.4.3「节点轻量、正文落 sidecar」一致——libraries 同属 design 产物，走同样 sidecar 模式（非图节点
  payload）。新增 sidecar 往返单测（写读 / 空集不写 / 缺文件不 panic）。326 passed / 0 failed。
- 2026-05-31 — **技术债清理（一）：收敛三处重复**。经 context-gatherer 全仓勘查后，去除长期增量开发积累
  的两处逐字重复：① `code_check` 桥接（语法 → HIR + 语义三层 + strip-assist 等价）此前在 CLI `graph_cmd`、
  e2e harness、benchmark sophia_mode **三处各一份**（约 240 行），收敛到唯一实现 `sophia_engine::code_check`
  + `domain_of_path`（workflow 层，与 `run_implement_loop` 同层；engine 加依赖 `sophia-syntax`/`sophia-check`；
  harness/benchmark 的 `real_check` 仅留打印薄包装）；② system prompt 文案（语法基线 + 按需库资产 + 输出
  形状）同样三处重复，收敛到 `sophia_prompt::design_system_prompt()` / `implement_system_prompt(libraries)`
  单一来源，顺带修复 design system prompt 漏 S2 `libraries` 字段的漂移。326 passed / 0 failed，clippy 0 警告。
- 2026-05-31 — **技术债清理（二）：修复文档漂移**。`CHANGELOG.md [Unreleased]` 补 F1/F2/S1/S2 条目；
  `README.md` 补 v1 设计文档（type_system / http_effect / http_host / stdlib_prompt_scaffolding）引用；
  `engineering_architecture.md` §8.3 更新 prompt crate API（stdlib + system prompt 单一来源 + `assets/stdlib/`
  布局）、§8.4.2 骨架免责说明补 `libraries` 参数对齐实际签名。纯文档。
- 2026-05-31 — **技术债清理（三）：拆分膨胀文件 graph_cmd**。最大文件 `cli/src/graph_cmd.rs`（1351 行）
  按职责拆为模块目录：`graph_cmd/mod.rs`（757 行——确定性命令 init/start/context/nodes + design/implement-loop
  LLM 命令）+ `graph_cmd/gate.rs`（624 行——select/materialize 的 gate 重跑：code_check + constraint_audit +
  hidden-case 真实执行 + artifact_diff/runtime validation）。共享 helper（`open_store`/`parse_node`/
  `artifacts_dir`/`write_code_artifacts`）以 `pub(super)` 提供，`select`/`materialize` 经 `pub use` 重导出
  保持 `main.rs` 调用点不变；测试随各自模块迁移。326 passed / 0 failed，clippy 0 警告，fmt clean。
  评估保留项（非债务）：H3（harness vs sophia_mode 闭环驱动形似）受「故意不复用 e2e harness」既有决策约束 +
  上下文不同；M1（LSP `analysis.rs` 诊断收集）与 `code_check` 同源不同层——LSP 需精确 span 供编辑器，属恰当
  分层；诊断 location 格式 `line N`（check_program 无文件归属的限制）现集中一处。详见 `engineering_notes.md`。
- 2026-05-31 — **D1/D2/D3 集成验收演示题落地（判据 2 达成）**。设计门 `docs/integration_demos.md`
  定稿（五决策点全部采纳）：三题是 F1/F2/S1/S2 的端到端集成验收（非语言扩充），落 benchmark 阶梯顶端
  **L6**——D1 `clamp_or_reject`（可失败返回 `one of`，失败是返回值非 raise）、D2 `fetch_length`
  （`Http.Get → Raw<Text>` 经 intent_conversion 转 Sanitized 后取长度，经 mock host 确定性执行）、D3
  `record_pipeline`（取→校验→落存储多 action 管线）。基础设施：runtime `run_hidden_case(s)_with_host`
  （预置宿主工厂注入）/ benchmark `NeutralTy` 增 `Text`/`OneOf`/`ErrorVariant` + `Level::L6` +
  `Problem.http_seed`（两 mode 共享 mock url→body，不进 prompt）/ baseline 契约扩充（`one of` 失败成员
  `{variant,fields}` dict 对称 + mock `http_get` 注入）。**D2 accept/reject 矩阵**：accept 半在 benchmark
  L6，reject 半落确定性测试 `cli/tests/intent_matrix.rs`（Sophia 静态拒绝不安全候选 CHECK-INTENT-001、
  接受经转换候选；TS 接受半以可复现片段文档化，不引入 tsc 门禁——与「baseline 只做 Python」一致）。
  防泄漏断言登记 L6 题集 token。**实现中修复 F2 潜伏缺陷**：`Http.Get` effect 引用 arity 1→0
  （`effects {}`/`allow {}` 声明位 0 参，此前被 HIR 误判 UnresolvedEffect，因语义测试绕过 HIR
  resolve_effect + D2 前无对 Http 程序完整 code_check 而潜伏）；强化 HIR 回归测试。三题手解验证均可
  表达、可执行（解释器跑通成功 + 失败两路）；其间确认两条语言事实（多参 input 须 `;` 分隔、variant
  字段绑定禁 shadow 可用空字段模式规避）。全工作区 **333 passed / 0 failed**，clippy（`-D warnings`）
  0 警告，fmt clean。真实 LLM 端到端实跑待 API key 环境（benchmark / e2e 不进 cargo test）。至此 v1
  完成判据 2 达成（D1/D2/D3 可端到端 + D2 真实 accept/reject 矩阵条目）；剩余 v1：工作流 A（WASM
  codegen，判据 1）+ 判据 3（strip-assist artifact 层）。
- 2026-05-31 — **文档归拢：标准库文档体系化**。将分散的标准库相关文档归拢为与「语言设计 / 语言实现」
  对称的两份框架文档 + 一份库契约文档：① **新建 `docs/stdlib_design.md`**（标准库设计：定位 / 功能库非
  协议栈 / 无 ambient authority / 基线 vs 库资产边界 / 两阶段提示词脚手架 / 范围准则 / 库清单）——**吸收**
  原 `stdlib_prompt_scaffolding.md`（S2 设计门）+ **集中** `language_design`·`dev_checklist`·`architecture`
  散落的标准库范围准则；② **新建 `docs/stdlib_implementation.md`**（标准库实现：prompt 资产布局与 API /
  两阶段全链路贯通 / host import 注入接缝 / 测试边界 / 新增库清单）——吸收 S2 实现部分 + `http_host.md`
  的通用 host 注入机制；③ **合并** `http_effect.md`（F2 语言契约）+ `http_host.md`（S1 真实 host）为
  **`docs/http_lib.md`**（标准库 `Http` 的统一契约文档）。删除 `stdlib_prompt_scaffolding.md` /
  `http_effect.md` / `http_host.md` 三个旧文档。本 checklist 的 F2/S1/S2 块收敛为「摘要 + 指向新文档的
  指针」（不再重复设计细节、消除 arity=1 等已修正的过时描述）。更新全仓引用（README / language_design /
  language_implementation / engineering_architecture / e2e_test_design / benchmark_design /
  integration_demos）。纯文档归拢，无功能代码改动；测试基线不变（333 passed）。
- 2026-05-31 — **标准库重定位决策（文档门 R0 完成）+ 规划 storage 移除 + File 库**。经讨论确立 (B)
  「I/O = 库」模型：文件 / 网络 / 数据库都是标准库（多数语言传统），不是语言原语；语言核心提供
  effect/capability/intent **机制**，库提供具体 I/O **能力族**；`Console`（`print`）作为输出原语保留为
  语言内置（例外）。据此：**移除**语义不清的 `storage` 顶层节点 + `DB.Read/Write` 内置 effect +
  `Persisted<T>` intent（`storage` 在 关系DB/KV/持久化/内存 间摇摆、无后端，是 v0 起步期的四不像）；
  **新增 `File` 库**（v1 内，优先级不低于 `Http`，`File.Read`/`File.Write` 与 `Http` 同构）；**`DB`** 重
  定位为未来候选库（需先澄清语义）；**D3** 演示题改用 `File` 库重做。本轮按"先修文档门、后重构代码"
  完成 **R0（全部截面文档改写）**：新建 `file_lib.md` 设计门、`stdlib_design` 改为「I/O = 库」+ 库清单、
  `engineering_notes` 立决策；`language_design`/`language_implementation`/`engineering_architecture`/
  `concepts`/`type_system`/`benchmark_design`/`e2e_test_design`/`integration_demos` 去 storage/DB/Persisted、
  改 Http/File 库表述（规范示例 CompleteTodo 改为不依赖 storage）。代码重构（R1 移除 / R2 File 库 / R3 D3
  重做）待后续。纯文档，无代码改动（测试基线仍 333，代码尚未动）。
- 2026-05-31 — **R1 + R2 代码重构落地（移除 storage/DB/Persisted + File 库）**。按文档门 R0 的设计，全
  链路移除语义不清的 `storage` 顶层节点 + `DB.Read/Write` 内置 effect + `Persisted<T>` intent：grammar
  （删 `storage_def` + parser.c ABI15 重生成）、AST/lower（`Item::Storage`/`Storage`/`lower_storage`/
  `IncludeKind::Storage`）、HIR（`NodeKind::Storage`/`resolve_storage`/特殊根 `storage`/closure Reads·Writes
  边 + `db_storage_of_effect` + `ExcludedDependency` 失效路径）、semantic（`StorageDecl`/`infer_storage_op`/
  contract storage 授权注释/union_check storage 分支）、runtime（`EffectHost::storage_get/save`/storage 桶/
  `try_storage_op`）、builtins（`DB.*` 出 `BUILTIN_EFFECT_OPS`、`Persisted` 出 `INTENT_WRAPPERS`）。新增
  **File 库**（R2）：`File.Read`/`File.Write` 进 `BUILTIN_EFFECT_OPS`（arity=0）+ 特殊根 `File`；semantic
  `infer_effect_op` 统一处理 File/Http（File.Read→`Raw<Text>`、File.Write→`Unit`，content 须 `Sanitized<Text>`，
  intent 边界复用既有严格相等）；runtime `EffectHost::file_read/file_write` + `InMemoryHost` 内存桶 +
  `seed_file`，`interp::try_effect_op` 统一 File/Http 委派；CLI `CliHost` 覆盖 `file_read/write` 为真实
  `std::fs`，CLI `run` 据入口 effect 含 `File.*`/`Http.Get` 判定注入；提示词资产 `assets/stdlib/file.md` +
  `stdlib_catalog` 增 file 行。下游：共享语法基线删 §storage（File/Http 知识归按需库资产）、规范示例
  `CompleteTodo`/`TodoDomain` 改为不依赖 storage（id 用裸 `Uuid`、capability allow Console.Write）、删 e2e
  G5 组、benchmark 移除 storage 版 record_pipeline（D3 待 R3 用 File 重做）、pipeline 集成测试改 Console 版、
  各 snapshot（hir asg index / semantic fingerprint / syntax CST / prompt baseline·catalog·design）重生成、
  新增 File 库测试（semantic 4 + runtime 3 + CLI host 3 + 资产 snapshot）。手解验证 File 库端到端可表达
  可执行（write→read 往返 + intent 转换）。全工作区 **336 passed / 0 failed**，clippy（`-D warnings`）0 警告，
  fmt clean。真实文件 / 网络 IO 不进 `cargo test`。剩余 R3（D3 用 File 重做）待办。
- 2026-05-31 — **R3 落地（D3 用 `File` 库重做 + e2e File 用例）**。标准库重定位收尾：benchmark L6 D3
  从历史 storage 版（`record_pipeline`，随 storage 移除已删）重做为 **`File` 库版** `archive_or_reject`
  ——取（`File.Read(source)`→`Raw<Text>`）→ 校验（`CheckAmount` 的 `one of { Int, Rejected }` match）→
  经 `intent_conversion` 转 `Sanitized<Text>` → 写出（`File.Write(dest, ...)`）→ 读回（`File.Read(dest)`）
  返回字符数；校验失败原样返回 `Rejected` 失败结局。基础设施：`Problem` 增 `file_seed`（path→content
  mock，两 mode 共享、不进 prompt）；sophia mode verify 注入同时 `seed_http` + `seed_file`；baseline
  runner 写 `file_seed.json` + 注入对称 mock `file_read` / `file_write`（内存桶，read-after-write）+
  prompt `file_clause`。补 **e2e G5-01**（`g5_file.rs`，笔记写入与回读：`File.Write`→`File.Read`→intent
  转换的自包含 write→read 往返，复用 storage 移除后空出的 G5 槽位）。防泄漏 token 登记 `Archive` /
  `ArchiveCap`（D3）+ `VaultCapability` / `StoreNote`（G5）。**手解验证**：D3 管线（clean check + 成功
  路径返回 Int 5 + reject 路径返回 `Rejected{amount}`）与 e2e G5 骨架（write→read 往返返回 Int 5）均经
  临时测试确认可表达、可执行后删除。关键语言事实复用 D2：`File.Read` 的 `Raw<Text>` 必须经
  `intent_conversion` 动作转为 `Sanitized<Text>` 才能写出 / 取长度（裸 `Text` 不能直接成 `Sanitized<Text>`，
  唯一跨 intent 合法处是 intent_conversion）。文档同步：`integration_demos`（§4.3 改 File 版 + 状态横幅 +
  实现表）、`benchmark_design`（L6 阶梯表 + §3.1 + 变更记录）、`e2e_test_design`（G5 改 File + 结构 +
  变更记录）、`file_lib`（§一 + 演示验收变更记录）。全工作区 **336 passed / 0 failed**，clippy
  （`-D warnings`）0 警告，fmt clean。真实文件 IO / 真实 LLM 不进 `cargo test`。至此 **R0–R3 标准库重定位
  全部完成** + **v1 完成判据 2（D1/D2/D3 端到端 + D2 accept/reject 矩阵）完整达成**；剩余 v1：工作流 A
  （WASM codegen，判据 1）+ 判据 3（strip-assist artifact 层）。
- 2026-05-31 — **工作流 A（WASM codegen）设计门起草**。B 全部完成（F1/F2/S1/S2 + 标准库重定位 R0–R3 +
  D1/D2/D3）后，按既定时序展开工作流 A。新建 `docs/wasm_codegen.md` 设计门草案：盘点解释器语义（值模型 /
  body 子语言 / effect 委派）作为 WASM 必须复刻的**唯一 oracle**；提出 ① 输入契约冻结（A1，消费
  `SemanticModel` / `ExecGraph` / AST body，**不引入新 body IR**，避免双真相源）；② 值 ABI（标签化堆值 +
  i32 句柄 + bump-only 内存，无 GC）；③ 函数 ABI（Outcome 包装值在返回通道冒泡，复刻 raise，不用 WASM
  异常扩展）；④ 纯值运行时 helper 生成进 module 自身（只有 I/O effect 走 host import）；⑤ effect ABI
  （host import + capability 按入口 effect 注入，失败诚实 Err 阻断）；⑥ 工具链（emit 用纯 Rust 编码库 +
  差测试用纯 Rust 解释执行器，重型部署工具链 wasmtime / 浏览器只作下游消费者、不进 cargo test）；
  ⑦ 差测试复用 benchmark/e2e 已被解释器跑通的参考解（解释器 vs WASM 逐 hidden case 等价）；⑧ strip-assist
  扩展到 artifact 字节级比对（判据 3）。列 W1–W5 实现阶段（对齐 A1–A5；A6 增量查询架构解耦推后）+ 7 个
  待确认决策点 + 拟新建 `tools/codegen` crate（确定性工具链层，依赖 core、禁 IO）。纯文档，无代码改动；
  测试基线不变（336 passed）。待讨论确认后按 W1→W5 推进。
- 2026-05-31 — **工作流 A 设计门定稿（七个决策点全部采纳）**。`docs/wasm_codegen.md` 七个决策点全部确认
  采纳（body 不引入新 IR / 值 ABI 标签化堆值 + i32 句柄 + bump 无 GC / raise 用 Outcome 包装冒泡 / 纯值
  helper 进 module、I/O 走 host import / 纯 Rust 编码库 + 解释执行器、重型 host 不进门禁 / 新 crate
  `tools/codegen` / 差测试复用 benchmark·e2e 参考解），状态从「草案」转「已定稿」。下一步进入 **W1**
  （A1：冻结 IR 输入契约 + `tools/codegen` crate 骨架）。纯文档。
- 2026-05-31 — **工作流 A · W1 落地（A1：冻结 IR 输入契约 + `tools/codegen` 骨架）**。按 `wasm_codegen.md`
  §九 W1 推进 codegen 第一阶段：新建 `tools/codegen` crate（tools 层，依赖 core 三 crate，**零 IO、不调
  LLM、不改 IR**）。`CodegenInput`（`contract.rs`）把 codegen 的三个**冻结输入**——`SemanticModel`（声明
  视图）/ `ExecGraph`（callable 粒度执行图，由 `ExecGraph::from_model` 构建，与解释器 `Interpreter::new`
  同源）/ 全程序 AST（+ 可经 `TypeChecker` 重算的 `TypeTable`）——捆为单一只读入口；`emit_module` W1 占位
  **诚实返回 `NotYetImplemented`**（不伪造空模块冒充产出）。`A1` 标志「codegen 消费 IR、绝不反向改 IR
  形状」（语言实现 §12.1）。工作区注册 member + 依赖。W1 契约测试守护图↔模型一致 + emit 诚实占位。全
  工作区 **338 passed / 0 failed**（336 + 2），clippy（`-D warnings`）0 警告，fmt clean。下一步 W2
  （最小 emit：值 ABI 标签化堆值 + i32 句柄 + bump 内存、函数 ABI Outcome 包装、标量 / 算术 / `if` /
  `match` / `let`-`set` / `return`-`raise` / 跨调用 body → WASM）。
- 2026-05-31 — **工作流 A · W2a 落地（A2 最小 emit 标量核心 + A3 差测试夹具）**。`tools/codegen` 接入
  `wasm-encoder` 0.243（emit `.wasm`，受 rust-version 1.80 约束）+ `wasmi` 0.40（dev-dep，差测试执行器）；
  两者均纯 Rust、确定性、进 cargo test（重型部署工具链 wasmtime / 浏览器不进树，符合 §七）。**值 ABI**
  （`abi.rs`）：标签化堆值 + i32 句柄 + bump-only 内存；**emit**（`emit.rs`）：module = prelude（值 ABI /
  函数 ABI helper，纯值操作生成进 module 自身、不外置）+ 每 callable 一 function（i32 句柄×N → i32
  Outcome 句柄）。覆盖 `Unit`/`Bool`/`Int`/`Null` 值、字面量 / `Ident` / `Not`·`Neg` / 二元全算子（Add
  按 `TypeTable` 静态分派 Int）/ `if`-`else` / `let`-`set` / `return` / 跨 callable 调用（Outcome 包装 +
  raise 冒泡 ABI，复刻解释器）。**诚实占位**：`match`/`repeat`/`raise`/`print`/Text/List/Entity/State/
  effect → `NotYetImplemented`（不伪造）。**差测试**（`tests/diff.rs`）：emit→`wasmi` 执行→与解释器
  oracle 逐 case 比对，5 题（算术 + 控制流 / 比较 / 跨调用 / 布尔 / 相等）全部等价。字节确定（段顺序 /
  名字字典序 / 布局固定，服务 W5 strip-assist artifact 比对）。全工作区 **344 passed / 0 failed**，clippy
  （`-D warnings`）0 警告，fmt clean。下一步 W2b（`match` / `repeat` / `raise`〔ErrorValue 布局〕 + Text /
  List / Entity / State 值 + `Field` / `Construct`），再 W4（effect host import）+ W5（artifact diff +
  `sophia build`）。
- 2026-05-31 — **工作流 A · W2b 落地（A2 错误代数 + `one of` 返回 + `match`）**。在 W2a 标量核心之上扩展
  `tools/codegen` emit：ErrorValue 值布局（具名记录，字段按 key 字典序）+ 常量字符串区（variant/字段名 intern
  进 data section）+ 新 prelude helper（`str_eq`/`rec_field`/`rec_name_eq`，纯值操作仍生成进 module 自身）。
  覆盖 `raise V{...}`（→ ErrorValue → Raised Outcome）、返回的 `one of` 失败成员 variant `Construct`、`match`
  （`Bool`/`Null`/标量 `Type`/`Variant` pattern，含记录名比较 + 字段按名绑定）；`Eq`/`Ne` 按 `TypeTable` 守
  标量操作数。诚实占位：`repeat`/Text/List/Entity/State 值/`Field`/entity `Construct`/嵌套 variant 字段 →
  `NotYetImplemented`。差测试新增 3 题（L4 raise / D1 `one of` 返回 / match 含 variant 绑定 + 跨调用），
  读回扩展为从线性内存解码 ErrorValue 记录；解释器 vs WASM 全等价。至此 benchmark L1–L4 + D1 形态均经差测试
  等价。全工作区 **347 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W2c（`repeat` /
  Text / List / Entity / State 值 + `Field` / entity `Construct` + entity·state match pattern），再 W4
  （effect host import）+ W5（artifact diff + `sophia build`）。
- 2026-05-31 — **工作流 A · W2c 落地（A2 聚合值：Entity + State）**。在 W2a/W2b 之上扩展 `tools/codegen`
  emit 覆盖结构化建模：State 值布局（`[tag][state_ptr][state_len][value_ptr][value_len]`）+ 常量串区扩充
  （entity/state 名 + 字段/值名）+ 新 prelude helper（`make_state`/`state_name_eq`/`state_value_eq`；
  `emit_variant_value` 泛化为 `emit_record_value`，ErrorValue/Entity 共用记录布局）。emit 扩展 entity
  `Construct` / `Field`（`StateName.Value` → State 值 / `entity.field` → 取字段）/ `match` 增 entity·state
  `Type` pattern + `State` 值 pattern。诚实占位：`repeat`/Text/List/`Text.length`/标准库 I/O/嵌套记录构造字段
  → `NotYetImplemented`。差测试新增 4 题（rectangle_area / entity 返回 / traffic_next / checkout_limit L5），
  夹具扩展 `Arg` 枚举（Int/State）+ 从内存读回 State/Entity。**至此 benchmark L1–L5 + D1 全部形态经差测试
  与解释器等价**。全工作区 **351 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W2d
  （`repeat` + Text/List 值 + `Text.length`/`to_text`），再 W4（effect host import）+ W5（artifact diff +
  `sophia build`）。
- 2026-05-31 — **工作流 A · W2d 落地（A2 Text 值 + `repeat`）**。在 W2a–W2c 之上扩展 `tools/codegen`
  emit：Text 值布局（`[tag][bytes_ptr][byte_len]`，bytes 指向常量串区或 bump 堆）+ 字符串字面量入常量区
  + 新 prelude helper（`make_text`/`text_length`〔UTF-8 Unicode 标量计数，与解释器 `chars().count()` 一致〕/
  `text_concat`/`get_text_*`；`value_eq` 增 Text 字节比较）。emit 扩展 `Str` 字面量 / `Add` 按 `TypeTable`
  静态分派（Int 加 / Text 拼接）/ `Text.length` 伪字段 / match Text Type pattern / `Eq`·`Ne` 放行 Text /
  **`repeat`**（倒计数循环，body 内 return/raise 经 WASM return 提前退出）。诚实占位：`print`/`to_text`/
  `List`/标准库 I/O → `NotYetImplemented`。差测试新增 4 题（Text 拼接 + `.length` 含多字节 Unicode / Text
  返回 + 相等 / repeat 累加 / repeat 早退）；夹具 `Arg` 增 `Text` + 从内存读回 Text。**至此全部 8 类值 +
  全部纯逻辑语句/表达式形态经差测试与解释器等价**（D2/D3 的 effect 部分待 W4）。全工作区 **355 passed /
  0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W4（effect host import），再 W5（artifact
  diff + `sophia build`）。`to_text` / `List` 无 v1 演示需求触发，按需再补（YAGNI）。
- 2026-05-31 — **工作流 A · W4 落地（A4 effect host import：Console/File/Http）+ A2 关闭**。emit 把副作用
  映射为 WASM host import（5 个 `sophia_host` import：console_write/file_write/file_read/http_get/read_copy，
  字节级 ABI——host 只收发字节、不识值布局）。函数索引前移 `IMPORT_COUNT=5`（`helper` 常量统一偏移，call
  站零改动）；所有 module 统一声明 import、真实 vs mock host 由实例化方提供（capability 边界编译期已兑现）。
  emit 扩展 `print` / `File.Write` / `File.Read` / `Http.Get`（File.Read/Http.Get 经 import 返回长度 →
  `alloc` + `read_copy` + `make_text`，结果运行时即 Text 值）。host 失败 trap（解释器为硬错误阻断，绝不
  伪造）。差测试新增 3 题（G2 Console / D2 Http+intent_conversion / D3 File 往返+intent），夹具加纯 Rust
  mock host（`Store<HostState>` + `Linker::func_wrap`，seed_file/seed_http 同 `InMemoryHost` 语义、未命中
  trap），解释器 oracle 改 `run_action_with_host` 注入同一份 seed。**至此 benchmark L1–L6（D1/D2/D3）+
  G2/G5 全部形态经差测试与解释器等价**——A2 emit 关闭、A4 关闭、A3 差测试覆盖完整。全工作区 **358 passed /
  0 failed**，clippy（`-D warnings`）0 警告，fmt clean。下一步 W5（strip-assist artifact 字节比对 +
  `sophia build` emit + smoke 串通），完成判据 3 + `sophia build` 落地。`to_text`/`List` 无 v1 演示需求触发，
  按需再补（YAGNI）。
- 2026-05-31 — **工作流 A · W5 落地（A5 strip-assist artifact 层 + `sophia build` emit）**。① **artifact
  层门禁**：`sophia-codegen` 新增 `emit_from_sources(sources, strip)` + `check_artifact_strip_equivalence`
  ——移除全部 Semantic Assist 字段前后 emit 的 `.wasm` 必须**逐字节相等**（判据 3，`language_design.md`
  §5.1）；前提是 emit 确定性（值布局字典序 / 常量池稳定序 / 段顺序固定，W2 已保证）。② **`sophia build`**
  从 v0 空操作改为：check（含 IR 层 strip-assist）→ artifact 层门禁 → emit `sophia-runs/build/program.wasm`；
  codegen 未覆盖构造（`to_text`/`List`）诚实报 `NotYetImplemented`（不伪造产出，解释执行仍可用）。
  `sophia.toml` `[build] target` 改 `wasm`；`smoke` build 步随之 emit；`engineering_architecture` §9.1
  命令表更新。codegen 测试加 artifact 门禁 + 确定性 2 题，CLI pipeline 加 build emit + 未覆盖诚实报告 2 题。
  `tools/codegen` 加 `sophia-hir` 依赖（emit_from_sources 需 resolve）、CLI 加 `sophia-codegen` 依赖。全工作区
  **362 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt clean。**至此工作流 A 的 W1–W5 全部落地**：
  A1 契约冻结 / A2 emit / A3 差测试 / A4 effect host import / A5 artifact 门禁 + build——**v1 完成判据 1
  （WASM 与解释器逐 case 等价，起步子集全覆盖）+ 判据 3（strip-assist artifact 层）达成**。剩余 A6（增量
  查询架构，Salsa 思想，与 codegen 解耦）待独立设计门；真实部署 host（wasmtime / 浏览器）随实际部署需求接入。
- 2026-05-31 — **CI 流水线接入 + 修正失真的 MSRV**。`.github/workflows/ci.yml`（此前已有 fmt/clippy/test/
  release build 雏形）增强为两 job：① **主门禁**（stable）fmt + clippy + test + release build，全部 `--locked`
  可复现——`cargo test --workspace` 已含工作流 A 的 A3 差测试（`tools/codegen/tests/diff.rs` 逐 hidden case
  比对解释器 oracle vs WASM）+ A5 artifact 门禁，故差测试确定性部分**自动纳入** CI；② **MSRV 守护 job**：从
  `Cargo.toml` 读 `rust-version` 装该 toolchain build + test，防 MSRV 声明静默腐烂。**接入时发现并修正失真
  MSRV**：声明值 `rust-version = "1.80"` 已**失真**——逐版验证（1.80/1.85/1.86/1.87/1.88/1.90/1.92/1.93/1.94
  均失败、1.95 通过）确认真实下限是 **1.95**（transitive `rusqlite → libsqlite3-sys 0.38` 用 `cfg_select!`
  〔1.95 稳定〕、`reqwest → icu 2.2` 需 1.86、`sha2 0.11 → block-buffer 0.12` 需 edition2024）；据实校正为
  `1.95` 并在 Cargo.toml 注释说明「项目跟随最新稳定版、此为已验证最低」。W1 当时「为兼容 1.80 而压低
  wasm-encoder 0.243 / wasmi 0.40」的理由本就建立在失真前提上（实际 MSRV 早已远高于 0.251/1.0 所需的
  1.85/1.86）——但保持现版本可用、不为此返工（YAGNI；升级 wasm-encoder/wasmi 无收益）。1.95 上 362 passed /
  0 failed 已验证。真实 LLM 的 e2e / benchmark 仍是 example、不进 CI（无 key 干净跳过）。
- 2026-05-31 — **A3 状态核定：`[~]` → `[x]`（确定性核心完成）+ 显式剥离可选增强**。复核 A3 标记：其
  确定性核心——「同 `.sophia` 经解释器 + WASM 逐 hidden case 等价、解释器为 oracle、接入 CI 确定性部分」
  ——已全部落地（`tools/codegen/tests/diff.rs` 19 等价测试覆盖全部 8 类值 + 全语句/表达式形态 + 全 effect，
  在 `cargo test --workspace` 内、CI 接入后已门禁化），且原 `[~]` 与该 bullet 自身「已接入 CI 门禁」措辞
  自相矛盾，故据实改 `[x]`。同时**诚实剥离**一项未做的**可选增强**（移入「从 v0 结转」开放项）：设计文本
  曾提「复用 benchmark/e2e 参考解」，但那些是 LLM 运行时生成、无静态 `.sophia` 源，无法字面复用；真正形态
  是把 WASM 差测试接进真实 LLM 候选循环（仅有 key 时跑、不进确定性 CI）——价值真实但属增量，非 A3 确定性
  门禁范围。改 `[x]` 不掩盖此残留：bullet 内 + 开放项各记一处。无代码改动。
- 2026-05-31 — **测试三类化重构（去 mock 的 e2e/benchmark + 文档整理为三篇 test guide）**。确立测试
  只有三类——**单元**（进 `cargo test` 门禁、确定性、唯一可 mock〔隔离不完整代码的不得已手段〕）/
  **e2e**（真实 LLM + 真实 IO、禁 mock）/ **benchmark**（与 Python 比成功率·耗时、禁 mock），**不允许
  第四类**（新增类型只会混淆重叠边界）。原「集成演示 D1/D2/D3」不是第四类，按能力维度并入 e2e；mock 会
  掩盖错误，AI 尤须避免「为通过测试走捷径」（此前为让题确定而用 mock 即此类捷径）。**代码**：① e2e 用
  真实 IO——`cli/src` 拆 bin→lib+bin（新建 `lib.rs` 导出协调层构件供 example 复用 `CliHost`），harness
  `execute_and_check` 据入口 `Http.Get`/`File.*` effect 注入真实 `CliHost`（`reqwest`/`std::fs`）；新增
  **G2-03**（`FetchNonEmpty` 真打 `example.com`，断言可信文本非空→Bool true）、**G4-03**（`ClampOrReject`
  可失败返回 `one of`，期望返回 `OutOfRange{value:15}`）；**G5-01** 改打真实临时文件（去内存桶 mock）。
  ② benchmark 去 mock——删 D2 `fetch_length`（http mock）+ D3 `archive_or_reject`（file mock），保留纯
  逻辑 `clamp_or_reject`；删 `Problem.http_seed`/`file_seed` 字段 + baseline runner 的 `http_get`/
  `file_read`/`file_write` 注入 + sophia_mode host 注入分支；删 `NeutralTy::Text`（YAGNI，无题用）。
  ③ 删 `runtime::{run_hidden_case_with_host, run_hidden_cases_with_host}`（去 mock 后只剩自身单测用）+
  re-export + 3 个 host 变体测试。**文档**：删 `integration_demos.md` / `e2e_test_design.md` /
  `benchmark_design.md`，整理为一致格式的三篇 test guide——`unit_test.md` / `e2e_test.md` /
  `benchmark_test.md`（精简设计篇幅〔决策已完成〕、增加每用例说明、偏 test guide），D1/D2/D3 设计理由
  折进 e2e/benchmark 指南。更新全仓引用（README 文档清单 + 入口、`concepts`/`http_lib`/`file_lib`/
  `workflow_graph_spec` 截面、Cargo.toml、各 example 代码注释、`intent_matrix.rs`）；防泄漏 token 增
  `IngestCapability`/`FetchNonEmpty`、删已移除题 token（`FetchLength`/`Archive`/`Rejected` 等）。全工作区
  **359 passed / 0 failed**（362 − 3 个删除的 host 变体测试），clippy（`-D warnings`）0 警告，fmt clean。
  真实 LLM / 真实 IO 不进 `cargo test`（e2e/benchmark example，无 key 干净跳过）。
- 2026-05-31 — **库插件模型重构（P1：标准库样板化 + 清单驱动 + 路线 B host + 标准库 crate）**。把「库」
  从散落 6 个 crate 9 处硬编码切片，重构为**清单 = 单一真相源 + `LibraryRegistry` = 各层只读数据源**
  （倒转索引方向）。设计门 `library_plugin.md` 经讨论全部确认采纳后**消化并入** `stdlib_design.md`（设计）
  + `stdlib_implementation.md`（实现），原文档安全删除。**新增两 crate**：`sophia-library`（core 层契约
  类型：`LibraryRegistry`/`OpContract`/`TypeDesc` + 清单解析，无 `Value`、零 IO）+ `sophia-stdlib`（内容
  层：`libs/http/`、`libs/file/` 清单 + 资产 + native/mock host）。**核心改动**：① `File`/`Http` 从
  `hir::builtins::BUILTIN_EFFECT_OPS` 迁出（仅留 `Console`），改由 `AsgIndex::with_libraries(registry)`
  注入；② `resolve` 特殊根放行用 `index.is_library_family`（去 `File`/`Http` 字面量）；③ `type_layer::
  infer_effect_op` 表驱动化（`index.library_op` 的 TypeDesc → Ty，替代命令式 match）；④ host 改**路线 B**
  ——`runtime::HostRegistry`（`(family,op) → Box<dyn HostFn>`）替代固定方法集 `EffectHost` trait + 删
  `InMemoryHost`，native（reqwest/std::fs）/ mock / 三方 WASM host 同构为 `Box<dyn HostFn>`；⑤ CLI `CliHost`
  删除、改 `register_native_hosts`；⑥ prompt crate 去 stdlib 内容（库目录 / 资产改由 `registry.catalog()`
  `registry.preamble(libs)` 提供，`implement_system_prompt(stdlib_block)` 收预渲染块）；⑦ codegen
  `CodegenInput` 持 `lib_index` 供 emit 表驱动；⑧ `check_program` / CLI / LSP / codegen 入口用
  `standard_registry()`。**两条正交维度**（surface = Sophia 源码 / effect-op × host = none / native / WASM）
  使纯 Sophia 库与 WASM 库在解释 / VM 两模式对称可用（保 oracle 不变量）。**去渗透判定达成**：`core/hir`/
  `core/semantic`/`runtime` 不再出现 `File`/`Http` 字面量（`Console` 唯一例外）。core 单测用内联清单夹具、
  不依赖 stdlib；runtime 分派测试用中性 `Vault` 库。纯重构、**零行为变化**（标准库 File/Http 端到端语义
  不变）。**新增库零改语言核心**。全工作区 **366 passed / 0 failed**，clippy（`-D warnings`）0 警告，fmt
  clean，`--locked` 一致。P2（三方动态发现 + 解释器内嵌 wasmi 执行三方 WASM host）待真实三方需求触发。
- 2026-05-31 — **库插件 P2 落地（三方动态发现 + 两个演示库）**。设计门 `library_plugin_p2.md` 经讨论
  全部确认后消化并入 `stdlib_design` / `stdlib_implementation`，原文删除。**三方发现**:`sophia-stdlib`
  新增 `discover`（`full_registry` / `full_registry_from` / `third_party_roots` / `DiscoverError`——扫约定
  根 `./sophia_libs/` + `$SOPHIA_LIB_PATH` → 读清单 + 资产 + `.sophia` 源码 + `host.wasm` → 合并注册表,
  确定性排序,失败启动报错）;`hir::LibrarySources`（库 Sophia 源码解析 owned AST,并入 index/model/执行）+
  `HirError::LibrarySourceParse`。**跨 domain 豁免**（唯一触及语言核心）:`AsgIndex.library_domains` +
  `is_library_domain` + `resolve` 对「用户 → 库 domain」放行 `ImplicitCrossDomain`（用户↔用户仍受检）。
  **WASM host**:`wasmi` 提为 `sophia-runtime` 正式依赖 + `runtime::WasmHostFn`（持 `host.wasm` 实例,统一
  字节 ABI,标量 i64 直传;`wasm-encoder` dev-dep 测试时生成 host.wasm）。**两演示库**:`hash_sophia`（纯
  Sophia 源码库,action `SophiaDigest`）/ `hash_wasm`（WASM-effect 库,op `WasmHash.Mix`,`effectful=false`）
  计算同一确定 digest。集成测试 `stdlib/tests/library_demo.rs` 验收:发现 + 注册表合并 + 跨 domain 豁免 +
  纯 Sophia 库执行 + WASM 库经 WasmHostFn 执行 + 两库逐位相等,全确定进门禁。**否决 sqlite 作首例**（沙箱
  无 IO、`list of record` 超 TypeDesc、撞 `DB` 槽位、淹没机制验证）。全工作区 **369 passed / 0 failed**,
  clippy（`-D warnings`）0 警告,fmt clean。**CLI 生产接线**（`library_registry()` → `full_registry()` +
  库源码并入命令 inputs + `sophia run` 注册三方 WASM host）列为后续项——机制 + 确定性 demo 已就位。
- 2026-05-31 — **CLI 生产路径接线落地（库插件 P2 收尾）**。三方库发现机制接入 CLI 生产路径:① `discover`
  新增 `project_roots(root)` / `full_registry_for(root)`（以**项目根**而非进程 CWD 解析 `<root>/sophia_libs/`,
  CLI 命令以 `--root` 定位项目）;`commands::library_registry(root)` 由 `standard_registry` 改 `full_registry_for`
  （返回 `Result`,发现失败诚实报错退出）。② 各命令（`check`/`run`/`index`/`graph`/`context`/`repair-context`）
  经新增 `library_context(root)` 把库随附 Sophia 源码（`LibrarySources`）并入 program inputs + asts——纯
  Sophia 库节点（如 `SophiaDigest`）须建模才可解析/执行;owned AST 在命令函数作用域持有,活到 resolve+analyze+run
  全程。③ `native_host` 新增 `register_wasm_library_hosts(host, registry)`(遍历注册表 `host.wasm` 库注册
  `WasmHostFn`,ABI 子集 `(Int,Int)->Int` 校验 + 装载失败诚实 `Err`);`sophia run` 的 `run_with_host` 统一
  注册三方 WASM host（无条件,据 registry.host_wasm）+ 标准库 native host（按入口 effect 按需）,替换原
  `run_with_default_host`/`run_with_real_host`。④ `tools/check::check_strip_assist_equivalence` 改 registry-aware
  （`(sources, registry, index)`,两侧对称并入库源码 + 同一 registry,否则用户引用库节点会让 strip 前后名称
  解析不对称误判）;`check_program` 并入库源码。⑤ graph gate（hidden-case 模型构建）/ design / implement-loop
  与 LSP / codegen 仍用 `standard_registry`（确定性子门禁,三方发现是启动行为不进门禁）。**手动 smoke**:项目带
  `./sophia_libs/{hash_sophia,hash_wasm}`,`check` 通过、`run ViaSophia`/`run ViaWasm` 均得同一 digest 210523。
  全工作区 **372 passed / 0 failed**（+3 WASM host 注册单测:标准库 no-op、ABI 子集外拒绝、非法 wasm 字节拒绝）,
  clippy（`-D warnings`）0 警告,fmt clean。库文档（`stdlib_design` §五.1/§五.3/变更记录、`stdlib_implementation`
  §2.3/§三/变更记录）措辞由「后续项」改「已落地」。
