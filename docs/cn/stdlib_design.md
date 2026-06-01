# Sophia 标准库与库插件设计

> 本文档与 `language_design.md` 对称：`language_design` 定**语言本身**的设计，本文档定**库**的
> 设计——库是什么、边界在哪、怎么组织、如何统一标准库与三方库、LLM 如何发现与使用。具体某个库的
> 语言契约（如 `Http`）单独成文（见 §七 库清单），实现细节见 `stdlib_implementation.md`。
>
> **状态：活文档。** 库随演示需求增量。当前：库插件模型已落地（清单驱动 + `LibraryRegistry` +
> 路线 B host 注册表）；标准库 `Http` / `File`（已落地，见 `http_lib.md` / `file_lib.md`）。

---

## 一、定位：库是什么

库是 Sophia 在**核心语言之外**、按需提供的**功能单元**——让 `.sophia` 程序能调用文件、网络等外部
能力，或复用一组纯逻辑 action，而无需把这些塞进语言核心。文件 / 网络 / 数据库这类 I/O 能力**都是库**
（多数语言的传统），不是语言原语。三个固有约束（承自语言定位 `language_design.md` §1/§3）：

1. **功能库，而非协议栈**：随演示需求按需引入**最小功能**，不自建底层协议。网络只需"从 URL 取数据"
   就只提供 `Http.Get`，不自建 TCP/TLS；文件只需读 / 写就只提供 `File.Read`/`File.Write`。无需求不加。
2. **无 ambient authority，effect 显式声明**：库的副作用能力一律是**可观测 effect**，用到必须
   `effects {}` 声明 + capability `allow`，与语言的 effect/capability 机制一致——库不开任何隐式副作用后门。
3. **契约与实现分离**：库的 effect 只定义**语言侧契约**（签名、类型、intent 边界、capability 形状）；
   真实副作用由 host 提供，`core` / `runtime` 不内置任何具体库（见 §五）。

> **机制 vs 能力族（关键边界）**：语言**核心**提供 effect / capability / intent **机制** + 通用语法
> （`effects {}` / `capability {}` / `effect` 声明 / 特殊根 method_call 调用形态）；**库**提供具体能力
> （`File` / `Http` / 未来 `DB` / 三方库）。
>
> **`Console`（`print`）是例外，保留为语言内置**：输出是调试 / 诊断原语，几乎所有语言内置，且已走
> effect/capability、无 ambient authority 问题。它不在库清单里，由 `hir::builtins::BUILTIN_EFFECT_OPS`
> 承载（唯一内置 effect 族），随常驻语法基线提供。

> **准入门槛**：库扩充走与语言扩充**同一套需求驱动 + 设计门**纪律——由具体演示需求触发、单库过设计门，
> 不预先铺功能清单。

---

## 二、核心模型：清单 = 单一真相源，注册表 = 各层只读数据源

> **历史背景**：库曾**不是一个结构实体**——一个库（如 `Http`）的契约散落在 6 个 crate 的 9 处硬编码
> const 表 / match 臂（HIR 注册 / HIR 特殊根 / 语义签名 / 运行时接口 / 运行时分派 / 真实 host / host
> 选择 / codegen / 提示词）。后果：① 库知识渗透进语言核心每一层；② 三方库无样板可循、无从入手。
> 根因是**索引方向反了**（`层 → {所有库的切片}`）。现模型**倒转索引**为 `库 → 清单 → 注册表 → 各层`。

一个库 = 一个**目录** + 一份**清单**（`library.toml`）。清单把该库在各层的契约集中到一处；各层从清单
构建的 **`LibraryRegistry`** 派生，而非各自硬编码。这是「库不渗透语言核心」的结构落点。

### 2.1 库的两条正交维度（surface × host）

一个库的能力沿**两条独立维度**描述——这是统一标准库 / 三方库、纯 Sophia 库 / WASM 库的概念核心：

- **surface（库怎样暴露能力）**：① **Sophia 源码节点**（库随附 `.sophia`，作为额外 ASG 输入；只能
  *组合*已有 effect/op，**不能**引入新原语）；② **effect-op 清单声明**（库声明新 effect 族 / 操作，
  调用形态 `Lib.Op(args)`；这是引入新原语的唯一途径，必须落到 host）。
- **host（原语 effect 怎样落地）**：**none**（纯 Sophia 组合）/ **native-Rust**（编译进二进制，标准库）/
  **WASM**（库目录内沙箱模块，三方库新原语）。

