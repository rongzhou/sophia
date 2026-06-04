# Sophia 端到端测试指南（e2e test）

> Sophia 三类测试的第二类。e2e 测试验证**完整的 v0 工作流闭环**在**真实 LLM + 真实 IO**下端到端
> 可用。它是 `example`（**不进** `cargo test` 门禁、无 API key 时干净跳过），按需手动 / 定时运行。
> 这是一份 test guide：讲清楚 e2e **测什么、怎么跑、按什么纪律组织、有哪些用例**。

---

## 一、定位

### 1.1 测什么

e2e 验证下面这条闭环在真实 LLM 下端到端跑通：

```
人类目标(Objective + 验收条件)
   → design_solution(真实 LLM)        → 语义伪代码
   → implement_design(真实 LLM)        → 候选 .sophia
   → code_check(真实 tools/check)      → 诊断
   → [repair(真实 LLM) ⟲ 预算内]       → 修好的候选
   → v0 解释器(sophia_runtime) 执行    → 对照期望结果
```

每个用例有明确的**可执行成功判据**（check 通过 + 解释器返回值 / raise / console 匹配期望），
但允许跨运行的 LLM 表述差异（不做逐字断言）。

### 1.2 不测什么

- 不测 LLM 的「智力上限」：用例规模保持在 v0 起步子集能表达、能解释执行的范围内。
- 不替代单元测试：检查器 / 解释器 / 图不变量的正确性由各 crate 的单元测试保证（见
  `docs/unit_test.md`）；e2e 只验证「把它们串起来 + 真实 LLM + 真实 IO」这条链路通。
- 不与 Python 比成功率 / 耗时——那是 benchmark（见 `docs/benchmark_test.md`）。

### 1.3 mock 政策：禁 mock，一律真实 IO

**e2e 不允许 mock。** 它的目的是验证真实行为，mock 会**掩盖错误**。具体：

- **真实 LLM**：经 OpenAI 兼容端点真实调用（无 key 时干净跳过，不伪造响应）。
- **真实网络**：需要 `Http.Get` 的用例打**稳定公开站点**（如 `example.com`），经真实 native host
  （`reqwest`）执行，不 mock。
- **真实文件**：需要 `File.Read` / `File.Write` 的用例读写**真实临时文件**；harness 把 native 文件
  host 的 sandbox 根设为 OS 临时目录，程序传相对路径，经真实 `std::fs` 执行，不用内存桶 mock。

> harness 据入口 action 声明的 effect 自动注入真实 host：入口声明 `Http.Get` / `File.Read` /
> `File.Write` 时用真实 native host，否则（纯逻辑 / `Console.Write`）用空 `HostRegistry`。真实 host
> 失败即 `Err` 阻断、如实判负，绝不伪造成功。
>
> harness 的 LLM 驱动运行在 Tokio async 外壳中；最终 v0 解释器与真实 File/Http host 仍保持同步
> 契约。需要真实 IO 的执行判定会放入 Tokio blocking 线程完整运行并析构，避免 `reqwest::blocking`
> 在 async runtime 上下文中释放其内部 runtime。

### 1.4 选题哲学：覆盖面（与 benchmark 的表现力阶梯不同）

e2e 选题重**覆盖面**（coverage）——按**正交能力维度**分组（语法 / effect / 启发式 / 错误代数 /
File / 目标树），每组钉住一类能力作正确性 / 回归 gate。这与 benchmark 的**表现力阶梯**选题
（按单调递增难度暴露与 Python 的分叉点，见 `benchmark_test.md`）不同。两者唯一共享的是底座：
同一份可泛化、防作弊的提示词 + 脚手架（`sophia_syntax_baseline` + 防泄漏纪律）；题目刻意不重叠。

---

## 二、运行

