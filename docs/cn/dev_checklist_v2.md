# Sophia 工程进展 · v2（dev_checklist_v2）

> **v2 开发进度跟踪 SSOT。** 本文档用于跟踪 v2：从 JSON 库需求牵引出的前置语言扩展（`Text` / `while`），
> 到纯 Sophia JSON 三方库，再到 `Http` + JSON 的 agent-like 示例。v0 / v1 进展分别归档于
> `dev_checklist_v0.md` / `dev_checklist_v1.md`；跨版本工程决策日志仍统一记录在 `engineering_notes.md`。
>
> v2 阶段定位见 `engineering_architecture.md` §14.3；JSON 库设计草案见 `json_lib_design.md`。
> 状态：**已完成** / **进行中** / **尚未开始** / **推迟**。

---

## 一、概述

**阶段目标（v2）**：围绕“用纯 Sophia 三方库实现 JSON validate / parse，并接入 agent example”这一条主线，
补齐必要但最小的语言能力，使 Sophia 能处理真实外部文本数据。

v2 不以新增 backend 或演化子系统为主；这些方向顺延到 v3+。v2 的价值判断是：如果 Sophia 已经能经 WASM
部署、能通过 `Http` / `File` 与外界交互，那么下一步应证明它能把外部 `Raw<Text>` 转为可检查的结构化语义，
而 JSON 是最小且足够真实的试金石。

**v2 完成判据**：

1. `Text` 最小解析能力（`char_at` / `slice`，可选 `starts_with`）全链路落地，并通过解释器 / WASM 差测试；
2. `while condition { ... }` 全链路落地，并通过解释器 / WASM 差测试；
3. `json` 三方库可被项目发现、检查、执行，并覆盖合法 / 非法 JSON hidden cases；
4. 至少一个 `Http.Get → Raw<Text> → ValidateJson/ParseJson → 领域 action` 的 agent-like 示例端到端通过；
5. JSON MVP 默认以纯 Sophia 库实现；host op 仅作为后续性能 / 完整 JSON 支持备选，不作为 v2 完成条件。

**起点状态**：v1 已完成 WASM codegen、解释器/WASM 差测试、库插件模型、标准库 `File` / `Http`、三方库发现与
host provider 机制。`json_lib_design.md` 已作为未落地设计草案存在，指出主要缺口为 `Text` 与循环表达力。

---

## 二、工作清单

### D0 — 设计冻结与范围收束

- [x] **D0.1 JSON MVP 子集冻结**：明确 validator 起步支持 object / array / string / int / bool / null /
      whitespace；暂缓 float / exponent / `\uXXXX` / JSON Schema。
- [x] **D0.2 Text 语义冻结**：明确索引采用 Unicode scalar 还是 byte offset；明确越界语义；明确
      `slice(start, length)` 的边界行为。
- [x] **D0.3 while 语义冻结**：明确语法、scope、condition 必须为 `Bool`、无 `break` / `continue` 的 MVP、
      不证明终止、runtime/WASM 同步循环形态。
- [x] **D0.4 JSON 返回模型冻结**：`ValidateJson` 返回 `one of { JsonValid, JsonInvalid }`；parser 阶段前
      评估递归 `JsonValue` 模型。
- [x] **D0.5 库资产草案**：写出 `json.md` prompt asset 草案，包括能力、公开 action、示例调用、错误模型。

### F1 — Text 最小解析能力

- [x] **F1.1 syntax / AST**：支持 `text.char_at(index)`、`text.slice(start, length)`；若设计确认，
      同步支持 `text.starts_with(prefix)`。
- [x] **F1.2 HIR / semantic**：校验 receiver 为 `Text`、参数为 `Int` / `Text`、返回类型为 `Text` / `Bool`；
      诊断覆盖错误 receiver、错误参数个数、错误参数类型。
- [x] **F1.3 interpreter**：实现确定性运行时语义；越界行为与 D0.2 保持一致。
- [x] **F1.4 WASM codegen**：实现与解释器等价的字符串操作 ABI / helper。
- [x] **F1.5 差测试与文档**：覆盖普通字符、空文本、越界、Unicode/byte 边界（按 D0.2 决策）、slice 组合；
      同步 `language_design.md` / 语法基线 / prompt asset。

### F2 — while 控制流