由此得四种库形态（覆盖全部需求）：**纯 Sophia 库**（source + none）/ **native-effect 库**（op + native，
如 `Http`/`File`）/ **WASM-effect 库**（op + WASM，三方新原语）/ **混合库**（op + source）。surface 与
host **正交**，都**不与执行模式绑定**——见 §六跨模式对称。

### 2.2 目录布局（四形态同形）

```
<库根>/<libname>/
  library.toml          # 清单：身份 + effect-op 签名 + host 绑定 + surface/资产引用
  <libname>.md          # 提示词资产（按 §三.1 结构）
  src/                  # （可选）Sophia 源码节点：*.sophia
  host.wasm             # （可选，仅三方 WASM-effect 库）host 实现
```

- **标准库根** = `sophia-stdlib` crate 内 `libs/`（编译进二进制，`include_str!`）。
- **三方库根** = 启动时发现的目录（§五.1）。
- **库标识 = 目录名**（小写，唯一）。一个库全部资产都在目录内——边界即命名空间。

### 2.3 清单 schema（`library.toml`）

```toml
[library]
name = "http"                            # = 目录名，唯一库标识
summary = "从网络 URL 获取数据（网络请求）"  # 一句话用途（进库目录）
abi_version = 1                          # 清单 schema 版本（不支持版本启动报错）

[surface]                                # （可选）Sophia 源码节点
sophia_sources = ["src/retry.sophia"]

[[op]]                                   # （可选，每 op 一条）effect 操作契约
family = "Http"                          # effect 族 = 特殊根名
op = "Get"
params = ["Text"]                        # 有序，中立 TypeDesc（§2.4）
returns = "Raw<Text>"
effectful = true                         # 是否需要 host（false = 纯计算 op）
host_fn = "http_get"                     # host 分派键 / WASM import 名

[prompt]
asset = "http.md"                        # 提示词资产文件（相对库目录）
```

### 2.4 类型描述符 mini-DSL（库能说什么）

```
TypeDesc := Scalar                  # Int | Bool | Text | Unit
          | Intent "<" Scalar ">"   # Raw<Text> | Sanitized<Text> | ...（intent 名取自核心固定集）
```

刻意只覆盖现有库形状（`Http`/`File` 全部签名都在内）。**故意不支持**库自定义类型作参/返、泛型、
`one of`、`list of`——将来某库需要时再扩 DSL + 过设计门（YAGNI）。这是把语义层 effect-op 校验从「逐 op
命令式 match」改为「解释清单 TypeDesc」（表驱动）的依据：每库从「改 Rust」坍缩为「写清单」。

### 2.5 `LibraryRegistry`（各层只读数据源）

注册表由一组库清单构建（标准库静态 / 三方启动时发现），**构建后冻结**（确定性门禁前提）。它承载：
op 契约（`family.op` → 签名 / 返回 / effectful / host_fn）、特殊根 family 集、提示词资产（目录 + 完整
文）、Sophia 源码节点。各层消费它替代硬编码：

| 层 | 消费方式 |
| --- | --- |
| HIR 注册（effect 符号表） | `AsgIndex::with_libraries(registry)` 注入 effect-op + 特殊根 family |
| HIR 特殊根放行 | `AsgIndex::is_library_family`（替代 `File`/`Http` 白名单） |
| 语义签名校验 | `index.library_op(family, op)` 的 TypeDesc 表驱动校验（替代 `infer_effect_op` 命令式 match） |
| 运行时分派 | `HostRegistry` 按 `(family, op)` 委派（路线 B，§五.3） |
| codegen | emit 的 host import 从注册表派生 |
| 提示词 | `registry.catalog()`（design 目录）/ `registry.preamble(libs)`（implement 资产） |

**去渗透判定标准**：`core/hir`、`core/semantic`、`runtime` 的代码里**不再出现** `"Http"`/`"File"`/`"Get"`
等具体库字面量（`Console` 是唯一例外）——它们只认 `registry` / `index`。

### 2.6 crate 结构（标准库是 crate）

- **`sophia-library`**（core 层，契约类型）：`LibraryRegistry` / `OpContract` / `TypeDesc` / 清单解析。
  无 `runtime::Value`，故 `core/*` 可依赖（避免依赖环）。零文件 IO（只解析传入的清单字符串）。