```bash
export SOPHIA_LLM_API_KEY=<key>          # OpenAI 兼容模式需要；不落盘 / 不进图 / 不打印
cargo run -p sophia-cli --example e2e -- --list         # 列出全部用例 ID（不需 key）
cargo run -p sophia-cli --example e2e -- --case G1-02   # 跑单个用例（首选）
cargo run -p sophia-cli --example e2e -- --group g2     # 跑某组（一个进程内顺序跑）
cargo run -p sophia-cli --example e2e -- --llm-mode ollama --case G1-02
```

批量执行器 `scripts/run_e2e.sh` 逐用例**各起一进程**串行跑，输出落盘 `sophia-runs/e2e-logs/`：

```bash
scripts/run_e2e.sh                       # 串行跑全部
scripts/run_e2e.sh g1                    # 只跑某组
scripts/run_e2e.sh --cases G1-01 G2-02   # 只跑指定用例
```

环境变量：
- `SOPHIA_LLM_MODE`（`openai` / `ollama`，默认 `openai`）；
- `SOPHIA_LLM_MODEL`（OpenAI 默认 `deepseek-ai/deepseek-v4-flash`；Ollama 默认 `qwen3.6:latest`）；
- `SOPHIA_LLM_BASE_URL`（OpenAI 默认 NVIDIA OpenAI 兼容端点；Ollama 默认 `http://localhost:11434`）；
- `SOPHIA_LLM_TIMEOUT_SECS`（响应读取空闲超时；OpenAI 默认 120，Ollama 默认 300）；
- `SOPHIA_LLM_MAX_REPAIRS`（默认 0 = 要求一次过；R 类用例单独设正预算）；
- `SOPHIA_E2E_LOG_DIR`（批量脚本日志目录，默认 `sophia-runs/e2e-logs`）。

也可用参数覆盖：`--llm-mode` / `--llm-model` / `--llm-base-url` / `--llm-api-key` /
`--llm-timeout-secs`。OpenAI 兼容
模式未设 `SOPHIA_LLM_API_KEY` 时 example **干净跳过并以成功退出码返回**（CI 安全）；Ollama 模式
无需 API key。批量脚本在 OpenAI 模式缺 key 时报错（它的前提就是要真跑），Ollama 模式不检查 key。
OpenAI 兼容与 Ollama 都走 streaming；超时语义是“连接 / 响应流长时间无进展”，不是限制整段生成
总耗时。OpenAI 兼容远端默认有界重试；Ollama 默认不重试，避免本地生成超时后重复请求。

---

## 三、纪律

### 3.1 防答案泄漏（第一原则）

**可以给 LLM**：任务需求（目标 / 描述 / 验收条件，含任务自己的领域词汇——这是题目不是答案）、
可泛化的语言事实（共享语法基线 `sophia_syntax_baseline`，只含标准语法 + 中立示例）、真实诊断
（`tools/check` 的「哪里错」，不给「应改成什么」）、命名保真规则（题面显式给出的名字逐字照用）。

**绝不能给 LLM**：目标程序的源码 / 片段、针对具体任务的实现提示、任何让用例退化为照抄的内容。

**结构化防线**（不靠自觉）：① 语法基线是单一共享资产，由 snapshot + 防泄漏断言测试守护（断言
基线不含任何任务 token，见 `workflow/prompt/tests/render.rs`）——新增用例引入新领域词汇须在该
断言里登记；② 用例的「期望结果」与「待修坏候选」只存在于 harness 内部，不喂 LLM；③ design 阶段
**不注入**语法基线（伪代码 semantics > format），基线只进 implement / repair。

### 3.2 标准库资产两阶段按需注入

**design** 阶段注入库目录（`stdlib_catalog`，每库一行用途），LLM 在伪代码 `libraries` 字段
**自选**要用的库；**implement / repair** 据所选库经 `stdlib_preamble(libraries)` 注入对应完整库
介绍（如 `["http"]` → `assets/stdlib/http.md`）。库选择是 LLM 的 design 决策，**不在用例元数据
预声明**（预声明会泄漏解法）。库资产同基线的 snapshot + 防泄漏纪律。详见 `docs/stdlib_design.md`。

