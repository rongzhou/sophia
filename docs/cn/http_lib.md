# 标准库 · `Http`（网络获取）

> 本文档定标准库 **`Http`** 的完整设计：语言契约（effect / 类型 / capability / intent 边界）与真实
> host。它是 `stdlib_design.md` §六库清单登记的首个库；标准库的总体框架（提示词脚手架、host 注入
> 通用机制）见 `stdlib_design.md` / `stdlib_implementation.md`。
>
> **状态：已落地（2026-05-30）。** 来源演示需求 **D2**（网络获取 + intent 安全，旗舰 LLM-native 演示）。
> 语言契约 + 真实 host 均已实现，全链路（parse→check→run）+ accept/reject 测试通过。
>
> **库插件重构（2026-05-31）**：`Http` 已从 `core` 硬编码迁出，改为**清单驱动**——契约由
> `sophia-stdlib/libs/http/library.toml` 声明、经 `LibraryRegistry` 注入各层；真实 host 由
> `sophia-stdlib::register_native_hosts` 注册进 `HostRegistry`（路线 B）。下文 §2.x/§3.x 描述的
> effect / 类型 / intent / host **语义不变**，但落点已从「`BUILTIN_EFFECT_OPS` + `infer_effect_op` 命令式
> match + `EffectHost::http_get` 方法」改为「清单 op 契约 + 类型层表驱动校验 + `HostRegistry` 闭包」。
> 权威实现机制见 `stdlib_design.md` / `stdlib_implementation.md`。
>
> 设计准则：**零新语法**——`Http` 与既有 `Console` 同构，复用 `(family, op, args)` effect 三元组
> + capability + intent 三套已实现机制；**功能库而非协议栈**（只做 `Http.Get` 这一功能，不碰 TCP/TLS）；
> host **诚实**（绝不伪造网络成功）。

---

## 一、演示目标（D2 旗舰）

落地技术报告 §7/§8 的旗舰主张：把"数据经历过什么"变成**机器可检查的语言事实**，构造一条真实
**accept/reject 矩阵**条目：

- **Sophia（accept 正例 + reject 反例）**：`Http.Get(url)` 取回的数据类型即 `Raw<Text>`（不可信）。
  下游若**直接**把它当可信值用（写入需 `Sanitized<T>` 的边界、或赋给 `Validated<T>` 字段）→ checker
  **静态拒绝**（intent 严格相等，`language_design.md` §7.2）；只有经**显式 intent 转换 action** 处理后
  才能使用 → **accept**。
- **baseline（TS + tsc）**：`fetch(url)` 返回 `string`，直接当可信值用 → tsc **接受**（类型系统无 intent
  概念）。

这正是"为何专门服务 LLM 自动编程"的强论证：同一个不安全模式，Sophia 在编译期拦截，主流语言放行。
端到端验收见 `e2e_test.md`（G2-03 网络获取 + intent 安全，真实站点）+ 确定性矩阵 `cli/tests/intent_matrix.rs`（reject 半静态拒绝，见 `unit_test.md`）。

---

## 二、语言契约

### 2.1 语法形态（零新语法，与 storage 同构）

`Http.Get` 是一个 **effect 操作**，既要**注册副作用**（用于 effect/capability 检查），又要**返回一个值**
（`Raw<Text>`）。这与 body 级 storage 操作（`storage.X.get(key)` 返回值 + 并入 `DB.Read`）**完全同构**——
复用既有的"特殊根 + 方法调用"识别路径，无需任何新语法：

```sophia
action FetchProfile {
  capability: NetCapability
  input  { url: Text }
  output { body: Raw<Text> }
  effects { Http.Get }
  body {
    let raw = Http.Get(url)        # method_call：base=Http, method=Get, args=[url]
    return raw                     # raw : Raw<Text>
  }
}
```

- 解析形态：`Http.Get(url)` 经 grammar 现有 `method_call` 规则解析为
  `MethodCall { base: Ident("Http"), method: "Get", args: [url] }`——与 `storage.Todos.get(k)` 同一形状
  （仅 base 由 `storage.<Name>` 字段访问换成裸 `Http` 标识符）。**grammar / AST / lower 零改动。**