- **`sophia-stdlib`**（内容层）：`libs/<lib>/`（清单 + 资产 + native host）+ `standard_registry()` +
  `register_native_hosts` / `mock_host`。归 core 之上、协调层之下（可做 IO）。**`core` 不依赖它**——
  `core/semantic` 像接收 `&SemanticModel` 一样接收 `&AsgIndex`（携库契约）只读参数。
- **`HostRegistry` / `HostFn`** 落 `sophia-runtime`（需 `Value`）。

依赖图无环：`sophia-library ← core/hir, core/semantic, sophia-runtime`；`sophia-runtime, sophia-library
← sophia-stdlib`；`sophia-stdlib ← cli / tools/check / tools/codegen / lsp`。

---

## 三、提示词脚手架：基线 vs 库目录 vs 库资产

> 来源演示需求 **D2**（网络获取 + intent 安全）。LLM 对 Sophia 库**没有先验知识**——不告知用途与用法
> 它写不出用到库的 `.sophia`。库知识由 `LibraryRegistry`（`sophia-stdlib`）承载，**不在 prompt crate**。

LLM 写 `.sophia` 时，prompt 注入三类资产，**边界分明**：

| | 常驻语法基线 `sophia_syntax_baseline` | 库目录 `registry.catalog()` | 库资产 `registry.preamble(libs)` |
| --- | --- | --- | --- |
| 内容 | 核心语言语法 | 每库一行「名 — 用途」 | 选中库的完整用法（签名 / intent 边界 / capability） |
| 注入阶段 | implement / repair（常驻） | design / revise（LLM 据此选库） | implement / repair（只注入 design 选中的库） |

**核心边界**：库知识**不进**常驻语法基线（基线只承载核心语法）。`prompt` crate 只持基线 + 模板；库目录 /
资产由调用方从 `LibraryRegistry` 算得后传入（`design_solution` 模板的 `stdlib_catalog` 变量 = `catalog()`；
`implement_system_prompt(stdlib_block)` 的 `stdlib_block` = `preamble(libs)`）。

### 3.1 库资产的标准结构（每份 `<lib>.md` 遵循）

1. **用途**（一两句）；2. **操作**（签名 + 语义）；3. **effect 与 capability**（声明 + allow 形状）；
4. **intent 边界**（返回值的 intent 约束）；5. **中立示例**（与任何任务无关的最小用法，仅示形状）。
资产**只含可泛化用法 + 中立示例**，不含任何任务答案 / 领域名 / 业务逻辑（防泄漏，snapshot + 断言守护）。

### 3.2 两阶段：design 选库（看目录）→ implement 用库（拿资产）

库选择是 **LLM 在 design 阶段的决策**（写入 `design_result.libraries`），**不是任务预声明**（后者泄漏
解法方向、且随三方库增长需推倒重做）。design / revise 注入极简目录；implement / repair 据所选库注入完整
资产（按库名字典序拼接，去重，未知库忽略，空集零注入）。选库经 `PseudocodeArtifact` / scheduler /
`run_implement_loop` / `StepPrompts` 全链路贯通到 implement system prompt（graph CLI 跨进程经 `.libs`
sidecar 持久化）。

---

## 四、范围与非目标

- **不做协议栈**：只提供功能层。
- **不开 ambient authority**：所有库能力是显式声明的 effect。
- **不让库定义新 intent 种类**：`Raw`/`Sanitized`… 是核心安全词汇，库只能**引用**（§六安全红线）。
- **不让库改语法 / 加关键字 / 加类型构造器**：只能声明 effect 族 + 操作签名。
- **不预先铺库清单**：单库过设计门、需求驱动增量。
- **不做实时热加载**：三方库只在启动时一次性发现，注册表随后冻结。

---

## 五、装载：标准库静态 / 三方启动时一次性

### 5.1 发现

- **标准库**：`sophia-stdlib` 的 `libs/` 编译进二进制（清单 + 资产 + 源码 `include_str!`，native host
  链接）。注册表标准库部分进程初始化时构建，**零 IO、确定**。
