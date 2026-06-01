# 标准库 · `File`（本地文件访问）

> 本文档定标准库 **`File`** 的完整设计：语言契约（effect / 类型 / capability / intent 边界）与真实
> host。它是 `stdlib_design.md` §六库清单登记的库；标准库的总体框架（提示词脚手架、host 注入通用机制）
> 见 `stdlib_design.md` / `stdlib_implementation.md`。
>
> **状态：已落地（2026-05-31）。** 来源：本地文件访问是与网络同级的基础 I/O 能力（优先级不低于
> `Http`）；移除语义不清的 `storage` 节点后（见 `engineering_notes.md` 2026-05-31 决策），文件读写以
> **库**形式提供，是 (B)「I/O = 库」模型的第二个落地库。设计准则与 `Http` 一致：**零新语法**（与
> `Console`/`Http` 同构，复用 effect/capability/intent 三套机制）、**功能库而非协议栈**（只做读 / 写
> 文件这一功能，不碰文件系统底层如权限位 / 符号链接 / mmap）、host **诚实**（失败即 `Err`，绝不伪造）。
> §2.6 四决策点已确认采纳；`File.Read` + `File.Write` 均已实现，全链路（parse→check→run）+ intent
> accept/reject 测试通过。
>
> **库插件重构（2026-05-31）**：`File` 已从 `core` 硬编码迁出，改为**清单驱动**——契约由
> `sophia-stdlib/libs/file/library.toml` 声明、经 `LibraryRegistry` 注入各层；真实 host 由
> `sophia-stdlib::register_native_hosts` 注册进 `HostRegistry`（路线 B）。下文 §2.x/§3.x 描述的语义不变，
> 但落点已从「`BUILTIN_EFFECT_OPS` + `infer_effect_op` 命令式 match + `EffectHost` 方法 + CLI `CliHost`」
> 改为「清单 op 契约 + 类型层表驱动校验 + `HostRegistry` 闭包（native：`std::fs`）」。权威实现机制见
> `stdlib_design.md` / `stdlib_implementation.md`。

---

## 一、动机与定位

文件读写是"严肃程序"的基础能力（配置、数据、日志）。移除 `storage` 后，持久化 / 本地状态的演示需求
由 `File` 库承接（端到端验收见 `e2e_test.md` G5-01：`File.Write` 写出 → `File.Read` 读回 → intent
转换的真实临时文件往返）。`File` 与 `Http` 是一对：都从外部取回**不可信数据**（`Raw<Text>`），
都经 intent 边界静态管控下游使用——这让 intent 安全的论证不局限于网络，也覆盖本地文件（同样是
"数据经历过什么"的可检查事实）。

`File` 与 `Http` 的对称：

| | `Http` | `File` |
| --- | --- | --- |
| 读取 | `Http.Get(url) -> Raw<Text>` | `File.Read(path) -> Raw<Text>` |
| 写入 | （v1 无） | `File.Write(path, content)`（见 §2.3 决策） |
| 资源标识 | url（运行时绑定值） | path（运行时绑定值） |
| 返回不可信 | 网络响应不可信 | 文件内容不可信（外部来源） |
| capability 粒度 | `allow { Http.Get }` | `allow { File.Read }` / `allow { File.Write }` |

---

## 二、语言契约

### 2.1 语法形态（零新语法，与 `Http` 同构）

`File.Read` / `File.Write` 是 **effect 操作**，复用"特殊根 method_call + host 委派"路径（与 `Http.Get`
完全同构）：

```sophia
action LoadConfig {
  capability: FileCapability
  input  { path: Text }
  output { content: Sanitized<Text> }
  effects { File.Read }
  body {
    let raw = File.Read(path)        # method_call：base=File, method=Read, args=[path]
    let clean = Trust(raw)           # 经 intent_conversion 转为可信
    return clean
  }
}
```

- `File` 是**特殊根标识符**（类比 `Http` / `storage` 此前），HIR 名称解析放行，不进 ASG index。
- grammar / AST / lower **零改动**——`File.Read(path)` 经现有 `method_call` 规则解析。

### 2.2 操作集（最小，按需）

| 操作 | 签名 | effect | 说明 |
| --- | --- | --- | --- |
| `File.Read(path)` | `(Text) -> Raw<Text>` | `File.Read` | 读取文件全文，类型为不可信 `Raw<Text>` |
| `File.Write(path, content)` | `(Text, Sanitized<Text>) -> Unit` | `File.Write` | 把可信文本写入文件（覆盖） |

- **`File.Read` 必做**（与 `Http.Get` 对称，是 intent 安全演示的本地版）。
- **`File.Write` 待 §七决策点确认**：写入要求 `content` 为 `Sanitized<Text>`（不可把未经处理的 `Raw`
  直接落盘——与 `Console.Write` 的输出 intent 边界同理），体现"写出边界"。若 v1 演示只需读，可仅做
  `File.Read`，`File.Write` 随 D3 重做时一起落地。