- `Http` 是**特殊根标识符**（类比 `storage`），HIR 名称解析放行（不报"未声明变量"），不进 ASG index。

### 2.2 为何不用顶层 `effect Http {...}` 用户声明

`Http` 是**内置** effect 族（与 `Console`/`DB` 同列入 `hir::builtins::BUILTIN_EFFECT_OPS`），不是用户
`effect` 声明。理由：① 它有**固定的 host 语义**（真实网络调用），不是领域自定义的契约；② 它的返回类型
（`Raw<Text>`）是语言内建的 intent 约定，需类型层特判（与 storage get 返回 `one of {V,Null}` 同理）。
用户 `effect` 声明的 operation 无返回值绑定语义，不适合承载"取回不可信数据"。

### 2.3 操作集（按需，最小）

| 操作 | 签名 | effect | 说明 |
| --- | --- | --- | --- |
| `Http.Get(url)` | `(Text) -> Raw<Text>` | `Http.Get` | GET 取回响应体，类型为不可信 `Raw<Text>` |

**仅 `Http.Get`**（功能库最小集，D2 演示只需取回）。`Http.Post` / 头部 / 状态码 / JSON 解析等**不预先
设计**——出现演示需求再按设计评审增量推进（避免架空协议栈）。`url` 参数类型为 `Text`（起步子集标量；不引入
`Url` 专类型，无需求）。

> **取舍记录**：`Http.Get` 返回 `Raw<Text>` 而非 `one of { Raw<Text>, HttpError }`。理由：D2 的演示焦点
> 是 **intent 安全**（不可信数据经类型管控），不是网络失败建模；网络失败 → 领域错误的映射属 host 错误
> 语义，留待真实 host 落地时若演示需要再扩展为 `one of {...}`（F1 的 `one of` 已就绪，到时零语言
> 改动）。当前失败时返回 `RuntimeError`（硬错误阻断），不伪造成功、也不静默吞错。

### 2.4 类型层

`Http.Get(url)` 的推断在 `type_layer` 的 `infer_effect_op`（与 `infer_storage_op` 并列的特判）：

- 识别 `MethodCall { base: Ident("Http"), method: "Get", args: [url] }`；
- 校验 `args.len() == 1` 且 `url` 推断为 `Text`（不符报 `TypeMismatch`）；
- 并入 effect `Http.Get`（无 arg——见 §2.6 capability）；
- 返回类型 `Ty::Intent(IntentKind::Raw, Box::new(Ty::Text))`。

下游的 intent 严格相等检查（已实现）**无需任何改动**即可拦截"`Raw<Text>` 直接当 `Sanitized<Text>` 用"
——这正是 D2 reject 用例的拦截点。

### 2.5 effect 层

`Http.Get` 进 `BUILTIN_EFFECT_OPS`（`("Http", "Get", 0)`，arity=0——**effect 身份不带 URL arg**，
见 §2.6 决策 1；声明形态 `effects { Http.Get }` / `allow { Http.Get }` 同 `Console.Write` 的 0 参）。
`used ⊆ declared` 与 `Pure` 冲突检查**完全复用**——body 用了 `Http.Get` 必须在 `effects { Http.Get }`
声明，否则 `UndeclaredEffect`。

> arity=0 仅约束**声明位**（`effects {}` / `allow {}` 的 effect 引用，经 HIR `resolve_effect` 校验）。
> body 级调用 `Http.Get(url)`（带 url 实参）走的是 `Http` 特殊根 method_call 路径（HIR
> `resolve_value_ident` 放行 + 语义 `infer_effect_op` 校验 `url:Text`），**不**经 `resolve_effect` 的
> arity 表——故 arity=0 与 body 调用带 url 不冲突。

### 2.6 capability 层

`capability NetCapability { allow { Http.Get } }`。capability 匹配复用 `Effect::covered_by`——
**关键决策：effect 身份只到 `Http.Get`，不含 URL 实参**。即注册的 effect 是 `Http.Get`（无 arg），
capability 写 `allow { Http.Get }` 即可授权。