- **三方库**：进程启动时按约定顺序扫描——① 项目根下 `<root>/sophia_libs/`，② 环境变量
  `$SOPHIA_LIB_PATH`（冒号分隔多目录）——逐子目录读 `library.toml` + 资产 + （若有）`.sophia` 源码 + `host.wasm`，合并进
  注册表。**只在启动做一次**，随后冻结。失败（清单非法 / `abi_version` 不符 / 名冲突 / wasm 校验失败）→
  **启动期诚实报错退出**，不静默跳过、不部分加载、不静默覆盖同名。发现实现 =
  `sophia-stdlib::full_registry_for(project_root)`（CLI 各命令用）/ `project_roots(project_root)`（只计算发现根）/
  `full_registry_from(roots)`（指定根，供测试）；
  库 Sophia 源码经 `hir::LibrarySources::from_registry` 解析为 owned AST，与用户 AST 同列进 index /
  model / 执行。**已落地（P2 + CLI 生产接线）**——演示库 `hash_sophia`（纯 Sophia）/ `hash_wasm`（WASM）
  经此发现执行（见 §六）；CLI `check` / `run` / `index` / `graph` / `context` / `repair-context` 各命令
  均经 `full_registry_for(root)` 发现三方库、并入库 Sophia 源码、`run` 经 `register_wasm_library_hosts`
  注册三方 WASM host（见 §五.3）。**确定性子门禁**（`tools/check::check_program` / `codegen` / LSP）仍只用
  `standard_registry`——三方发现是协调层启动行为，不进核心确定性门禁。

### 5.2 冲突与隔离

库名唯一、effect family 唯一、Sophia 源码 domain 唯一（库名即 domain，与用户 domain 隔离）——任一冲突
启动报错。一库全部资产在自己目录内，注册表按库聚合，互不重叠由结构保证。

### 5.3 host 分派 = 路线 B（开放注册表）

host 是 `HostRegistry: Map<(family, op), Box<dyn HostFn>>`（不是固定方法集 trait）。`Console`（`print`）
例外，由 `HostRegistry::console_write` 单独捕获。

- **标准库**：`sophia-stdlib::register_native_hosts` 注册真实 `reqwest` / `std::fs` 闭包；`mock_host`
  注册确定性内存桶（测试 / 差测试）。
- **三方 WASM 库**：`sophia-stdlib::register_wasm_library_hosts(host, registry)` 遍历注册表里携带
  `host.wasm` 字节的库（= 三方 WASM-effect 库），为其每个 effect-op 注册一个 `WasmHostFn`——内部持
  `host.wasm` 的 `wasmi` 实例，按统一字节 ABI 转发。区分准则 = **装载方式**（注册表是否持 `host.wasm`），
  与标准库 native host 互补不重叠。当前 ABI 子集 `(Int, Int) -> Int`（标量 i64 直传）；超出子集的签名 /
  装载失败一律诚实 `Err` 阻断（不静默跳过、不伪造 host），待 ABI 随需扩展。
- **诚实性红线**：mock 未命中 / 真实失败 / wasm trap 一律 `Err` 阻断，解释器物化为硬错误，绝不伪造。

路线 B 的回报：**native 与 WASM host 在解释器看来同构为 `Box<dyn HostFn>`**——解释器对「effect 背后是
Rust 还是 wasm」无感知，这是跨模式对称（§六）的实现支点。

### 5.4 统一 host ABI

复用 codegen 的字节级 `sophia_host` import ABI（W4 的 console_write / file_write / file_read / http_get /
read_copy，host 只收发线性内存字节、不识值布局）作为 native 闭包与三方 `host.wasm` 的**单一** host 契约
——单一 ABI、已验证、跨模式同接缝。

---

## 六、跨模式执行：两种库形态 × 两种模式（保住 oracle 不变量）

surface 与 host 都**不与执行模式绑定**，故四象限全部可用：

| | 解释模式 | VM 模式（WASM codegen） |
| --- | --- | --- |
| 纯 Sophia 源码库 | ✅ 库节点是普通 ASG 输入 | ✅ 库节点随用户码一起 emit |
| native-effect 库（标准库） | ✅ `HostRegistry` native 闭包 | ✅ 模块声明 import，实例化方提供 native |
| WASM-effect 库（三方） | ✅ `WasmHostFn`（内嵌 `wasmi` 调 `host.wasm`） | ✅ `host.wasm` 作 import 提供者 |

- **纯 Sophia 源码库两模式天然通用**——它就是 Sophia 代码（解释器解释它、codegen 编译它），零额外机制。
- **WASM host 库两模式也通用**——VM 模式天然；解释模式靠**内嵌 `wasmi`**（已提为 `sophia-runtime` 正式
  依赖，仅三方 WASM 库触发,标准库零开销;解释模式接线 = `runtime::WasmHostFn`,持 `host.wasm` 的 `wasmi`
  实例、注册进 `HostRegistry`）。
