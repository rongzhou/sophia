# JSON 库设计草案

> 状态：v2 设计草案。本文记录“围绕实现 JSON 三方库而补齐前置语言扩展（`Text` / `while`），再从
> JSON validate / parse 跑到 agent example”的目标、现状约束与路线图。
> 它不是已确认的语言规范；真正进入实现前，应把需要改动的语言能力拆成独立设计评审与工程任务。
>
> 相关文档：`language_design.md`、`stdlib_design.md`、`stdlib_implementation.md`、`type_system.md`、
> `wasm_codegen.md`、`engineering_architecture.md` §14.3、`dev_checklist_v2.md`。

---

## 一、目标

Sophia 目前已有 `File` / `Http` 标准库，能够从文件或网络取得文本，但还缺少把外部文本转成可检查结构的
能力。JSON 是最小且有代表性的下一步：`Http.Get(url)` 取得 `Raw<Text>` 后，如果能用 Sophia 代码验证或
解析 JSON，就可以编写更接近真实 agent 的程序，例如：

1. 从 HTTP API 获取响应；
2. 验证响应是否为 JSON；
3. 解析出需要的字段；
4. 根据字段做领域决策或调用后续 action。

本草案的核心目标是：

- 把 v2 收束为一条端到端主线：`Text` / `while` 前置语言扩展 → JSON 三方库 → agent example；
- 用一个三方库实践库插件模型，而不是把 JSON 直接塞进语言核心；
- 检验 LLM 是否能在 Sophia 的显式语法与检查器约束下实现非平凡 parser / validator；
- 形成 `Http` + JSON + 领域 action 的可用示例，让 Sophia 从“可执行示例”迈向“能处理真实数据”的程序。

优先级上，**先 validator，后 parser**；先可验证的最小 JSON 子集，后完整 JSON / schema。

---

## 二、为什么适合做成三方库

JSON 不是语言核心机制。语言核心应只提供确定性的基础值操作、类型系统、effect / capability / intent 机制；
JSON 的语法、错误分类、解析策略和数据模型属于可复用功能单元，适合作为库。

这也符合 `stdlib_design.md` 的库边界：

- 纯逻辑优先用 Sophia 源码库表达；
- 库知识通过 `library.toml` + prompt asset 提供给 LLM；
- 库源码作为额外 ASG 输入并入 index / semantic model / runtime；
- 三方库在项目根 `sophia_libs/` 或 `$SOPHIA_LIB_PATH` 下发现，不改 core。

如果最终 JSON parser 能以纯 Sophia 实现，它将成为比 `hash_sophia` 更有价值的三方库样板。

---

## 三、当前现状

### 3.1 已具备的基础

当前项目已经具备以下前提：

- 三方库发现：`sophia-stdlib::full_registry_for(project_root)` 会合并标准库与三方库；
- 纯 Sophia 库：`library.toml` 的 `[surface].sophia_sources` 可把库 `.sophia` 文件并入用户程序；
- 库 domain 隔离：库源码 domain = 库名，用户引用库节点时由 HIR 豁免库 domain 的跨 domain 诊断；
- 运行时执行：库 action / transition 与用户 action 同列进入 `SemanticModel` 与 `ExecGraph`；
- 已有三方库示例：`stdlib/tests/fixtures/sophia_libs/hash_sophia`。

这些说明“纯 Sophia JSON 库”在装载、索引、检查和执行路径上是可行的。

### 3.2 主要缺口

当前语言还不能直接写出可靠 JSON parser，关键缺口集中在 `Text` 与循环表达力：

1. **Text 缺少位置访问**
   - 目前可用能力主要是字面量、拼接、相等比较与 `.length`。
   - parser 必须能读取第 `i` 个字符，或者取出一个区间。

2. **Text 缺少 slice / substring**
   - 解析 string literal、number、object key 时需要从输入中截取片段。
   - 没有 slice 时只能累积拼接，复杂且容易低效。