### 3.3 中间产物全程在内存

harness 用 `GraphStore::open_in_memory`、候选 `.sophia` 正文是内存 `Vec<(路径, 正文)>`、全程不写
文件系统；唯一可见形式是 stdout（经 `run_e2e.sh` 落盘为文本日志）。这让每个用例自包含、无残留。
（对比：CLI `graph` 路径才把 `.pseudo` / 候选落盘到 `sophia-runs/graph/artifacts/`。）

> 注意：§3.3 指的是**工作流中间产物**（图 / 伪代码 / 候选源码）在内存；而需要真实 IO 的用例
> （G2-03 网络、G5-01 文件）的**执行**仍打真实站点 / 真实临时文件（§1.3），二者不矛盾。

---

## 四、用例清单

用例按能力维度分组。每条给出：场景、考察点、入口 + 实参、成功判据。**只描述题目与判据，不含
任何 Sophia 源码答案。**

### G1 基本语法 / 纯逻辑（4/4 一次过）

| ID | 场景 | 考察点 | 入口 / 实参 | 成功判据 |
| --- | --- | --- | --- | --- |
| G1-01 | 整数计数器加一 | 单 action、Int、算术、纯函数 | `IncrementCounter(41)` | 返回 `42` |
| G1-02 | 待办状态置为完成 | state（多 value）+ 状态值返回 | `CompleteTodo(TodoStatus.Pending)` | 返回 `TodoStatus.Done` |
| G1-03 | 购物车单项金额合计 | entity（多字段）、字段访问、整数乘法 | `LineTotal(CartItem{unit_price=7,quantity=6})` | 返回 `42` |
| G1-04 | 免邮资格判定 | Bool 逻辑、比较（`>=`） | `QualifiesForFreeShipping(150)` | 返回 `true` |

G1 全部「一次过、零修复」（`max_repairs=0`），考察良好脚手架下模型能否直接产出可用代码。

### G2 effect + capability（含真实网络）

| ID | 场景 | 考察点 | 入口 / 实参 | 成功判据 |
| --- | --- | --- | --- | --- |
| G2-01 | 审计日志写入 | `Console.Write` effect + capability 绑定、intent 边界（`Sanitized<Text>`）、`.length` | `LogNotice(Sanitized "hello")` | 返回 `5` 且 console = `["hello"]` |
| G2-02 | 双行通知广播 | 多次 `Console.Write` 顺序执行、effect 只声明一次 | `Broadcast()` | 返回 `2` 且 console = `["hello","bye"]` |
| G2-03 | 网络获取 + intent 安全 | `Http.Get` effect + capability、intent 边界（`Raw<Text>` 经 `intent_conversion` 转 `Sanitized<Text>`）、**真实网络** | `FetchNonEmpty("https://example.com")` | 返回 `true`（取回可信文本非空） |

- **G2-01 / G2-02** 同时校验返回值与 console 输出（验证 effect 真经解释器 effect host 执行）。
  `Console.Write` 只接受字面量 / `Sanitized<T>` / `Redacted<T>`（intent 边界），故「打印输入文本」
  类用例的输入建模为 `Sanitized<Text>`——这是需求约束而非实现提示。
- **G2-03** 是 LLM-native 旗舰演示：`Http.Get` 取回的 `Raw<Text>` **不可信**，必须经显式
  `intent_conversion` 转 `Sanitized<Text>` 才能用，否则静态拒绝。harness 注入真实 native host
  （`reqwest`）真打 `example.com`（IANA 维护的稳定示例域）。断言取**稳定属性**「取回的可信文本
  非空 → 返回 Bool true」而非精确长度（真实响应体长度不稳定），避免脆弱断言。**reject 半**（不
  转换直接用 → 静态拒绝）由确定性单元测试 `cli/tests/intent_matrix.rs` 钉死（见 `unit_test.md`）。