- 追加 / 二进制 / 目录遍历 / 删除 / 元数据等**不预先设计**——出现演示需求再按设计门增量。

### 2.3 effect 层

`File.Read` / `File.Write` 进 `BUILTIN_EFFECT_OPS`，arity=0（**effect 身份不带 path arg**，与 `Http.Get`
决策一致，见 §2.4）：声明形态 `effects { File.Read }` / `allow { File.Write }`。`used ⊆ declared` 与
`Pure` 冲突检查完全复用。

> arity=0 只约束声明位（`effects {}` / `allow {}` 经 HIR `resolve_effect` 校验）；body 调用
> `File.Read(path)`（带 path）走 `File` 特殊根 method_call 路径（`resolve_value_ident` 放行 + 语义
> `infer_effect_op` 校验 `path:Text`），不经 arity 表。

### 2.4 capability 层

`capability FileCapability { allow { File.Read; File.Write } }`。**effect 身份只到 `File.Read` /
`File.Write`，不含 path 实参**——理由同 `Http.Get`（见 `http_lib.md` §2.6）：path 通常是运行时绑定值，
作 arg 只会被 `covered_by` 当通配，无管控收益。capability 授权"能否读 / 写文件"这一能力。

> 未来若需"限定目录白名单"，那是 host 层策略，非语言 capability（与 Http 的"域名白名单"同理）。

### 2.5 intent 边界（核心，复用既有检查，零改动）

- `File.Read(path)` 返回 **`Raw<Text>`**（不可信，外部来源）——下游直接当 `Sanitized<Text>` 用 →
  `IntentMismatch` 静态拒绝；唯一合法路径是经 `intent_conversion: true` 转换 action。
- `File.Write(path, content)` 的 `content` 要求 **`Sanitized<Text>`**——不可把 `Raw<Text>` 直接落盘
  （与 `Console.Write` 只接受字面量 / `Sanitized` / `Redacted` 同理，是写出边界）。

这让 `File` 也能构造 accept/reject 矩阵条目（本地文件版的 intent 安全），与 `Http` 的旗舰演示对称。

### 2.6 已确认决策点（2026-05-31，均采纳）

1. **`File.Read` + `File.Write` 同入 v1 首版**（D3 用 File 重做需要 Write）——**采纳**。
2. **`File.Read` 返回 `Raw<Text>`、`File.Write` 收 `Sanitized<Text>`**（intent 边界对称 Http/Console）
   ——**采纳**。
3. **effect 身份不带 path arg**（capability 粒度到"能否读/写"）——**采纳**（与 Http 一致）。
4. **特殊根 `File`** 与 `Http` 同列 body 内置根——**采纳**。

---

## 三、host：mock 与真实文件

### 3.1 host 接口

`EffectHost` trait 新增方法（与 `http_get` 并列）：

```rust
/// 读取文件全文（不可信文本）。失败（不存在 / 无权限 / 非 UTF-8 等）→ Err，硬错误阻断。
fn file_read(&mut self, path: &str) -> Result<String, String>;
/// 写入文件（覆盖）。失败 → Err。（若 File.Write 纳入 v1）
fn file_write(&mut self, path: &str, content: &str) -> Result<(), String>;
```

### 3.2 `InMemoryHost` 确定性 mock

`InMemoryHost` 维护 `path -> content` 内存桶（`seed_file` 预置），用于一切确定性测试：
- `file_read(path)`：命中预置即返回；**未命中即 `Err`**（诚实阻断，不伪造）；
- `file_write(path, content)`：写入内存桶（不触真实文件系统），便于 read-after-write 测试。

mock 性质明确标注"非真实文件系统"。

### 3.3 真实 host：CLI 协调层 `CliHost`

真实文件 I/O 属**协调层（CLI）**，不进 `runtime`（解释器保持零 IO）。`CliHost` 组合委派——复用
`InMemoryHost` 的 console（print），覆盖 `file_read` / `file_write` 为真实 `std::fs`：
- `file_read`：`std::fs::read_to_string(path)`，失败 → `Err`；
- `file_write`：`std::fs::write(path, content)`，失败 → `Err`；
- **注入判定**：CLI `run` 据入口 action `declared_effects` 含 `File.Read`/`File.Write` 才注入真实文件
  host（无文件程序零开销，机制见 `stdlib_implementation.md` §三）。

> 真实文件 I/O 不进 `cargo test`（与 Http 真实网络同策略）；接缝单测用 mock host。
> 安全：真实 host 在受限路径下操作（文档标注；未来可加沙箱根目录策略）。

---

## 四、提示词资产

`File` 的 LLM 提示词资产是 `workflow/prompt/assets/stdlib/file.md`（用途 / `File.Read(path) -> Raw<Text>`
+ `File.Write` 操作 / effect+capability / intent 边界 / 中立示例），按 `stdlib_design.md` §3.1 结构组织，
**不进**常驻语法基线（按需注入）。design 阶段经库目录 `stdlib_catalog` 让 LLM 自选（catalog 增一行
`file — 本地文件读写`），implement 阶段注入完整资产。