3. **Text 缺少字符分类**
   - JSON validator 至少需要判断空白、数字、引号、反斜杠、结构字符。
   - 可以先用 `char == " "` 这类比较实现，但需要 `char_at` 返回单字符 Text。

4. **循环只有 `repeat n times`**
   - JSON parser 常见写法是“while cursor < length 且满足条件”。
   - 起步阶段可以用 `repeat text.length times` 加内部状态模拟，但会比较繁琐。
   - v2 将 `while condition { ... }` 列为明确前置语言扩展目标，而不是等 LLM 多次失败后再补。
   - 这里不是追求省略糖，而是为游标式 parser 提供直接、诚实、可检查的控制流形态。

5. **递归数据模型需要谨慎验证**
   - JSON value 是递归结构：object / array 内可嵌套 value。
   - Sophia 已有 entity、list、one of，但递归 entity / 递归 union 的 checker、runtime、codegen 支撑需要单独确认。

6. **库 op 签名 DSL 不适合表达复杂返回**
   - `TypeDesc` 目前只支持 `Int` / `Bool` / `Text` / `Unit` 与 intent 包装。
   - 纯 Sophia 库不受 `TypeDesc` 限制，可以定义自己的 entity / error / action。
   - 但如果把 JSON parse 做成 host op，就会立刻需要扩展 `TypeDesc`，因此不应先走 host op 路线。

---

## 四、路线选择

### 4.1 推荐路线：先补 Text + while，再做纯 Sophia 库

推荐路线是：

1. 在语言核心补最小、确定性的 `Text` 值操作；
2. 新增 `while condition { ... }`，支撑游标式解析循环；
3. 用这些操作实现纯 Sophia JSON validator；
4. 再扩展为 parser 或有限结构化访问；
5. 最后接入 `Http` 演示真实数据处理。

这样能最大化验证 Sophia 自身的表达能力，也能避免过早把 JSON 变成 Rust / WASM host 黑盒。

### 4.2 暂不推荐路线：JSON 作为 host op

可以把 JSON 解析写成 `Json.Parse(text)` host op，但这不适合作为第一步：

- 它绕过了“LLM 编写 validator/parser”的验证目标；
- 它需要扩展 `TypeDesc` 支持库自定义复杂类型或返回 `Text` 再二次处理；
- 它会把最有价值的 parser 逻辑藏到 host 内部，降低 Sophia 语言本身的证明力度。

host op 可以作为后续性能或完整 JSON 支持的备选，但不应作为 MVP。

---

## 五、前置语言能力

### 5.1 Text 最小能力

建议先引入以下纯值能力。它们不是 effect，不需要 capability；它们应像 `Text.length` 一样成为 core value
operation，并保持解释器与 WASM codegen 对称。

| 能力 | 形态 | 返回 | 用途 |
| --- | --- | --- | --- |
| 字符读取 | `text.char_at(index)` | `Text` | 读取单个 Unicode scalar 或单字节字符 |
| 切片 | `text.slice(start, length)` | `Text` | 截取字符串片段 |
| 前缀判断 | `text.starts_with(prefix)` | `Bool` | 简化固定 token 判断，可选 |

MVP 可以只做 `char_at` + `slice`。`starts_with` 可由库代码组合实现，但作为基础能力能显著降低 LLM 生成难度。

需要明确的边界：

- 索引采用 Unicode scalar 还是 UTF-8 byte offset。现有 `.length` 是 Unicode scalar count；为一致性，
  `char_at` / `slice` 应优先采用 Unicode scalar 索引。
- 越界语义。建议越界返回 `""` 还是 runtime error 需要设计评审。parser 场景更适合返回空 Text，避免每次访问前重复分支；
  但 Sophia 一贯偏向诚实错误，需权衡。
- WASM codegen 必须同步实现，不能只支持解释器。

### 5.2 while 控制流目标

v2 明确新增 `while condition { ... }` 作为 JSON 库的前置语言扩展。旧的替代模式是：

```sophia
let mutable cursor = 0
repeat input.length times {
  if cursor < input.length {
    // inspect input.char_at(cursor)
    // set cursor = cursor + 1
  }
}
```

