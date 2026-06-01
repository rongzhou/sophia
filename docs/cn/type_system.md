# F1 设计：类型语法统一 —— `of` 关键字族 + `<>` 专属 Intent

> **状态：设计定稿（2026-05-30 决策锁定），待实现。** 对应 `dev_checklist_v1.md` 工作流 B · F1，
> 来源演示需求 D1（可失败结果建模）/ D3（严肃管线）。本文档定**整套类型语法的统一规则**及其全链路
> 落点。**单一路径、无旧接口兼容、不以语法糖为借口**——这是 v1 起步阶段的一次性彻底重构。
>
> 设计准则（`language_design.md` §1.1 / §3）：强语义、少符号约定、**无例外**；不仿造泛型系统 / 模板 /
> 宏；语义直观、显式无省略、不惧繁琐。

---

## 一、核心规则（一条，无例外）

> **`Wrapper<T>` 形式专属于 Intent Type。所有结构类型用 `of` 关键字族表达。**

这条规则消除了 v0 的隐性例外（`List<T>` / `Optional<T>` 借用 `<>` 却不是 intent）。`<>` 从此**只**意味
一件事——intent 包装；结构类型一律走可朗读的关键字形态。这正是「强语义、少约定、无例外」：一条规则，
零特例，LLM 不必记"哪个 `<>` 是 intent、哪个是容器"。

### 1.1 `of` 关键字族（结构类型）

| 类型 | 语法 | 含义 |
| --- | --- | --- |
| 列表 | `list of T` | 元素同构的列表（取代 `List<T>`） |
| 联合 | `one of { A, B, ... }` | 多个**互斥结局**之一（取代 `Optional<T>`，并表达可失败返回） |
| 渐进 | `schema of T` | 渐进类型（取代 `Schema<T>`） |

`Unknown` 仍是裸关键字（无参数，不变）。未来若确有需求再扩展 `set of T` / `map of K to V` 等同族
形态——但**按需引入**，不预先设计。

### 1.2 `<>` 专属 Intent Type（不变）

8 个 intent 包装保持 `<>` 形式，且**只有它们**用 `<>`：
`Raw<T>` / `Parsed<T>` / `Validated<T>` / `Sanitized<T>` / `Verified<T>` / `Authorized<T>` /
`Secret<T>` / `Redacted<T>`。intent 严格一层、严格相等（§语言设计 7.2 不变）。

### 1.3 `Null` —— 表达"无"的内置单值类型

新增内置类型 `Null`（单一值，类似 `Unit` 但语义是"缺席/无结果"）。它主要作为 `one of` 的成员表达
可空：`one of { Todo, Null }` = "要么一个 Todo，要么无"。`Null` 是裸类型关键字，可出现在任何类型位置
（不限于 `one of` 内，但典型用法在其中）。字面值写作 `Null`。

---

## 二、`one of` 联合类型（D1 的核心）

### 2.1 形态与语义

`one of { M1, M2, ... }`：值在运行时**恰是其中一个成员**。成员可以是：标量（`Int`/`Bool`/`Text`/…）、
`Null`、已声明 entity / state、**error variant**（复用既有 error algebra 的 variant）。

```sophia
action Withdraw {
  input  { balance: Int; amount: Int }
  output { result: one of { Int, InsufficientFunds } }   # 直接列出可能结局，无包装
  body {
    if amount > balance {
      return InsufficientFunds { shortfall = amount - balance }   # 直接返回失败，无 Err()
    }
    return balance - amount                                       # 直接返回成功，无 Ok()
  }
}
```

- **无包装构造**：成功直接 `return <Int 值>`，失败直接 `return <Variant { ... }>`。没有 `Ok`/`Err`/
  `Some`/`None` 这类包装子——成员**就是它自己**。这是与 Rust `Result`/`Option` 的本质区别：Sophia 已有
  具名 variant + tagged union，不需要再套一层泛型容器。
- **可失败返回 vs `raise`**：`one of {..., SomeError}` 是**可恢复**失败——它是返回值，调用方必须显式
  处理。`raise` 仍是**不可恢复**、自动向上传播的通道（`errors {}` + `raise`，不变）。一个 variant 既
  可被某 action 作为 `one of` 成员**返回**，也可被另一处 `raise`——区别在"怎么交出去"。
- **返回的 error variant 不需进 `errors {}`**：它是被**返回**而非 **raise** 的，两通道正交。`errors {}`
  只约束 `raise` 传播。

### 2.2 distinguishability（成员须可按 tag 区分）

`one of` 的成员必须**两两可由 match tag 区分**，否则 checker 报错：
- 标量按类型名区分（`Int` / `Bool` / `Text` / …）；
- entity / state 按其名区分；
- error variant 按 variant 名区分；
- `Null` 是唯一字面。

因此 `one of { Int, Int }`、`one of { Int, Text }` 中两个标量**可**区分（按类型），但 `one of { Todo, Todo }`
或两个同类型成员不可区分 → 报错。（error variant 永远名字不同，天然可区分；约束主要落在多个同类型
非 error 成员上。）