---

## 五、全链路落点（实现顺序）

| 步骤 | 层 | 改动要点 |
| --- | --- | --- |
| F.1 | hir | `File.Read`/`File.Write` 进 `BUILTIN_EFFECT_OPS`（arity=0）；特殊根 `File` 放行 |
| F.2 | semantic | `infer_effect_op` 识别 `File.Read(path)`/`File.Write(path, content)`：校验 path/content 类型、并入 effect、返回 `Raw<Text>`/`Unit`；intent 边界复用 |
| F.3 | runtime | `EffectHost::file_read`/`file_write`；`InMemoryHost` 加 path→content mock 桶 + `seed_file`；`interp::try_effect_op` 识别 `File.*` 委派 |
| F.4 | CLI host | `CliHost` 覆盖 `file_read`/`file_write` 为真实 `std::fs`；按入口 effect 判定注入 |
| F.5 | 资产 + 测试 | `assets/stdlib/file.md` + `stdlib_catalog` 行；semantic/runtime 回归 + intent reject/accept；接缝单测 |
| F.6 | 文档 | 本文档转定稿；`stdlib_design.md` §六库清单登记；`language_design`/`language_implementation` effect 表补 `File` 族 |

---

## 六、变更记录

- 2026-05-31 — 设计门草案。`File` 本地文件访问库，与 `Http` 同构（特殊根 method_call + effect/capability +
  intent 边界，零新语法）；`File.Read(path) -> Raw<Text>`（不可信，须经 intent 转换）+ `File.Write(path,
  Sanitized<Text>)`（写出边界）；mock host（`seed_file`）+ 真实 `std::fs` host（CLI 协调层）。承接 storage
  移除后的本地持久化演示需求（D3 重做用之）。待确认 §2.6 四个决策点。
- 2026-05-31 — **落地（§2.6 四决策点采纳）**。hir（`File.Read/Write` 进 `BUILTIN_EFFECT_OPS` arity=0、
  特殊根 `File` 放行）/ semantic（`infer_effect_op` 识别 `File.Read`→`Raw<Text>` / `File.Write`→`Unit`，
  校验 path:Text、content:Sanitized<Text>，intent 边界复用）/ runtime（`EffectHost::file_read/file_write`
  + `InMemoryHost` 内存桶 + `seed_file` + `interp::try_effect_op` 统一 File/Http）/ CLI host（`CliHost`
  覆盖 `file_read/write` 为真实 `std::fs`，按入口 effect 含 `File.*` 判定注入）/ 提示词资产
  `assets/stdlib/file.md` + `stdlib_catalog` 行。回归测试：semantic（clean / 未声明 effect / read Raw 直用
  reject / write Raw content reject）+ runtime（write→read 往返 / seed_file 读 / 缺文件诚实 Err）+ CLI host
  接缝（委派 / 缺文件 Err / 真实写读往返）。全工作区 336 passed / 0 failed。真实文件 IO 不进 `cargo test`。
- 2026-05-31 — **演示验收（R3）：D3 + e2e 用例落地**。`File` 库经两个集成演示验收：① benchmark L6
  `archive_or_reject`（D3 严肃管线综合题，取 `File.Read` → 校验 `one of` match → intent 转换 →
  `File.Write` 写出 → `File.Read` 读回，经 mock host `seed_file` 确定性执行）；② e2e G5-01 笔记写入与
  回读（`File.Write`→`File.Read`→intent 转换的自包含 write→read 往返，默认 host 执行）。手解验证两者
  均可表达、可执行（D3 成功路径返回 Int 5 + reject 返回 `Rejected{amount}`；G5 往返返回 Int 5）。
  benchmark `Problem` 增 `file_seed`（path→content mock，两 mode 共享、不进 prompt），baseline runner
  注入对称 mock `file_read` / `file_write`。详见 `integration_demos.md` / `benchmark_design.md` /
  `e2e_test_design.md`。无库本体代码改动（演示验收复用 R2 已落地的 `File` 全链路）。
- 2026-05-31 — **测试三类化：`File` 的端到端验收并入 e2e（用真实 IO）**。确立测试只有三类（单元 /
  e2e / 基准），mock 仅单元测试可用，e2e / benchmark 一律真实 IO。原 D3 benchmark mock 文件题
  `archive_or_reject` 随之移除（网络 / 文件题不入 benchmark——禁 mock、真实 IO 不确定不公平）；`File`
  库的端到端验收收敛到 **e2e G5-01**（`File.Write` → `File.Read` → intent 转换的 write→read 往返），
  并改造为打**真实临时文件**（harness 据入口 `File.*` effect 注入真实 `CliHost`，非内存桶 mock）。
  benchmark `Problem.file_seed` 字段 + runner mock `file_read` / `file_write` 注入一并删除。测试组织
  详见 `e2e_test.md`（G5-01）/ `benchmark_test.md`（§一.4 禁 mock）/ `unit_test.md`。`File` 库本体
  代码不变。