- [x] **F2.1 grammar / AST lowering**：新增 `while condition { ... }` 语句，重生成 tree-sitter parser，
      更新 CST / AST snapshot。
- [x] **F2.2 HIR scope**：while body 复用 block scope；禁止 shadowing 规则保持一致；condition 中名称解析正常。
- [x] **F2.3 semantic**：condition 必须为 `Bool`；body 的 type / effect / contract 分析并入 callable；
      flow 分析支持 `return` / `raise`，但不证明循环终止。
- [x] **F2.4 interpreter**：实现同步循环；保留运行时错误诚实传播。
- [x] **F2.5 WASM codegen**：emit loop / branch 结构，并与解释器逐 case 等价。
- [x] **F2.6 差测试与文档**：覆盖 0 次、多次、状态提前结束、嵌套 while、while 内 return/raise；
      同步 `language_design.md` / 语法基线 / prompt asset。

### L1 — JSON validator 三方库

- [x] **L1.1 fixture 布局**：新增 `stdlib/tests/fixtures/sophia_libs/json/`，包含 `library.toml`、`json.md`、
      `src/*.sophia`。
- [x] **L1.2 公开 API**：实现 `ValidateJson`，输入 `Raw<Text>`，返回 `one of { JsonValid, JsonInvalid }`。
- [x] **L1.3 内部 parser 状态**：用 `Text` + `while` 实现 cursor、空白跳过、value/object/array/string/int
      validator。
- [x] **L1.4 hidden cases**：覆盖 `{}`、`[]`、`{"ok":true}`、嵌套对象/数组、缺括号、多余逗号、未闭合字符串、
      非法 token、尾随垃圾。
- [x] **L1.5 工具链验证**：三方库 discovery、`sophia check`、interpreter run、WASM run、strip-assist
      artifact 等价均通过。
- [ ] **L1.6 LLM 生成评估**：通过 Development Graph 记录 `.pseudo → .sophia → hidden cases` 的成功/失败路径。

### L2 — parser 与结构化访问

- [ ] **L2.1 递归数据模型评估**：确认 entity 字段中的递归 union / list 是否被 checker、runtime、WASM 支持。
- [ ] **L2.2 parser MVP 决策**：若递归 `JsonValue` 可行，实现 `ParseJson`；否则先实现有限字段抽取或 flat object
      parser。
- [ ] **L2.3 结构化返回测试**：覆盖字符串、整数、布尔、null、数组、对象成员读取；明确非法 JSON 的返回或
      raise 策略。
- [ ] **L2.4 intent 边界评估**：决定 parser 返回结构是否需要携带“来自已验证 JSON”的 intent 信息。

### E1 — HTTP + JSON agent example

- [ ] **E1.1 示例目标**：定义一个真实但稳定的 agent-like 用例，例如获取 HTTP API 响应、验证 JSON、读取字段、
      做领域判断。
- [ ] **E1.2 capability / effect**：示例显式声明 `Http.Get` 及所需 capability，保持 `Raw<Text>` 到可信结构的
      intent 边界清晰。
- [ ] **E1.3 graph / e2e 路径**：验证 LLM 能在 design 阶段从库 catalog 选择 `Http` + `json`，implement 阶段
      获得对应 prompt asset 并生成可通过候选。
- [ ] **E1.4 记录结果**：记录 accept/reject 或成功/失败矩阵：LLM 是否会漏 intent 转换、是否误用 JSON API、
      hidden cases 卡在哪些语言能力上。

### 延后项

- [ ] **JSON host op**：仅作为后续性能或完整 JSON 兼容备选，不进入 v2 MVP。
- [ ] **完整 JSON Schema**：待 parser 与结构化访问稳定后再评估。
- [ ] **`break` / `continue`**：除非 JSON 库实现证明无它们会造成明显复杂度，否则不进 while MVP。
- [ ] **可选 backend / 演化能力**：native / TS / Python emit、Evolution Boundary、Semantic Identity 等顺延 v3+。

---

## 三、验证方式

每个 v2 步骤独立可合入、独立可测试，合入前必须全绿：

- 构建：`cargo build --workspace`
- 测试：`cargo test --workspace`
- 格式：`cargo fmt --all -- --check`
- Lint：`cargo clippy --workspace --all-targets -- -D warnings`

额外要求：

- `Text` / `while` 必须进入解释器与 WASM 差测试；
- JSON 库 hidden cases 必须同时覆盖合法与非法输入；
- agent example 的真实 IO 用例归入 e2e/example，不进入确定性 `cargo test` 的网络依赖路径。