> **为何不把 URL 作为 effect arg**（对比 storage 的 `DB.Read("Todos")` 把存储名作 arg）：storage 名是
> **静态已知的资源标识**（capability 可精确到某张表）；而 URL 通常是**运行时绑定值**（input 参数），
> 静态未知，作 arg 只会被 `covered_by` 当通配处理（见 `effect.rs`），无实际管控收益，反而使 capability
> 声明形态摇摆。故 `Http.Get` 的 effect 身份**不带 arg**——capability 授权"能否发 HTTP GET"这一能力，
> 符合 capability 的语义粒度。未来若需"限定域名白名单"，那是 host 层策略，非语言 capability。

### 2.7 intent 边界（D2 的核心，已实现，零改动）

`Raw<Text>` 的下游约束完全由既有 intent 检查兑现：
- `Console.Write` 只接受字面量 / `Sanitized<T>` / `Redacted<T>` → 直接 `print Http.Get(url)` 结果被拒；
- 赋给 `Sanitized<Text>` / `Validated<Text>` 字段或 output → intent 严格相等拒绝；
- 唯一合法路径：经 `intent_conversion: true` 的转换 action（如 `Sanitize(Raw<Text>) -> Sanitized<Text>`）。

### 2.8 已确认决策点（2026-05-30）

1. **`Http.Get` 不带 URL 作 effect arg**（capability 粒度到"能否 GET"，§2.6）——**采纳**。
2. **`Http.Get` 返回裸 `Raw<Text>`** 而非 `one of { Raw<Text>, HttpError }`（网络失败走 host 硬错误，
   不在 v1 建模为可恢复返回；§2.3 取舍）——**采纳**。
3. **特殊根 `Http`** 与 `storage` 同列为 body 内置根标识符——**采纳**。

---

## 三、host：mock 与真实网络

### 3.1 host 接口（与具体 host 实现无关的契约）

`EffectHost` trait 的方法：

```rust
/// 处理 `Http.Get(url)`：取回响应体（不可信文本）。
/// 返回 Ok(body 文本) 或 Err（网络/IO 失败，硬错误阻断——绝不伪造成功）。
fn http_get(&mut self, url: &str) -> Result<String, String>;
```

解释器在 `MethodCall` 求值时识别 `Http.Get` 特殊根（与 `try_storage_op` 并列的 `try_effect_op`），委派
`host.http_get(url)`，把返回文本包成 `Value::Text(body)`（运行时不携带 intent 标签——intent 是编译期静态
属性，运行时只留结构）。host import 的通用注入机制（`run_action` / 组合 host / 按 effect 判定
注入）见 `stdlib_implementation.md` §三。

### 3.2 `InMemoryHost` 确定性 mock

`InMemoryHost` 提供**确定性 mock**，**不做真实网络**：

- 维护一个 `BTreeMap<String, String>`（url → 预置响应体），由测试 / harness 经 `seed_http` 预置；
- `http_get(url)`：命中预置则返回对应文本；**未命中则返回 `Err`**（硬错误阻断，诚实——绝不编造
  "默认成功响应"）；
- mock 性质在 host 文档与 trace 中**明确标注**"非真实网络"。

mock host 用于一切确定性测试（含 D2 的 benchmark accept 题、集成演示）；真实网络一律走 §3.3。

### 3.3 真实 host：CLI 协调层 `CliHost`

真实网络 host 属**协调层（CLI）**，不进 `runtime`（解释器保持零 IO）。`CliHost` **组合委派**——console /
storage 复用 `InMemoryHost`，只覆盖 `http_get` 为真实 `reqwest::blocking`：

```rust
fn http_get(&mut self, url: &str) -> Result<String, String> {
    let resp = self.http.get(url).send().map_err(|e| format!("Http.Get 请求失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Http.Get 非 2xx 状态：{status}"));   // 诚实阻断，不当成功
    }
    resp.text().map_err(|e| format!("Http.Get 读取响应体失败：{e}"))
}
```

- **同步**：用 `reqwest::blocking`（与 `http_get` 的 sync 签名匹配；`run` 命令本就是同步路径，无需把
  tokio runtime 穿进解释器）。workspace `reqwest` 加 `blocking` feature（仅 CLI 用到）。