- **不能按模式割裂库能力**：项目铁律是「解释器是唯一 oracle」，差测试要求同程序经解释器与 WASM 逐 case
  等价。若某类库只能在一种模式跑，用了它的程序就没有跨模式基准 → 破坏不变量。故库能力**必须跨模式对称**。

> **已落地（P2）**：两个演示库 `hash_sophia`（纯 Sophia 源码库）/ `hash_wasm`（WASM-effect 库,
> `host.wasm` 经 `WasmHostFn`）计算同一确定 digest,`cargo test` 验收发现 + 跨 domain 豁免 + 两库结果逐位
> 相等。WASM host ABI（统一字节级契约,标量 i64 直传 / 文本经 ptr-len + stash + read_copy,export 方向）见
> §五.4。VM 模式 import←`host.wasm` 链接随差测试扩展。

---

## 七、安全边界（不渗透核心的硬约束）

| 库**能**做 | 库**不能**做（schema / 注册期拒绝） |
| --- | --- |
| 声明 effect 族 + 操作（`Lib.Op(args)`） | 新增 intent 种类（核心固定集，只能引用） |
| 用受限 TypeDesc 定签名 | 加语法 / 关键字 / 类型构造器 / 泛型 |
| 提供 Sophia 源码节点（库 domain 内） | 注入对用户 domain 可见的新类型（仍由用户 `.sophia` 定义） |
| 提供提示词资产 | 改 parser / 类型系统结构 / 求值规则 |
| 绑定 host（native Rust / 三方 WASM 沙箱） | 开 ambient authority（所有 op 仍须 effect 声明 + capability allow） |

- **intent 词汇属核心**是不可妥协红线：若三方能注入 intent 种类 = 让三方定义安全语义。清单 TypeDesc 里
  的 intent 名经核心 `IntentKind::from_head` 解析——未知 intent 名保守恢复为 `Ty::Error`，绝不放行。
- **三方 host = WASM 沙箱**：capability（谁能触发 effect）× WASM 沙箱（隔离 host 实现本身）正交组合——把
  「三方 host = 任意代码执行」降为「加载一个沙箱模块」，安全且可移植。
- **纯 Sophia 三方库零 host 风险**：它没有 host，就是受检的 Sophia 代码，走与用户码同一套静态检查。

---

## 八、库清单

| 库 | 用途 | 形态 | 语言契约文档 | 状态 |
| --- | --- | --- | --- | --- |
| `Http` | 网络 GET 取回响应体（不可信 `Raw<Text>`） | native-effect | `http_lib.md` | 已落地 |
| `File` | 本地文件读 / 写 | native-effect | `file_lib.md` | 已落地 |
| `DB` | 持久化数据存储 | （未定） | （未建） | **未来候选**：需先澄清语义（KV / 关系 / 文档？后端？一致性？）再过设计门；不在 v1 |

> 新增库：加 `libs/<lib>/`（清单 + 资产 + 可选源码 + 可选 host）→ 在 `sophia-stdlib::STDLIB_LIBS` 登记
> 一行（标准库）或放入三方根目录（三方）→ 新建 `<lib>_lib.md` 契约文档（若 effectful）。详见
> `stdlib_implementation.md` §五。
>
> **历史说明**：v0 起步期曾有 `storage` 顶层节点 + `DB.Read/Write` 内置 effect（按名分桶内存 KV），因
> 语义不清已移除；持久化能力未来以语义清晰的 `DB` 库重新提供。本地状态 / 持久化的近期演示需求由 `File`
> 库承接。

---

## 九、变更记录

- 2026-05-31 — 建立标准库设计文档（与 `language_design.md` 对称）：定位、两类知识、基线 vs 库资产边界、
  两阶段提示词脚手架。