---

## 四、变更记录

- 2026-06-05 — L1.1-L1.5 JSON validator 三方库落地。新增 `stdlib/tests/fixtures/sophia_libs/json/`
  纯 Sophia 源码库，公开 `ValidateJson(text: Raw<Text>) -> one of { JsonValid, JsonInvalid }`，内部用
  `Text.char_at` / `Text.slice` / `while` 实现 cursor validator，覆盖 object / array / string / int /
  bool / null / whitespace MVP；非法 JSON 作为 `JsonInvalid` 普通返回值。新增 stdlib discovery/interpreter
  hidden cases、codegen interpreter/WASM 差测试、CLI `check` / interpreter `run` / `build` / WASM `run`
  端到端测试。L1.6 LLM 生成评估仍待 Development Graph 路线执行。
- 2026-06-05 — F2 while 控制流收口。新增 `while condition { ... }` 的 grammar / AST lowering / HIR scope /
  semantic / interpreter / WASM codegen 全链路；condition 强制 `Bool`，body 复用 block scope，MVP 保持无
  `break` / `continue`、不做终止性证明。新增 syntax、semantic、runtime 与 interpreter/WASM 差测试，覆盖
  0 次、多次、状态变量提前结束、嵌套 while、while 内 `return` / `raise`；同步 `language_design.md`、
  语法基线 prompt asset 与 snapshot。验证：`cargo fmt --all -- --check`、`cargo test --workspace`、
  `cargo clippy --workspace --all-targets -- -D warnings`、`git diff --check` 通过。
- 2026-06-05 — F1.5 收口。同步 `docs/cn/language_design.md` 的 body 子语言表、`workflow/prompt/assets/
  sophia_syntax_baseline.md` 与对应 snapshot，明确 Text 原语语法和 D0.2 Unicode scalar / 越界语义。验证：
  `cargo test -p sophia-prompt syntax_baseline_preamble_is_stable`、`cargo test -p sophia-syntax
  documented_examples_parse_without_errors` 通过。
- 2026-06-05 — F1.4 WASM codegen 落地。`tools/codegen` prelude 新增 `Text.char_at` / `Text.slice` /
  `Text.starts_with` helper，按 D0.2 的 Unicode scalar index、越界空 `Text`、slice 边界夹取语义 emit；
  method-call emit 以静态类型分派 Text 方法，不走库 host import。新增 interpreter/WASM 差测试覆盖 Unicode、
  空文本、负数/越界、slice 组合和空前缀。验证：`cargo test -p sophia-codegen`、
  `cargo clippy -p sophia-codegen --all-targets -- -D warnings`、`cargo fmt --all -- --check` 通过。
- 2026-06-04 — F1.1-F1.3 首片落地。现有 method-call syntax / AST 已可表达 `text.char_at(index)`、
  `text.slice(start, length)`、`text.starts_with(prefix)`；semantic 层新增 Text receiver / arity / 参数类型 /
  返回类型校验；解释器按 D0.2 实现 Unicode scalar index、越界空文本、slice 夹取和空前缀匹配。目标测试：
  `cargo test -p sophia-semantic text_parser_methods`、`cargo test -p sophia-runtime text_` 通过。下一步是 F1.4
  WASM codegen helper 与差测试。
- 2026-06-04 — D0 设计冻结完成。JSON validator MVP 固定为 object / array / string / int / bool / null /
  whitespace 子集；Text 索引与 `.length` 保持一致，采用 Unicode scalar index，负数或越界 `char_at` 返回空
  `Text`，`slice(start, length)` 对边界做确定性夹取，负起点按 0、负长度按空片段处理；`while condition { ... }`
  使用 block scope、condition 必须为 `Bool`、MVP 无 `break` / `continue`、不做终止性证明；`ValidateJson`
  返回 `one of { JsonValid, JsonInvalid }`，parser 的递归 `JsonValue` 留到 L2 评估；新增 JSON 库 prompt asset
  草案。
- 2026-06-04 — 建立 v2 进度跟踪文档。v2 定位为围绕 JSON 三方库的端到端阶段：先补 `Text` 与 `while`
  前置语言扩展，再实现纯 Sophia JSON validator/parser，最后接入 `Http` + JSON agent-like 示例。