### G3 启发式节点处理

| ID | 场景 | 考察点 | 入口 / 实参 | 成功判据 |
| --- | --- | --- | --- | --- |
| G3-01 | 库存扣减 | 经调度器 `run_goal_loop`：LLM 自主决策 decision→design→implement 推进到候选 | `DeductStock(50, 8)` | 返回 `42` |

G3 用 `CaseKind::Scheduler`：harness **不**硬编码 design→implement 顺序，把动作选择权交给 LLM。
每轮 decision 的 prompt 据当前 active context + 进度（`GoalProgress`）在调用时刻渲染——LLM 据演进
状态自主多步推进。

### G4 复杂程序（能力组合，非难度）

| ID | 场景 | 考察点 | 入口 / 实参 | 成功判据 |
| --- | --- | --- | --- | --- |
| G4-01 | 订单总价（跨 action 调用） | 一个 action 调用另一个（经 Execution Graph 调用边） | `OrderTotal(7, 5, 7)` | 返回 `42` |
| G4-02 | 提现校验（error algebra） | error 声明 + `errors` + `raise`：非法输入 raise 领域错误（不可恢复中断） | `Withdraw(30, 50)` | raise `InsufficientFunds` |
| G4-03 | 受限取值（可失败返回 `one of`） | `one of { Int, OutOfRange }`：失败是**返回值**（可恢复结局），调用方须 match 两路 | `ClampOrReject(15, 10)` | 返回 `OutOfRange{value:15}`（非 raise） |

- **G4-01** 验证跨 action 调用经 Execution Graph 调用边路由。
- **G4-02 / G4-03** 是错误处理的**两种范式对照**：G4-02 用 `raise`（不可恢复、中断），成功判据
  `Expect::Raises` 校验 raise 出的 variant；G4-03 用 `one of` 返回失败成员（可恢复、失败是值），
  成功判据 `Expect::Returns(ErrorValue{...})` 校验返回的失败结局。G4-03 是 F1（`one of` 可失败
  返回）相对 v0 的核心能力增量。

### G5 标准库 `File`（真实文件读写）

| ID | 场景 | 考察点 | 入口 / 实参 | 成功判据 |
| --- | --- | --- | --- | --- |
| G5-01 | 笔记写入与回读 | `File.Write` + `File.Read` effect + capability、intent 边界（`File.Read` 的 `Raw<Text>` 经 `intent_conversion` 转 `Sanitized<Text>`；`File.Write` 只收 `Sanitized<Text>`）、**真实临时文件** | `StoreNote(<临时路径>, Sanitized "hello")` | 返回 `5`（写入后读回长度） |

G5-01 用例**自包含**（`File.Write(path, message)` → `File.Read(path)` → intent 转换 → 返回长度的
write→read 往返）。harness 注入真实 native host，文件 sandbox 根为 `std::env::temp_dir()`，path 是
进程内固定相对名，打到**真实临时文件**（非内存桶 mock）。考察 `File` 库的本地文件读写 + intent 边界（与 G2-03 的
网络 intent 链路同构，但叠加**写出边界**：`File.Write` 只收 `Sanitized<Text>`）。`File` 库语法 /
intent 边界由按需库资产 `assets/stdlib/file.md` 承载，不进常驻基线。见 `docs/file_lib.md`。

### G6 目标树遍历（decompose）

| ID | 场景 | 考察点 | 入口 / 实参 | 成功判据 |
| --- | --- | --- | --- | --- |
| G6-01 | 温控面板的两个独立读数换算 | LLM 自主 `decompose` 拆根目标为两个具名 action 子目标 + 人类授权检查点 + binding 继承 + 子目标各自推进 | `CelsiusToScaled(21)` | 返回 `42` |