- 2026-05-31 — 确立 (B)「I/O = 库」模型，移除 `storage`，库清单新增 `File` / `DB`（未来候选）。
- 2026-05-31 — **库插件模型落地（消化原 `library_plugin.md` 设计门）**。把库从散落各层的硬编码切片重构
  为**清单 = 单一真相源 + `LibraryRegistry` = 各层只读数据源**（倒转索引）。新增两条正交维度（surface ×
  host）统一标准库 / 三方库、纯 Sophia 库 / WASM 库；二元装载路径（标准库静态编译 / 三方启动时一次性发现 +
  WASM 沙箱 host）；表驱动化 effect 签名（TypeDesc mini-DSL，替代命令式 match）；host 分派改路线 B
  （`HostRegistry: Map<(family,op), Box<dyn HostFn>>`，native 与 WASM 同构）；标准库抽为 `sophia-stdlib`
  crate + 契约类型 `sophia-library` crate（`core` 经只读 `&AsgIndex`〔携库契约〕消费、不依赖 stdlib）；
  跨模式对称（四象限可用，保 oracle 不变量）；intent 词汇属核心安全红线。实现 P1（标准库样板化、纯重构、
  零行为变化）落地，P2（三方动态发现 + WASM host 内嵌 `wasmi`）待真实三方需求触发。
- 2026-05-31 — **库插件 P2 落地（三方动态发现 + 两个演示库，消化原 `library_plugin_p2.md`）**。三方库
  **启动时一次性发现**：`sophia-stdlib::full_registry_for(project_root)`（项目根 `sophia_libs/` +
  `$SOPHIA_LIB_PATH`）/ `full_registry_from`（指定根，供测试 fixture）；库 Sophia 源码经 `hir::LibrarySources::from_registry`
  解析为 owned AST 与用户 AST 同列进 index / model / 执行。**跨 domain 豁免**：`AsgIndex.library_domains`
  （从 `registry.sophia_sources` 收集）+ `resolve` 对「用户 → 库 domain」放行 `ImplicitCrossDomain`
  （用户↔用户仍受检）。**WASM host**：`wasmi` 提为 `sophia-runtime` 正式依赖 + `runtime::WasmHostFn`（持
  `host.wasm` 实例，统一字节 ABI，§五.4 / §六.1）。**两演示库**：`hash_sophia`（纯 Sophia 源码库，action
  `SophiaDigest`）/ `hash_wasm`（WASM-effect 库，op `WasmHash.Mix`，`effectful=false`，`host.wasm` 由
  wasm-encoder 测试时生成）计算同一确定 digest（`acc=acc*31+value` ×3）。`cargo test` 集成测试
  （`stdlib/tests/library_demo.rs`）验收：发现 + 注册表合并 + 跨 domain 豁免 + 纯 Sophia 库执行 + WASM 库
  经 `WasmHostFn` 执行 + 两库结果逐位相等。全确定、进门禁（369 passed）。**CLI 生产接线**
  （`library_registry(root)` → `full_registry_for(root)` + 库源码并入各命令 inputs + `sophia run` 注册三方 WASM host）
  列为后续项——机制 + 确定性 demo 已就位。
- 2026-05-31 — **CLI 生产路径接线落地（库插件 P2 收尾）**。CLI `library_registry(root)` 由
  `standard_registry` 改为 `full_registry_for(root)`（以项目根解析 `<root>/sophia_libs/` + `$SOPHIA_LIB_PATH`
  发现三方库，返回 `Result`，发现失败诚实报错退出）；新增 `discover::project_roots` / `full_registry_for`。
  `check` / `run` / `index` / `graph` / `context` / `repair-context` 各命令经 `library_context(root)` 把库随附
  Sophia 源码（`LibrarySources`）并入 program inputs + asts（纯 Sophia 库节点须建模才可解析 / 执行）。
  `sophia run` 经 `sophia-stdlib::register_wasm_library_hosts` 据注册表 `host.wasm` 注册三方 WASM host
  （`WasmHostFn`），与标准库 native host 互补（按装载方式区分）；ABI 子集外签名 / 装载失败诚实 `Err`。
  `tools/check::check_strip_assist_equivalence` 改为 registry-aware（两侧对称并入库源码 + 同一 registry，
  否则用户引用库节点会让 strip 前后解析不对称误判）。**确定性子门禁**（`check_program` / `codegen` / LSP /
  graph gate 与 LLM 工作流命令）仍只用 `standard_registry`。手动 smoke（项目带 `./sophia_libs/{hash_sophia,
  hash_wasm}`）：`check` 通过、`run ViaSophia` / `run ViaWasm` 均得同一 digest 210523。全工作区 372 passed
  （+3 WASM host 注册测试：标准库 no-op、ABI 子集外拒绝、非法 wasm 字节拒绝）/ 0 failed，clippy 0 警告，fmt clean。