该模式虽然能表达，但会把 parser 的核心逻辑埋进“固定次数循环 + 内部 if + 手动停机状态”的样板里，增加
LLM 生成错误和人工审阅成本。`while` 的 v2 目标不是引入复杂并发或异步语义，而是补一个同步、确定、直接的
循环形式。

建议语法：

```sophia
while cursor < input.length {
  let ch = input.char_at(cursor)
  set cursor = cursor + 1
}
```

设计边界：

- 条件表达式必须为 `Bool`；
- body 与 `repeat` 共用块语义、scope 规则、`return` / `raise` 终止性分析；
- `while` 没有 `break` / `continue` 作为 MVP，提前结束通过更新循环条件中的状态表达；
- runtime 与 WASM codegen 均按同步循环实现；
- checker 只保证类型与 effect 合法，不证明循环终止。

---

## 六、JSON 库 MVP 范围

### 6.1 第一阶段：validator

第一阶段只验证文本是否是 JSON，不返回 JSON AST。

建议公开 action：

```sophia
action ValidateJson {
  input { text: Raw<Text> }
  output { result: one of { JsonValid, JsonInvalid } }
  body { ... }
}
```

建议库内定义：

- `entity JsonValid { ... }`
- `entity JsonInvalid { reason: Text; position: Int }`
- 若需要硬错误，可定义 `error JsonParseError`，但 validator 更适合返回 `one of`，避免把普通非法输入变成 runtime failure。

MVP JSON 子集：

- object：`{}`
- array：`[]`
- string：双引号字符串，先支持常见 escape；
- int：十进制整数；
- bool：`true` / `false`；
- null：`null`；
- whitespace：空格、换行、回车、tab。

暂缓：

- 小数；
- exponent；
- `\uXXXX`；
- JSON Schema；
- 保留 key 的完整抽取。

### 6.2 第二阶段：parser

第二阶段返回结构化 JSON value。

初步模型：

```sophia
entity JsonString { fields { value { type: Text } } }
entity JsonInt { fields { value { type: Int } } }
entity JsonBool { fields { value { type: Bool } } }
entity JsonNull { fields { value { type: Unit } } }
entity JsonMember { fields { key { type: Text } value { type: JsonValue } } }
entity JsonArray { fields { items { type: list of JsonValue } } }
entity JsonObject { fields { members { type: list of JsonMember } } }
```

这里的 `JsonValue` 只是概念名。当前 Sophia 没有 type alias；若要表达递归 `one of`，需要进一步确认语言是否允许
entity 字段中直接写递归 union，或需要拆成更显式的非递归 MVP。

因此 parser 阶段前必须先完成“递归数据模型可行性评估”。

### 6.3 第三阶段：HTTP agent 示例

在 validator / parser 可用后，增加一个端到端示例：

1. `Http.Get(url)` 获取 `Raw<Text>`；
2. 调用 `ValidateJson` 或 `ParseJson`；
3. 对解析结果做领域判断；
4. 返回结构化 entity 或领域错误。

该示例用于证明 Sophia 能处理真实 API 响应，而不仅是 toy arithmetic / todo 流程。

---

## 七、验证策略

### 7.1 确定性测试

JSON 库应先作为三方库 fixture 加入测试：

```text
stdlib/tests/fixtures/sophia_libs/json/
  library.toml
  json.md
  src/*.sophia
```

测试应覆盖：

- 三方库 discovery；
- `sophia check` 合并库源码；
- interpreter 执行 validator；
- WASM backend 与 interpreter 等价；
- strip-assist 等价不受库源码干扰。

### 7.2 用例集合

validator 起步用例：

- `{}`
- `[]`
- `{"ok":true}`
- `{"items":[1,2,3]}`
- `{"nested":{"a":null}}`
- 缺右括号；
- 多余逗号；
- 未闭合字符串；
- 非法 token；
- 合法 JSON 后存在尾随垃圾。

### 7.3 LLM 生成能力评估

该库的一个核心价值是评估 LLM 编写 validator/parser 的能力。建议使用 Development Graph 路线：

