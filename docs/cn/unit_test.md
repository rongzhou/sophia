# Sophia 单元测试指南（unit test）

> Sophia 三类测试的第一类。单元测试是**进 `cargo test` 门禁、确定性、可离线**的回归网，
> 验证各 crate 内部构件（解析器 / 检查器 / 解释器 / 图 / 提示词 / codegen 等）的正确性。
> 这是一份 test guide：讲清楚单元测试**测什么、怎么跑、按什么纪律组织、有哪些用例**。

---

## 一、定位

### 1.1 测什么

单元测试验证**单个 crate / 模块的构件**在隔离环境下行为正确：语法解析的 CST 形状、名称解析
的作用域、语义三层（类型 / effect / 契约）的诊断、解释器的求值、Development Graph 的事件溯源
不变量、提示词模板渲染、WASM codegen 与解释器的等价等。它是**确定性**的——同样的输入永远得到
同样的结果，故能进 `cargo test` 门禁、阻塞 CI、离线可跑。

### 1.2 不测什么

- 不测**真实 LLM 端到端闭环**——那是 e2e（`docs/e2e_test.md`）。
- 不测**与 Python 的成功率 / 耗时对比**——那是 benchmark（`docs/benchmark_test.md`）。
- 不发起真实网络 / 真实文件 IO（除测试自身的临时夹具）——非确定的外部 IO 会破坏门禁。

### 1.3 mock 政策（三类测试里唯一可 mock 的一类）

**单元测试允许 mock，且 mock 是其正当手段。** 这是三类测试的关键分界：

- **单元测试可 mock**：mock 用于**隔离尚不完整或不确定的依赖**，把被测构件单独拎出来验证。
  例：`tools/codegen` 的差测试用纯 Rust mock host（`Store<HostState>` 桥接 WASM import）隔离
  真实 IO，专注比对「解释器 oracle vs WASM 执行」；`runtime` 解释器测试用 `InMemoryHost`
  （`seed_http` / `seed_file` 预置桶）确定性执行 effect。
- **e2e / benchmark 禁 mock**：它们的目的是验证真实行为，mock 会**掩盖错误**（见各自指南）。

> 纪律提醒：mock 是「测试不完整代码」的不得已手段，不是「让测试变绿」的捷径。被 mock 的真实
> 路径仍需在 e2e / benchmark 用真实 IO 覆盖。

---

## 二、运行

```bash
cargo test --workspace                 # 全工作区单元测试（门禁口径）
cargo test -p sophia-semantic          # 单个 crate
cargo test -p sophia-runtime --test verify   # 单个测试二进制
cargo test --workspace --locked        # CI 口径（锁定依赖）
```

配套门禁（CI 同口径，见 `.github/workflows/ci.yml`）：

```bash
cargo fmt --all -- --check                          # 格式
cargo clippy --workspace --all-targets -- -D warnings   # 0 警告
cargo test --workspace                              # 全绿
```

当前基线：**359 passed / 0 failed**。snapshot 测试用 `insta`（`cargo insta review` 审阅差异）。

---

## 三、纪律

- **确定性优先**：单元测试不得依赖时钟 / 随机 / 网络 / 进程外状态；需要外部行为时用 mock 注入
  固定数据（§1.3）。
- **snapshot 守护**：CST / 语义模型指纹 / 提示词渲染等用 `insta` 快照，守护「变更不被静默引入」；
  改动模板 / 数据结构时须 `cargo insta review` 确认差异符合预期。
- **诚实判定**：测试断言真实行为，绝不为通过而弱化断言；执行硬错误判失败而非吞掉。
- **防答案泄漏（与 e2e / benchmark 共享）**：`sophia_prompt` 的防泄漏断言测试守护共享提示词资产
  （语法基线 + 标准库资产）不含任何任务领域 token（见 `workflow/prompt/tests/render.rs`）。

---

## 四、用例清单

按 crate 组织。每个 crate 列出测试二进制、用例数与考察点。

### core（语言核心：语法 → HIR → 语义）

| crate | 测试二进制 | 数 | 考察点 |
| --- | --- | --- | --- |
| `sophia-syntax` | `src/lib.rs`（单元） | 7 | tree-sitter 解析、CST→AST lowering 基础 |
| `sophia-syntax` | `tests/lowering.rs` | 17 | AST lowering：item / expr / 控制流 / 一元取负 / Text 等全形状 |
| `sophia-hir` | `tests/resolve.rs` | 19 | 名称解析、作用域、特殊根（`Http` / `File`）、effect 引用解析、禁 shadowing |
| `sophia-hir` | `tests/closure.rs` | 11 | action-rooted 语义闭包、跨 node 引用、reads/writes 收集 |
| `sophia-semantic` | `tests/analyze.rs` | 41 | 语义三层（类型 / effect / 契约）诊断 + intent 边界（`Raw<Text>` 直用拒绝）+ 模型指纹 snapshot |
| `sophia-exec-ir` | `tests/graph.rs` | 4 | Execution Graph 结构（节点 / 调用边）+ snapshot |