### 2.3 `one of` 取代 `Optional`

`Optional<T>` **废弃**。"可空"统一写 `one of { T, Null }`。`Some(x)` / `None` 表达式与 pattern、
`<optional>.exists` 伪字段**一并移除**（单一路径，无兼容层）。

---

## 三、match：类型 pattern（新机制，诚实标注）

v0 的 `match` 只处理 Bool / state-value / Some-None。`one of` 要求 match 按**成员 tag** 分派，这是
**新增机制**：

```sophia
match Withdraw(b, a) {
  Int remaining                   => return remaining            # 匹配 Int 成员，绑定 remaining
  InsufficientFunds { shortfall } => return 0 - shortfall        # 匹配 variant，绑定字段
}
```

```sophia
match find_todo(id) {            # 返回 one of { Todo, Null }
  Todo t => return t.status
  Null   => raise NotFound { id = id }
}
```

pattern 形态：
- **类型 pattern** `<TypeName> <binding>`：匹配该类型成员，把值绑定到 `binding`（标量 / entity / state）。
- **variant pattern** `<VariantName> { f1, f2, ... }`：匹配该 error variant，按字段名绑定（沿用既有
  error variant 字段绑定形态）。
- **`Null`**：匹配 `Null` 成员，无绑定。
- **state-value pattern** `StateName.Value`：当成员是 state 时，可进一步按值匹配（沿用既有 state match）。
- **`Bool` 字面** `true` / `false`：当 `match` 主语是 `Bool` 时（既有，不变）。

**穷尽性（永久禁止 `_`，不变）**：`match` 一个 `one of` 必须覆盖**全部成员**；`match` 一个 `Bool` 须覆盖
`true`/`false`；`match` 一个 state 须覆盖全部值。缺成员即 `NonExhaustiveMatch` 诊断。

> 这是本设计**唯一的新机制成本**：match 从"固定几种主语"升级为"按 `one of` 成员 tag 分派 + 类型
> pattern 绑定"。语义直观（读 pattern 即知匹配什么），是诚实的、必要的新增。

---

## 四、统一后的类型词汇（全集）

```
标量：     Unit | Bool | Int | Text | Uuid | Time | Null
结构：     list of T | one of { M, ... } | schema of T | Unknown
intent：   Raw<T> | Parsed<T> | Validated<T> | Sanitized<T> | Verified<T>
           | Authorized<T> | Secret<T> | Redacted<T>
具名：     已声明 entity / state / error variant
```

`<>` ⟺ intent，`of` ⟺ 结构，裸名 ⟺ 标量 / 渐进 / 具名。**一条规则覆盖全部，无例外。**

---

## 五、废弃清单（单一路径，彻底移除，无兼容）

| 移除 | 取代 |
| --- | --- |
| `List<T>` 语法 + `Ty::List` 的 `<>` 解析 | `list of T` |
| `Optional<T>` 语法 + `Ty::Optional` | `one of { T, Null }` |
| `Some(expr)` 表达式 / `Some(x)` pattern | 成员直接构造 / 类型 pattern |
| `None` 表达式 / pattern | `Null` 字面 / pattern |
| `Schema<T>` 语法 | `schema of T` |
| `<optional>.exists` 伪字段 | 谓词上下文用 `!= Null`；body 用 `match ... { Null => ... }` |

`Value::Optional` → 用 `one of` 的运行时表示取代（见 §六）。`<text>.length` 伪字段**保留**（与本次无关）。

---

## 六、全链路落点（实现顺序 F1.1–F1.6）

| 步骤 | 层 | 改动要点 |
| --- | --- | --- |
| F1.1 | syntax（grammar + parser.c + AST + lower） | 新 `list_of` / `one_of` / `schema_of` 类型规则；移除 `generic_type` 对 List/Optional/Schema 的承载（`generic_type` 仅剩 intent）；移除 `some_expr`/`None`；新增 `Null` 字面；match pattern 增类型 pattern；AST：`TypeRef` 增 `ListOf` / `OneOf` / `SchemaOf`，`Expr` 移除 Some/None、增 `Null`，`Pattern` 移除 Some/None、增 `Type{ty_name,binding}` / `Null` |
| F1.2 | hir | 名称解析：`one of` 成员逐个解析（类型 / variant）；`Null` 内置；移除 List/Optional/Schema wrapper 名；match 类型 pattern 绑定入 scope |
| F1.3 | semantic | `Ty` 增 `OneOf(Vec<Ty>)` / 用 `ListOf`、`SchemaOf` 重命名；移除 `Ty::Optional`；`one of` distinguishability 检查；match 穷尽扩展到 `one of` 成员；类型 pattern 的成员归属与绑定类型；assignability（`one of` 子集 / 成员相容规则，见 §七） |
| F1.4 | runtime | `Value` 移除 `Optional`，`one of` 值**直接用成员自身的 `Value`**（Int 就是 `Value::Int`、variant 就是带 tag 的错误值）——无包装变体；match 按值的实际 tag 分派；`Null` → `Value::Null` |
| F1.5 | I/O 库 | 库读取操作返回 `one of { ValueTy, Null }`（命中值本身 / 未命中 `Null`，须 `match` 取值），是 `one of` 在标准库的典型用法（如未来 `File`/`DB` 的查找）。〔注：F1 落地时此规则验证于当时的 `storage` 节点；`storage` 已移除，见 `stdlib_design.md`〕 |
| F1.6 | 全仓改写 + 测试 + 文档 | 共享语法基线、例子 `.sophia`、e2e/benchmark 用例、snapshot、`language_design`/`language_implementation` 同步 |