G6 经目标树遍历层 `run_goal_tree` 驱动（`CaseKind::Tree`）。与 G3（单目标 spine）不同点在**非线性
树推进 + 人类授权检查点**：

- **人类授权检查点**：decompose 落图产出 `Decomposition` + 子 `Objective`（LLM provenance、默认
  未绑定）后，遍历层回调注入的审查者。harness 用 `AutoAcceptReviewer`（代表人类操作员）裁决接受
  → 引擎建**真实** human `AcceptanceEvent`（非绕过 binding 谓词）；子目标随即沿 `member_of` 继承
  binding，进入各自的 active context。引擎**不伪造**人类授权——拒绝路径不递归、不伪造 withdrawal。
- **focus-aware prompts**：harness 的 prompt 提供者按 `focus` id 从 active context 取目标题面，
  使子目标的 design / implement 看到的是**自己**的子目标（而非根目标）。根目标使用用例级
  acceptance；拆出的子目标不继续注入根 acceptance，避免每个子目标都实现整棵树。implement
  阶段也把当前 focus 目标作为语义上下文注入，防止 design 伪代码的命名漂移在实现阶段被固化。
- **成功判据**：harness 合并所有子目标候选文件为一个程序，执行入口对照期望。

> 实现状态：引擎侧 binding 链路（接受 → 继承 → 子目标进入 active context）由 traversal 单元测试
> 覆盖。G6-01 的真实 LLM 端到端实跑待 API key 环境（无 key 时干净跳过）。

### R 修复闭环（横切）

| ID | 场景 | 注入缺陷（题目，非答案） | 成功判据 |
| --- | --- | --- | --- |
| R-01 | 加一 action 的坏候选 | C 风格 `int`、`output` 缺花括号、body 引用未声明变量 | 预算内修好 → check 通过 → `IncrementCounter(41)=42` |

R 类用例显式给正修复预算，考察「真实诊断驱动的收敛」（与各组可组合）。

---

## 五、工程结构

所有 e2e 用例共享**一个** harness（消除脚手架重复）：

```
cli/examples/e2e/
├── main.rs          ← 入口：选组 / 选用例（--group / --case / --list）、跑、报告
├── harness.rs       ← 复用件：LLM 后端构造（+ 瞬时抖动重试）、design→implement→check→repair
│                       驱动、真实 tools/check 桥接、v0 解释器执行（含 console 校验 + 真实
│                       host 注入）、防泄漏的 prompt 组装
└── cases/
    ├── mod.rs               ← 用例注册表（按组）
    ├── g1_basics.rs         ← G1 用例 + R-01
    ├── g2_effects.rs        ← G2 用例（含 G2-03 真实网络）
    ├── g3_heuristic.rs      ← G3 用例（调度器自主推进）
    ├── g4_complex.rs        ← G4 用例（跨调用 / error algebra / 可失败返回）
    ├── g5_file.rs           ← G5 用例（标准库 File，真实临时文件）
    └── g6_tree.rs           ← G6 用例（目标树遍历）

scripts/run_e2e.sh           ← 串行批量执行器（逐用例各起一进程，日志落盘 + 汇总）
```

一条用例用统一的 `Case` 描述（题目 + 入口 + 期望 + 可选待修坏候选），harness 据 `CaseKind`
（`DesignImplement` / `RepairSeed` / `Scheduler` / `Tree`）分派驱动路径。新增用例 = 在对应组文件
加一个 `Case` 并在 `mod.rs` 登记，无需碰 harness；若引入新领域词汇，须在 `render.rs` 的防泄漏
断言里登记 token。

### 与 CI 的关系

e2e 默认**不进** `cargo test`。其防泄漏纪律的**结构化部分**（语法基线 / 库资产不含任务 token）由
`sophia-prompt` 的单元测试守护，**那部分进 CI**。e2e 本身作为按需手动 / 定时运行的真实链路验证。