1. 先由人写清楚目标与边界；
2. 让 LLM 生成 `.pseudo`；
3. 从 `.pseudo` 生成 `.sophia`；
4. 通过 hidden cases gate；
5. 保留失败路径，分析哪些语言能力或 prompt 资产导致失败。

这比直接手写最终库更符合 Sophia 项目的研究目标。

---

## 八、风险与开放问题

1. **Text 索引语义**
   - Unicode scalar 与 byte offset 如何取舍？
   - 越界返回空 Text 还是 hard error？

2. **while 语法细节**
   - MVP 是否需要 `break` / `continue`？当前建议不需要。
   - `while` 的终止性是否只作为运行时责任？当前建议 checker 不证明终止。
   - 是否允许 condition 中调用有 effect 的 action？当前建议沿用表达式既有 effect 规则，不额外开洞。

3. **递归 JSON value**
   - 当前 type/check/runtime/codegen 是否接受递归 entity / union？
   - 如果不接受，parser MVP 是否先返回特定字段抽取结果，而不是完整 AST？

4. **Intent 边界**
   - `ValidateJson` 是否应从 `Raw<Text>` 返回 `Validated<Text>`？
   - 如果 parser 返回结构化值，是否需要表达“该结构来自已验证 JSON”？

5. **错误模型**
   - 非法 JSON 是返回 `JsonInvalid`，还是 raise `JsonParseError`？
   - 推荐 validator 返回值表达非法输入，parser 可在无法恢复时 raise。

6. **是否进入标准库**
   - 初期应保持三方库，以实践插件机制。
   - 若后续成为大量示例依赖的基础能力，再评估是否提升为标准库。

---

## 九、建议路线图

### R0：设计冻结

- 明确 JSON MVP 子集；
- 明确 Text 原语语义；
- 明确 `while` 语法、scope、终止性与 codegen 形态；
- 明确 validator 返回模型；
- 写出库 prompt asset 草案。

### R1：Text 原语落地

- syntax / lower 支持 `text.char_at(index)` 与 `text.slice(start, length)`；
- semantic 校验签名；
- interpreter 实现；
- WASM codegen 实现；
- 增加差测试，保证 interpreter 与 WASM 一致。

### R2：while 控制流落地

- syntax / lower 支持 `while condition { ... }`；
- HIR scope 与名称解析复用 block 规则；
- semantic 校验条件为 `Bool`，并入 body 的 type / effect / flow 分析；
- interpreter 与 WASM codegen 实现同步循环；
- 增加差测试，覆盖循环 0 次 / 多次 / 提前通过状态结束 / return / raise。

### R3：JSON validator 三方库

- 新增 `sophia_libs/json` fixture；
- 用 LLM 生成或辅助生成 `.pseudo` 与 `.sophia`；
- 覆盖合法 / 非法 JSON hidden cases；
- CLI 手动 smoke：项目根发现库并运行。

### R4：HTTP + JSON agent 示例

- 编写 `Http.Get` + `ValidateJson` 的 agent-like 示例；
- 验证 capability / effect 声明完整；
- 记录 LLM 是否能根据库 catalog / asset 选择并使用 JSON 库。

### R5：parser 与结构化访问

- 评估递归 JSON value 模型；
- 若递归模型可行，实现 `ParseJson`；
- 若递归模型暂不可行，先实现有限字段抽取或 flat object parser；
- 再决定是否扩展 type alias / recursive union / 更丰富 Text API。

---

## 十、当前判断

“用纯 Sophia 实现 JSON validator/parser 三方库”是合理且有价值的方向，但它不是当前语言立即可完成的任务。
它应作为 v2 的需求牵引：先补最小 `Text` 能力与 `while` 控制流，再用三方库和 Development Graph 验证
LLM 写 parser 的真实能力。

最佳第一步不是直接写 JSON parser，而是完成 R0/R1/R2：把 parser 所需的最小、确定性、可 codegen 的
`Text` 操作与 `while` 控制流补齐。