### runtime（解释器 = 唯一执行 oracle）

| 测试二进制 | 数 | 考察点 |
| --- | --- | --- |
| `tests/interpret.rs` | 21 | 求值：标量 / 算术 / 比较 / `if` / `match` / `let`-`set` / `return`-`raise` / 跨调用 / effect（`InMemoryHost` mock） |
| `tests/trace.rs` | 4 | Execution Trace 投影（节点 / 调用边 / 结局） |
| `tests/verify.rs` | 6 | hidden-case 执行器：返回值 / raise variant 匹配、不匹配判失败、执行硬错误判失败（绝不伪造） |

### workflow（提示词 / LLM / 引擎 / 图）

| crate | 测试二进制 | 数 | 考察点 |
| --- | --- | --- | --- |
| `sophia-prompt` | `tests/render.rs` | 18 | 模板渲染 snapshot、schema strict、**防答案泄漏断言**（基线 / 库资产无任务 token）、标准库目录 / 按需注入 |
| `sophia-llm` | `src/lib.rs` + `tests/structured.rs` | 7 + 6 | 客户端抽象、结构化输出（schema 校验 + 重试，mock client） |
| `sophia-engine` | `tests/{implement_loop,loop_steps,scheduler,select_materialize,step,traversal}.rs` | 5+6+9+8+4+7 | design / implement-loop / 调度器决策 / 目标树遍历 + 人类授权检查点 / 选择物化（mock LLM client） |
| `sophia-graph-db` | `tests/{active_context,append_only,assessment,decomposition,factory,store}.rs` | 12+2+7+6+6+17 | 事件溯源 append-only 不变量、active context 推导、binding 谓词、拆解 / 评估节点 |
| `sophia-materialize` | `tests/{gate,score}.rs` | 9 + 7 | 候选 gate（重跑 check）+ 打分 |
| `sophia-lsp` | `src/lib.rs` + `tests/analysis.rs` | 3 + 9 | LSP 诊断收集 / hover / goto（精确 span） |

### tools（确定性判定 / 检查 / codegen）

| crate | 测试二进制 | 数 | 考察点 |
| --- | --- | --- | --- |
| `sophia-audit` | `tests/audit.rs` | 7 | 纯判定层：消费注入的 verifier outcome（不执行代码） |
| `sophia-check` | `src/lib.rs` + `tests/checker.rs` | 2 + 5 | strip-assist 等价门禁、集成 check 桥接 |
| `sophia-codegen` | `tests/contract.rs` + `tests/diff.rs` | 3 + 21 | WASM 值 ABI 契约 + **差测试**（解释器 oracle vs `wasmi` 执行逐 hidden case 等价，mock host 桥接 5 个 import）+ artifact strip 字节门禁 |

### cli（协调层 + 确定性集成）

| 测试二进制 | 数 | 考察点 |
| --- | --- | --- |
| `src/lib.rs`（单元） | 16 | 协调层构件（项目布局 / 渲染 / verifier store / 参数规约） |
| `tests/pipeline.rs` | 22 | 确定性 CLI 管线：init → check → build → run / smoke（无 LLM、无真实外部 IO） |
| `tests/intent_matrix.rs` | 3 | **intent accept/reject 矩阵**（确定性）：Sophia 静态拒绝把 `Http.Get` 的 `Raw<Text>` 不经转换直接当 `Sanitized<Text>` 用的候选（`CHECK-INTENT-001`）+ 接受经 `intent_conversion` 的安全候选；TS 接受半以文档矩阵呈现（不引入 tsc 门禁） |

> `intent_matrix.rs` 是**确定性 code_check 矩阵**（对固定程序的检查器裁定，不需 LLM / 网络），
> 故是单元测试。它与 e2e 的网络获取用例（G2-03）互补：reject 半（静态拒绝）在此确定性钉死，
> accept 半（真实取数据跑通）在 e2e 用真实 IO 验证。

---

## 五、工程结构

- 单元测试就近放在各 crate 的 `tests/`（集成测试）或 `src/` 内联 `#[cfg(test)]`（私有单元）。
- snapshot 落各 crate `tests/snapshots/*.snap`（`insta`）。
- mock 夹具就近定义在使用它的测试 crate 内（如 `tools/codegen/tests/diff.rs` 的 WASM mock host、
  `runtime` 的 `InMemoryHost`）；不抽公共 mock 库（YAGNI，避免过度耦合）。
- 全部进 `cargo test --workspace` 门禁，CI `check` job 用 `--locked` 跑。