> **关键 runtime 简化**：`one of` **不需要**新的包装 `Value` 变体。一个 `one of { Int, InsufficientFunds }`
> 的值在运行时**就是** `Value::Int(...)` 或一个带 variant tag 的错误值——它本来就是"它自己"。match 直接
> 看值的实际形状分派。这比 Rust 的 `Result`/`Option` 运行时更省（无 discriminant wrapper），也更贴合
> "成员就是自己"的语义。错误 variant 的运行时值需要一个能携带 variant tag + 字段的 `Value` 形态——复用
> 既有 `RaisedError` 的结构，但作为**返回值**（实现为 `Value::ErrorValue { variant, fields }`），区别于 `raise`
> 的控制流。

---

## 七、类型规则细节（semantic）

- **赋值相容**：`one of { A, B }` 位置可接受其任一成员的值（成员 → 联合的 upcast）；联合 → 联合当
  目标成员集 ⊇ 源成员集。成员之间不互转。`Null` 只相容 `Null` 成员。
- **return 检查**：返回 `one of {...}` 的 action，每条 `return e` 的 `e` 类型须是某成员（或可 upcast 到
  某成员）。全路径 return/raise 终止性不变。
- **match 主语**：`Bool` / state / `one of` 可 match；其它类型 match 报 `InvalidMatchSubject`。
- **distinguishability**：见 §2.2，建联合类型时静态检查。
- **list**：`list of T` 协变比较 inner（同原 `List`）。

---

## 八、对 D1 / D3 / 基准的影响

- **D1 演示题**：可失败计算返回 `one of { Int, SomeError }`，调用方 match 两成员——直接演示"可恢复
  失败 + 强制处理"，无任何包装样板。
- **D3 管线**：管线步骤返回 `one of {...}`，调用方 match 后决定继续/中止。
- benchmark 判定：hidden case 的 `ExpectedOutcome` 仍是 `Returns(Value)` / `Raises(variant)`。`one of`
  的成功成员走 `Returns(Value::Int(..))`，返回的 error variant 走 `Returns(Value::ErrorValue{..})`（**可恢复
  返回值**），与 `raise`（`Raises`，不可恢复）区分。

---

## 九、决策记录（已锁定 2026-05-30）

1. **统一规则**：`<>` 专属 intent；结构类型用 `of` 族（`list of` / `one of` / `schema of`）。**无例外。**
2. **废弃** `Optional<T>` / `List<T>` / `Schema<T>` 的 `<>` 形式、`Some`/`None`、`<optional>.exists`——
   **单一路径、彻底移除、无兼容层、不留语法糖**。
3. **`one of` 成员直接构造 / 直接 match**，无 `Ok`/`Err`/`Some`/`None` 包装子。
4. **新增 `Null`** 内置单值类型表达"无"。
5. **match 类型 pattern** 是必要新机制；穷尽性 + 禁止 `_` 不变；distinguishability 静态检查。
6. **可空查找返回 `one of { ValueTy, Null }`**：标准库的查找类操作（命中值 / 未命中 `Null`）是 `one of`
   的典型用法（如未来 `File`/`DB` 的读取）。〔注：F1 落地时验证于当时的 `storage` 节点，`storage` 已移除〕
7. **范围**：`one of` 可用于任意类型位置（output / input / field / let），不限 output——它是基础类型
   构造，限制反而是额外规则（违背"少约定"）。

> 这是 v1 起步阶段的一次性彻底重构：**总语法表面更小**（少了 `<>`/`of` 的二义、少了 Some/None/包装子），
> **规则更强**（一条无例外），是强论证的 LLM-native 收益，不是架空。

---

## 十、变更记录

- 2026-05-30 — 取代已废弃的 `result_type.md`。经讨论否决 Rust 式 `Result<T,E>`（被 Rust 心智带偏、
  且方案 C 下 wrapper 无信息量）。改为：`one of {...}` 联合类型直接表达可失败 / 可空返回（成员即自己、
  无包装子），并顺势**统一全部类型语法**——`<>` 专属 intent、结构类型用 `of` 关键字族、废弃
  `Optional`/`List<>`/`Schema<>`/`Some`/`None`、新增 `Null`、match 引入类型 pattern。单一路径、彻底
  重构、无兼容、不留语法糖。