- **超时**：client builder 设固定超时（如 10s），避免挂死；超时 → `Err`（诚实阻断）。
- **错误映射（本阶段）**：网络失败 / 非 2xx / 读取失败一律 → `Err(String)`，解释器物化为 `RuntimeError`
  （硬错误中止）。**不**把网络失败映射成领域 `one of {..., HttpError}` 返回值——与 §2.3 决策一致；若日后
  演示需要可恢复网络失败，再按 F1 的 `one of` 扩展 `Http.Get` 返回类型（语言层改动，走设计评审流程）。
- **注入判定**：CLI `run` 默认用 `InMemoryHost`；**当入口 action 的 `declared_effects` 含 `Http.Get`**
  才构造 `CliHost` 注入真实网络——无网络程序零开销、零行为变化（机制见 `stdlib_implementation.md` §3.2）。

### 3.4 真实 host 决策点（2026-05-30）

1. **真实 host 在 CLI（协调层）、组合委派复用 `InMemoryHost`**（runtime 保持零 IO）——**采纳**。
2. **sync `reqwest::blocking` + 固定超时**（不把 tokio 穿进解释器）——**采纳**。
3. **网络失败 → 硬错误 `RuntimeError`**（不建模为 `one of` 可恢复返回；与 §2.3 一致）——**采纳**。
4. **CLI `run` 据入口 action 的 `declared_effects` 含 `Http.Get` 才注入 `CliHost`**（无网络程序零开销）
   ——**采纳**。

---

## 四、与既有机制的一致性核对

| 机制 | storage（已实现） | `Http`（本库） | 是否复用 |
| --- | --- | --- | --- |
| 调用形态 | `storage.X.get(k)`（特殊根 method_call） | `Http.Get(url)`（特殊根 method_call） | ✅ 同构，零新语法 |
| effect 注册 | `DB.Read("X")`（arg=存储名） | `Http.Get`（无 arg，见 §2.6） | ✅ 同三元组机制 |
| 返回值特判 | `infer_storage_op` → `one of {V,Null}` | `infer_effect_op` → `Raw<Text>` | ✅ 并列特判 |
| host 委派 | `storage_get/save` | `http_get` | ✅ 同 trait 扩展 |
| capability | `allow { DB.Read("X") }` | `allow { Http.Get }` | ✅ `covered_by` 复用 |
| 诚实性 | 内存桶（非持久化） | 内存 mock / 真实网络失败硬错误 | ✅ 同诚实标注纪律 |

**新增面仅"一个内置 effect 三元组 + 一个 host 方法 + 一个类型层特判 + 一份提示词资产 + CLI 真实 host"**
——扩展面小、说服力大，符合标准库的强论证准入。

---

## 五、提示词资产

`Http` 的 LLM 提示词资产是 `workflow/prompt/assets/stdlib/http.md`（用途 / `Http.Get(url) -> Raw<Text>`
操作 / effect+capability / `Raw<Text>` intent 边界 / 中立示例），按 `stdlib_design.md` §3.1 结构组织，
**不进**常驻语法基线（库知识按需注入，见 `stdlib_design.md` §3）。注入机制（design 看目录 `stdlib_catalog`
自选 → implement 注入 `stdlib_preamble`）见 `stdlib_implementation.md` §二。

---

## 六、变更记录

- 2026-05-30 — `Http` 语言契约落地：内置 effect 族，复用 storage 的"特殊根 method_call + host 委派"
  路径（零新语法）；`Http.Get(url) -> Raw<Text>`，intent 边界拦截不可信数据（D2 reject 用例）；
  mock host 诚实（未命中即 `Err`）。三决策点（§2.8）确认采纳，hir / semantic / runtime 全链路实现 +
  9 测试。
- 2026-05-30 — 真实 host 落地：`CliHost` 在 CLI 协调层组合委派复用 `InMemoryHost`、只覆盖 `http_get`
  为真实 `reqwest::blocking`（固定超时 + 诚实错误）；runtime 暴露 `run_action` 注入接缝；
  CLI `run` 据入口 effect 含 `Http.Get` 判定注入。四决策点（§3.4）确认采纳。接缝单测覆盖（委派等价 /
  注入路径 / effect 判定），真实网络不进 `cargo test`。
