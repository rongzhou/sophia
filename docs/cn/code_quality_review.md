# 项目代码质量审核记录

本记录按“自底向上”的模块顺序推进，优先记录会影响行为正确性、工程边界、测试覆盖与后续演进的事项。审核默认不修改实现，除非后续明确进入修复阶段。

## 审核原则：信任边界与契约闭合

本记录采用“系统外不可信、系统内靠契约闭合”的判断标准：

- 系统外输入必须校验：用户源码、CLI 参数、LSP 文档、LLM 输出、HTTP/文件 IO、SQLite 事件日志、artifact 文件、环境变量、三方库 manifest 和 WASM host/provider 返回值。
- 模块间如果上游已经通过强契约完成验证，例如类型态、checked constructor、私有构造器、一次事务提交、只读语义模型或明确的 `CheckPassed` 状态，下游不必重复完整校验；可保留轻量断言、清晰诊断和 contract tests。
- public API 若可被外部直接调用，应按边界入口处理；若只是内部 fast path，应通过可见性、命名或文档标明 `prechecked` / `unchecked` 前置条件，避免把“不重复校验”误读成“可接收任意输入”。
- 持久化 replay、跨进程加载、文件系统、LLM/provider、CLI/LSP 入口都视为重新进入系统，不能只信内存中曾经完成过的校验。

## 2026-06-04 — core 模块

审核范围：

- `core/syntax`
- `core/library`
- `core/hir`
- `core/semantic`
- `core/exec-ir`

验证命令：

```bash
cargo test -p sophia-syntax -p sophia-hir -p sophia-library -p sophia-semantic -p sophia-exec-ir
cargo clippy -p sophia-syntax -p sophia-hir -p sophia-library -p sophia-semantic -p sophia-exec-ir -- -D warnings
```

结果：全部通过。

总体判断：

- 分层边界清晰：`syntax -> hir/library -> semantic -> exec-ir` 的依赖方向没有明显倒挂。
- `syntax` 保持 CST 到 AST 的职责边界，`hir` 负责 index/scope，`semantic` 拆分为 model/type/effect/contract，`exec-ir` 当前保持小而确定。
- core 层整体遵守零 IO 纪律，确定性输出使用 `BTreeMap`/快照测试的习惯较一致。

发现：

1. 高优先级：callable 调用缺少参数个数检查。（已修复）
   - 位置：`core/semantic/src/type_layer.rs` 的 `infer_call`。
   - 现象：只按实际传入的 `args` 检查已有实参类型；少传参数不会报错，多传参数会被忽略。`to_text` 也只检查第一个参数。
   - 风险：错误程序可能通过 semantic 检查，并在 runtime/codegen 阶段表现为错误值、缺变量或不可预期行为。
   - 修复：`infer_call` 对普通 callable 与 `to_text` 统一检查 arity；少参、多参、`to_text` 多参均有回归测试。

2. 高优先级：error variant 构造字段没有语义校验。（已修复）
   - 位置：`core/semantic/src/type_layer.rs` 的 `Expr::Construct` 分支、`check_construct`；`core/semantic/src/model.rs` 已记录 variant 字段但未被使用。
   - 现象：`raise Bad { ghost = 1 }`、缺少必填字段、字段类型错误等可能只检查到 variant 名是否声明，而不会检查构造内容。
   - 风险：error 契约的结构完整性被绕过，后续解释器或宿主集成可能接收到不符合声明的错误值。
   - 修复：variant 构造纳入 record construct 检查，覆盖缺字段、未知字段、字段类型不匹配；entity 与 variant 复用同一字段规则。

3. 中优先级：用户 `effect` 声明可能覆盖内置/库 effect op。（已修复）
   - 位置：`core/hir/src/index.rs`。`AsgIndex::new` 先注入 builtin/library ops，随后用户 `Item::Effect` 直接 `insert`。
   - 现象：用户声明 `effect Console` 或与库 op 同名时，arity/builtin 信息可能被改写。
   - 风险：核心内置 effect 与库 effect 的边界被用户声明破坏，capability/effect 检查结果可能漂移。
   - 建议：把与 builtin/library effect op 的冲突作为 HIR hard error 或 diagnostic；补覆盖内置、库 op 冲突的测试。

4. 中低优先级：`LibraryRegistry` 未校验 manifest 的 `[surface].sophia_sources` 与传入 `LibraryContent.sophia_sources` 是否一致。
   - 位置：`core/library/src/registry.rs`。
   - 现象：只要 manifest 或 content 任一侧非空，就登记调用方传入的 Sophia sources；manifest 中声明的路径列表没有被核对。
   - 风险：“清单是单一真相源”的约束被削弱，后续 build bundle/registry fingerprint 也更依赖调用方正确组装。
   - 建议：registry 层校验 manifest 路径集合与 `LibraryContent` 路径集合一致，缺失/多余均报错。

建议修复顺序：

1. 已完成 callable arity 检查与 error variant 构造字段检查。
2. 下一步修复 effect op 冲突检查。
3. 收紧 library surface source 一致性校验。

## 2026-06-04 — runtime 模块

审核范围：

- `runtime/src/interp.rs`
- `runtime/src/value.rs`
- `runtime/src/validate.rs`
- `runtime/src/host.rs`
- `runtime/src/value_wire.rs`
- `runtime/src/wasm_host.rs`
- `runtime/src/wasm_program.rs`
- `runtime/src/trace.rs`
- `runtime/src/verify.rs`

验证命令：

```bash
cargo test -p sophia-runtime
cargo clippy -p sophia-runtime -- -D warnings
```

结果：全部通过。

总体判断：

- runtime 主线比较稳：解释器入口/输出有运行期校验，跨 callable 调用经 `ExecGraph` 验证，host 失败遵守诚实错误，不伪造成功。
- `Value` / `validate` / `trace` 的职责清晰，确定性 trace 不引入墙钟时间，符合工程笔记里“可复现优先”的习惯。
- WASM host/provider 和 program runner 的错误处理大体是硬失败路径，适合作为 sandbox/host 边界。

发现：

1. 中高优先级：解释器对未注册库 host 的诊断路径不够诚实直达。（已修复）
   - 位置：`runtime/src/interp.rs` 的 `try_effect_op`。
   - 现象：旧实现中只有 `(family, op)` 已注册到 `HostRegistry` 时才把 `Lib.Op(args)` 识别为库调用；如果程序通过语义检查但调用方漏注册 host，解释器会退回普通 method path，最终多半报“未绑定变量 `Lib`”，而不是 `HostRegistry::call` 已设计好的“无 host 实现”。
   - 风险：失败仍是硬错误，但诊断偏离真实原因，排查三方库/标准库 host 注入问题时会误导。
   - 修复：`SemanticModel` 现在保存从 `AsgIndex` 冻结下来的 `library_ops` 契约；解释器用语义模型判断 `Lib.Op` 是否为已知库 op，再统一委派 `HostRegistry::call`。调用方漏注册 host 时会直达“无 host 实现：`Family.Op`”诊断，不再退回普通 method path。已补 `Vault.Read` 语义已知但 host 未注册的回归测试，并更新 semantic model fingerprint 快照。

2. 中优先级：transition 构造式字段契约尚未在 semantic 闭合，runtime 会静默修正坏输入。（已修复）
   - 位置：`runtime/src/interp.rs` 的 `eval_construct`。
   - 现象：transition 调用按 input 顺序从字段 map 重排，缺字段用 `Value::Unit` 代替，多余字段直接丢弃。
   - 风险：semantic 层当前也没有完整检查 transition 构造式字段，因此错误程序会延后到被调用方 input validation，诊断位置和语义都变差；如果缺字段恰好期望 `Unit`，还可能误通过。
   - 建议：优先在 semantic 层补静态字段检查。semantic 未闭合前，runtime 可临时 hard error，避免静默补 `Unit`；semantic 修复后，runtime 不必长期复制完整字段规则，可降为轻量契约断言或 `InvalidInput` 诊断。

3. 中优先级：error variant 构造字段契约尚未在 semantic 闭合，runtime 会放大问题。（已修复）
   - 位置：`runtime/src/interp.rs` 的 `eval_raise` 与 `eval_construct`。
   - 现象：`raise Variant { ... }` 与返回 `ErrorValue` 时直接收集字段，不按 `SemanticModel::variants` 校验缺字段、未知字段、字段类型。
   - 风险：这会放大 core semantic 中“variant 构造未检查”的问题；错误可能在顶层 `Outcome::Raised` 中以结构不完整的领域错误出现。
   - 建议：优先在 semantic 层修复 variant 构造。runtime 侧只在外部/动态构造入口做轻量契约断言或统一 outcome validate，不维护第二套完整 variant 类型系统。

4. 中低优先级：WASM runner 写入入参内存前缺少与 provider 侧一致的长度/负指针防御。
   - 位置：`runtime/src/wasm_program.rs` 的 `write_bytes`。
   - 现象：`bytes.len() as i32` 未检查溢出，`sophia_alloc` 返回负指针后会转成巨大 `usize` 交给 `memory.write`。
   - 风险：通常仍会以 wasmi 内存错误失败，但诊断不如 `WasmHostFn` provider 侧明确；边界代码风格不一致。
   - 建议：复用类似 `checked_i32_len` 的长度检查，并显式拒绝负指针。

建议修复顺序：

1. core semantic 的 callable/variant 静态检查已完成；继续补 transition 构造静态检查。
2. runtime host 缺失诊断已修复；继续补字段集合契约边界：semantic 修复前可临时 hard error，修复后降为轻量断言/测试。
3. 补 WASM runner `write_bytes` 的长度和负指针检查。

## 2026-06-04 — stdlib 模块

审核范围：

- `stdlib/src/lib.rs`
- `stdlib/src/discover.rs`
- `stdlib/src/native_host.rs`
- `stdlib/libs/file/library.toml`
- `stdlib/libs/http/library.toml`
- `stdlib/tests/assets.rs`
- `stdlib/tests/library_demo.rs`

验证命令：

```bash
cargo test -p sophia-stdlib
cargo clippy -p sophia-stdlib -- -D warnings
```

结果：全部通过。

总体判断：

- 标准库内容层职责清楚：标准库静态 include，三方库启动时发现，host 注册与 runtime 的 `HostRegistry` 对接。
- 三方发现路径按目录排序，发现失败/清单失败/registry 冲突均 hard error，符合“诚实失败、不静默跳过”的工程纪律。
- 测试覆盖了资产防泄漏、catalog/preamble 确定性、mock host 诚实失败、三方纯 Sophia 库、三方 WASM host 与两库等价。

发现：

1. 高优先级：真实 `File.Read` / `File.Write` host 没有项目根或策略边界。（已修复）
   - 位置：`stdlib/src/native_host.rs` 的 `register_native_hosts`。
   - 现象：`File.Read(path)` 和 `File.Write(path, content)` 直接把 Sophia 程序传入的 `Text` 当作进程文件路径执行真实 IO。
   - 风险：capability 目前只授权 `File.Read` / `File.Write` 这个 op，不限制路径范围；一旦用户程序获得 File capability，就可能读写项目根外路径。作为 CLI 真实执行边界，这比 mock/测试路径更敏感。
   - 修复：`register_native_hosts(&mut host, project_root) -> Result<(), String>` 成为唯一 native 注册入口；文件 host 只接受相对路径，拒绝绝对路径和 `..`，读取/写入前用真实路径确认仍在项目根内；CLI 解释器后端、WASM 后端与 e2e harness 均走同一注册入口。

2. 中优先级：`SOPHIA_LIB_PATH` 用硬编码冒号分隔。（已修复）
   - 位置：`stdlib/src/discover.rs` 的 `project_roots`。
   - 现象：环境变量路径用 `path_var.split(':')` 解析。
   - 风险：Unix 正常，但不符合 Rust 跨平台最佳实践；Windows 路径与平台分隔符会出问题。
   - 修复：改用 `std::env::split_paths` / `std::env::var_os` 按平台 path-list 语义解析，并补充 `join_paths` 回归测试；中英文 stdlib design 文档同步为平台语义描述。

3. 中低优先级：`register_native_hosts` 内部 `Client::builder().build().expect(...)` 可能 panic。（已修复）
   - 位置：`stdlib/src/native_host.rs`。
   - 现象：HTTP client 构建失败时 panic，而不是返回启动期 `Result`。
   - 风险：实际概率低，但与 stdlib 其它“启动期诚实 Err”的风格不一致。
   - 修复：没有保留兼容封装；`register_native_hosts` 直接返回 `Result`，HTTP client 构建失败作为启动期错误返回。

4. 中低优先级：三方 WASM demo 测试会动态写 fixture `host.wasm` 到工作区。（已修复）
   - 位置：`stdlib/tests/library_demo.rs` 的 `ensure_host_wasm`。
   - 现象：测试用 `wasm-encoder` 生成 wasm 并原子写入 `stdlib/tests/fixtures/.../host.wasm`。
   - 风险：已经用 temp + rename 避免并发半写，但测试仍会修改工作区 fixture；对只读工作树、打包校验、审计 dirty diff 不够友好。
   - 修复：保留真实目录发现测试，但仓库 fixture 仅作为只读模板；每个测试复制到临时三方根，在临时 `hash_wasm` 目录生成/覆盖 `host.wasm`，不再修改工作区。

建议修复顺序：

1. 已完成 native File host 项目根 sandbox，并将 native host 注册改为可返回错误的启动期 API。
2. 已用 `std::env::split_paths` 修正 `SOPHIA_LIB_PATH`。
3. 已将三方 WASM demo fixture 写入迁移到临时目录。

## 2026-06-04 — tools/check 模块

审核范围：

- `tools/check/src/lib.rs`
- `tools/check/src/strip_assist.rs`
- `tools/check/tests/checker.rs`
- `workflow/engine/src/code_check.rs` 中对 `sophia_check::check_program` 的调用前置

验证命令：

```bash
cargo test -p sophia-check
cargo clippy -p sophia-check -- -D warnings
```

结果：全部通过。

总体判断：

- `tools/check` 作为确定性静态门禁的组装层，职责集中：HIR、semantic、strip-assist 等价检查在一个报告里输出。
- workflow 的 `code_check` 主路径会先做语法诊断，语法干净后才调用 `check_program`，与当前前置假设一致。
- strip-assist 门禁把声明模型指纹和语义诊断纳入比对，比只比声明模型更强。

发现：

1. 中优先级：`check_program` 和 strip-assist 内部解析使用 `expect("parse")`。（已修复）
   - 位置：`tools/check/src/lib.rs`、`tools/check/src/strip_assist.rs`。
   - 现象：公共函数接收源码字符串，但解析失败会 panic；文档写了“调用方应已过滤语法错误”。
   - 风险：workflow 当前主路径做了前置过滤，风险受控；但作为 crate 公共 API，未来 CLI/LSP/测试若直接调用，会把用户输入错误升级成 panic。
   - 修复：新增 `CheckError::Syntax` 和共享 `parse_checked`；`check_program` 与 strip-assist 的 `parse_all` 均返回结构化语法错误，不再 panic。workflow 仍保留前置语法诊断，用于生成更细的逐文件 `CodeCheck` 诊断。

2. 中优先级：`check_program` 固定使用 `standard_registry`，不适合作为带三方库项目的完整检查 API。（已修复）
   - 位置：`tools/check/src/lib.rs`。
   - 现象：函数内部构建 `sophia_stdlib::standard_registry()`；CLI 生产路径另行使用 full registry 后调用 `check_strip_assist_equivalence`，但 `check_program` 本身无法接收项目 registry。
   - 风险：调用方若误以为 `check_program` 等价于项目完整 check，会漏掉三方库纯 Sophia 源码与三方 op 契约。
   - 修复：保留 `check_program` 作为标准库-only helper；新增 `check_program_with_registry(sources, registry)`，调用方可传项目启动期发现的完整 registry。两者共用同一内部检查路径，避免标准库路径与项目路径分叉，并补充纯 Sophia 库源码并入回归测试。

3. 中低优先级：strip-assist 指纹只用用户 AST，不把库 AST 纳入 `ir_fingerprint`。
   - 位置：`tools/check/src/strip_assist.rs`。
   - 现象：stripped 侧构建 index 时并入库源码，但指纹/语义分析只传用户 AST。
   - 风险：对于依赖纯 Sophia 三方库的项目，strip-assist 门禁比完整 semantic check 的上下文弱；当前标准库无 Sophia 源码，因此标准路径不受影响。
   - 建议：若新增 `check_program_with_registry`，strip-assist 比对应与主 check 使用同一批 `user AST + library AST`，或明确只验证用户源码 assist 与用户声明 IR。

建议修复顺序：

1. 让 `check_program` / strip-assist 解析失败返回结构化 `CheckError`。
2. 新增 registry-aware 的 `check_program_with_registry`。
3. 统一 strip-assist 与主 check 的 AST 上下文，覆盖纯 Sophia 三方库测试。

## 2026-06-04 — tools/audit 与 tools/materialize 模块

审核范围：

- `tools/audit/src/lib.rs`
- `tools/audit/tests/audit.rs`
- `tools/materialize/src/gate.rs`
- `tools/materialize/src/write.rs`
- `tools/materialize/src/score.rs`
- `tools/materialize/tests/gate.rs`
- `tools/materialize/tests/score.rs`

验证命令：

```bash
cargo test -p sophia-audit -p sophia-materialize
cargo clippy -p sophia-audit -p sophia-materialize -- -D warnings
```

结果：全部通过。

总体判断：

- `tools/audit` 很小，职责清晰：只消费 verifier 结果，不执行 verifier，缺少可执行 verifier 结果时 hard error。
- `tools/materialize` 的类型状态链清楚，`Unchecked -> CheckPassed -> AuditPassed -> RuntimeValidated -> Selected` 能在编译期阻止跳过 gate。
- 写入路径有基本逃逸防护：拒绝绝对路径与 `..`。
- score 排序确定性较好，有 compile fail 封顶和平局按原始下标稳定排序。

发现：

1. 中优先级：`Forbidden` 约束当前不会驱动 gate。（已修复）
   - 位置：`tools/audit/src/lib.rs` 的 `audit_one`。
   - 现象：除 `Invariant` 外，其它 constraint kind 全部 `Skipped`；包括语义上听起来很强的 `Forbidden`。
   - 风险：调用方可能误以为 `Forbidden` 会阻断 materialize，但实际只作上下文。若没有其它 verifier 或静态规则兜底，禁止行为不会被 audit gate 拦住。
   - 修复：`tools/audit` 将 `Invariant` / `Forbidden` + 可执行 verifier（HiddenCase / AuditRule）统一纳入 gate；缺少执行结果仍 hard error。CLI materialize gate 的 hidden verifier 执行同步扩展到 Forbidden，避免 audit 等不到 outcome。无 verifier 或 Manual 的 Forbidden 仍只作上下文，不伪造判断。

2. 中优先级：`atomic_write_all` 是逐文件 rename，不是多文件事务原子。（已修复）
   - 位置：`tools/materialize/src/write.rs`。
   - 现象：先写 staging，再对每个文件逐个 `rename` 到最终路径；如果第 N 个 rename 失败，前 N-1 个文件已经替换。
   - 风险：注释写“不触碰已有目标文件”与“文件集合原子写入”容易过度承诺；实际只能保证 staging 写入阶段失败不触碰目标，rename 阶段不是全有或全无。
   - 修复：修正 `tools/materialize` 注释和中英文 `language_design` 表述为“staging 阶段失败不触碰目标；单文件替换原子，集合非事务”。没有引入备份回滚或版本目录，避免在未设计完整提交协议前制造伪事务。

3. 中低优先级：`.sophia-staging` 固定目录会让并发 materialize 互相干扰。
   - 位置：`tools/materialize/src/write.rs`。
   - 现象：每次写入都删除 `target_root/.sophia-staging`。
   - 风险：两个进程/线程同时向同一 target root materialize 时，一个任务可能删除另一个任务的 staging。
   - 建议：staging 目录加唯一后缀（pid + nonce），最终清理自身目录；必要时再加 target root 级 lock。

4. 中低优先级：`capability_minimality` 使用文本计数，和真实语法/语义不完全一致。
   - 位置：`tools/materialize/src/score.rs`。
   - 现象：通过 `content.matches("effects {")` 与 `content.matches("capability:")` 计数。
   - 风险：作为排序弱信号可以接受，但格式变化、注释、字符串字面量都可能影响分数；不应被误认为语义级权限最小化证明。
   - 建议：若排序权重提高，改为基于 AST/semantic model 统计 declared effects 与 capability binding；当前保留时需在文档中标明是启发式弱信号。

建议修复顺序：

1. 明确或调整 `Forbidden` 约束的 gate 语义。
2. 已修正 materialize 原子性文档：单文件替换原子，集合非事务。
3. 为 staging 目录加唯一后缀并考虑并发 lock。
4. 将 capability minimality 统计升级为 AST/semantic 统计。

## 2026-06-04 — tools/codegen 模块

审核范围：

- `tools/codegen/src/lib.rs`
- `tools/codegen/src/build.rs`
- `tools/codegen/src/error.rs`
- `tools/codegen/src/abi.rs`
- `tools/codegen/src/contract.rs`
- `tools/codegen/src/emit.rs`
- `tools/codegen/tests/contract.rs`
- `tools/codegen/tests/diff.rs`

验证命令：

```bash
cargo test -p sophia-codegen
cargo clippy -p sophia-codegen -- -D warnings
```

结果：全部通过。

总体判断：

- `tools/codegen` 的核心边界清楚：只消费已检查 IR/semantic model，emit 确定性 WASM bytes，不做 IO，不引入第二语义真相源。
- 差分测试质量较高，已覆盖算术、布尔、跨 callable 调用、error algebra、one-of、match、entity、state、Text、repeat、console effect、Http/File mock、三方动态 import 与三方 `ValueWire` provider。
- 主要风险不在当前 happy path，而在“已检查输入”的前置条件没有通过 API 命名、类型态或可见性显式化时，public convenience API 可能被误当成 raw-source 边界入口。

发现：

1. 高优先级：`repeat` 计数从 `i64` 收窄到 `i32`，大整数会与解释器语义漂移。（已修复）
   - 位置：`tools/codegen/src/emit.rs` 的 `Stmt::Repeat` 分支。
   - 现象：count 经 `GET_INT` 后立即 `I32WrapI64`，再用 `i32` 局部倒计数；解释器侧语义是 `i64` 的 `n.max(0)` 次。
   - 风险：例如 `4294967296` 会在 WASM 侧 wrap 为 `0`，循环体不执行；解释器侧会进入循环。若循环体首轮 `return`，可形成可观察返回值差异，且不需要真实跑完超大循环。
   - 修复：WASM repeat counter 改为独立 `i64` local，循环判断/递减使用 `I64LeS` / `I64Sub`；保留原 i32 scratch 给 ValueWire/Text 拷贝游标。新增 `repeat 4294967296 times { return ... }` 差分回归，避免大整数 wrap 后跳过首轮。

2. 中优先级：`emit_from_sources` 忽略 HIR diagnostics，也不重新执行 semantic diagnostics。（已修复）
   - 位置：`tools/codegen/src/build.rs`。
   - 现象：`resolve_program` 返回的 diagnostics 被绑定为 `_diags` 后忽略；随后直接 `SemanticModel::build` 并 `emit_module`。函数文档写明前提是源码已通过 `sophia-check`。
   - 风险：CLI artifact strip gate 当前可以由上层 check 兜底，但 `emit_from_sources` 是 public convenience API，直接接收源码和 registry；未来调用方绕过 check 时，无效程序可能进入 emit，错误位置也会变晚。
   - 建议：把边界说清：若该函数继续 public 且接收 raw sources，应作为 checked 入口拒绝非空 HIR/semantic diagnostics；若仅供已 check 管线调用，应改名为 `emit_prechecked_from_sources` 或收窄可见性，并在文档标明不重复 semantic check。
   - 修复：`emit_from_sources` 作为 raw-source public API 重新拒绝非空 HIR diagnostics，并改用 `analyze_program` 收集 semantic diagnostics；任一诊断返回 `CodegenError::InvalidInput`。已补 HIR / semantic 诊断拒绝回归测试。

3. 中优先级：callable 调用 arity 依赖上游语义检查，但 codegen 契约没有显式化。（已修复）
   - 位置：`tools/codegen/src/emit.rs` 的 `emit_call`。
   - 现象：函数只确认 callee 存在且是 callable，然后逐个 emit 已提供实参并直接 `Call(idx)`，没有比较 `args.len()` 与 callee input 数量。
   - 风险：若上游 arity 漏检或 public API 被直接喂入坏输入，codegen 可能生成栈签名不匹配的无效 WASM，或把错误推迟到 module validation/runner 阶段。
   - 建议：先修 core callable arity。codegen 侧可在过渡期返回 `CodegenError::InvalidInput` 或使用 `debug_assert` 暴露契约破坏；长期不必复制完整 arity/type checker，只需依赖 `CodegenInput` prechecked 契约并补 contract/diff tests。
   - 修复：`emit_call` 在实参求值前显式比较 AST call args 与 `SemanticModel.callables[callee].inputs`，不匹配时返回 `CodegenError::InvalidInput`。已补篡改 prechecked 模型签名的契约回归测试，防止生成栈签名不匹配的 WASM。

4. 中低优先级：模块文档和测试注释仍停留在 W2 早期阶段，低估了当前实现能力。
   - 位置：`tools/codegen/src/lib.rs`、`tools/codegen/src/abi.rs`、`tools/codegen/tests/diff.rs` 的顶部注释。
   - 现象：文档仍写 `match` / `repeat` / `raise` / `print` / Text / Entity / State / effect 未覆盖，但当前实现和差分测试已经覆盖其中多数能力。
   - 风险：维护者可能按过期阶段说明做决策，误判哪些 NotYetImplemented 是真实缺口，哪些已经落地。
   - 建议：把阶段说明改成“当前已覆盖能力 + 已知未覆盖能力”列表；`to_text` 等仍未实现项单独列为已知缺口。

5. 低优先级：`to_text` 的 NotYetImplemented 文案已经不准确。
   - 位置：`tools/codegen/src/emit.rs` 的 `emit_call`。
   - 现象：`to_text` 返回 `to_text（Text 待后续增量）`，但 Text 值、拼接、长度、相等已经被 WASM 路径支持。
   - 风险：错误信息会误导排查，以为整体 Text 尚未支持。
   - 建议：更新为“`to_text` 内建转换尚未支持”，并在 wasm codegen 已知缺口中列明。

建议修复顺序：

1. 已完成 `repeat` 的 `i64 -> i32` 语义漂移修复与差分边界测试。
2. 已完成 codegen callable arity 契约防线。
3. 明确 `emit_from_sources` 是 checked 边界还是 prechecked fast path，并据此调整 API。
4. 更新 codegen 阶段文档、ABI 注释、diff 测试注释与 `to_text` 错误文案。

## 2026-06-04 — workflow/graph-db 模块

审核范围：

- `workflow/graph-db/src/lib.rs`
- `workflow/graph-db/src/store.rs`
- `workflow/graph-db/src/event.rs`
- `workflow/graph-db/src/edge.rs`
- `workflow/graph-db/src/ids.rs`
- `workflow/graph-db/src/meta.rs`
- `workflow/graph-db/src/payload.rs`
- `workflow/graph-db/src/factory.rs`
- `workflow/graph-db/src/active_context.rs`
- `workflow/graph-db/src/assessment.rs`
- `workflow/graph-db/src/decomposition.rs`
- `workflow/graph-db/tests/*.rs`

验证命令：

```bash
cargo test -p sophia-graph-db
cargo clippy -p sophia-graph-db -- -D warnings
```

结果：全部通过。

总体判断：

- `graph-db` 的 crate 边界清楚，SQLite 事件日志 + 内存物化视图的实现简单、可审计。
- 节点创建经 `HumanFactory` / `LlmFactory` / `DeterministicFactory` 分流，能较好地把 provenance 从调用路径上固定住。
- 单条节点/边 append 的不变量覆盖较扎实，测试覆盖了 role/provenance 矩阵、append-only 前缀稳定、supersedes 无环、I6、active context 继承、assessment/decomposition helper 等关键路径。
- 主要风险集中在多事件 helper 的原子性、文件库多进程并发、以及事件日志 replay 时没有重新验证不变量。

发现：

1. 高优先级：多事件 helper 非事务化，失败会留下半成品图。（已修复）
   - 位置：`workflow/graph-db/src/assessment.rs` 的 `decompose_assessment`，以及 `workflow/graph-db/src/decomposition.rs` 的 `build_decomposition`。
   - 现象：`decompose_assessment` 在创建 `Assessment` 并追加 `consumed` / `assesses` 边后，才逐条检查 `proposed_invariants`；若后续发现非 `Invariant`，函数返回错误，但前面事件已经写入。`build_decomposition` 也会先写 `Decomposition` 与边，再逐个创建子目标；后续子目标 payload 校验失败时会留下部分节点/边。
   - 风险：调用方看到 `Err` 可能认为本次拆解没有落图，实际事件日志已追加半成品；active context、I6 校验或后续 workflow 可能读到意外节点。
   - 修复：`decompose_assessment` / `build_decomposition` 在写任何事件前完整预校验 role、self-check、payload 必填字段、Invariant kind 与 verifier ref；失败路径不触碰事件日志。新增无效 first slice、非 invariant、无效子目标的 `raw_event_log` 不变回归测试。

2. 高优先级：文件库多进程/多 store 并发会重复分配 `NodeId`。（已修复）
   - 位置：`workflow/graph-db/src/store.rs` 的 `next_id` 内存字段与 `append_node`。
   - 现象：`next_id` 只由打开时 replay 得到；事件表只有 `seq` 和 JSON payload，没有节点 ID 唯一索引。两个 `GraphStore::open(path)` 实例若同时基于同一日志分配，都可能写出相同 `NodeId`。
   - 风险：replay 时 `nodes.insert(id, ...)` 会以后写节点覆盖内存视图中的同 ID 节点，但旧事件仍在日志里，导致 raw log 与 materialized view 语义分裂。
   - 修复：新增内部 `graph_node_ids(id INTEGER PRIMARY KEY)` 投影表；`append_node` 在 SQLite `BEGIN IMMEDIATE` 事务中分配唯一 NodeId、写投影表并追加 `NodeCreated` 事件，同事务提交。打开旧库时从事件 replay 同步投影表。新增两个已打开 store 交替写入仍分配不同 NodeId 的回归测试。

3. 中优先级：事件 replay 只反序列化并 apply，不重放不变量校验。（已修复）
   - 位置：`workflow/graph-db/src/store.rs` 的 `replay` / `apply_in_memory`。
   - 现象：replay 对历史事件调用 `serde_json::from_str` 后直接 `apply_in_memory`；不会检查重复 node id、role/payload 一致性、payload 字段约束、边悬空、edge role 矩阵、supersedes 环等。
   - 风险：一旦数据库来自旧版本、手工编辑、并发写坏或迁移 bug，`GraphStore::open` 可能成功打开一个不满足 crate 文档不变量的图。
   - 建议：把 replay 改成 checked replay：按事件顺序用与 append 相同的校验逻辑构建视图，并显式拒绝重复 NodeId、非法边和坏 payload；必要时提供 `open_unchecked_for_recovery` 作为修复工具入口。
   - 修复：`GraphStore::replay` 在 apply 前按事件顺序重放节点契约和边契约校验，拒绝重复 / 非法 NodeId、role/payload/provenance/status 不一致、坏 payload、悬空边、非法 edge kind、payload 级边约束和 supersedes 约束。已补重复 NodeId 与悬空边损坏日志的打开失败测试。

4. 中优先级：`ContextSnapshot.digest` 只校验格式，不校验与 `snapshot` 内容一致。（已修复）
   - 位置：`workflow/graph-db/src/store.rs` 的 `validate_payload`。
   - 现象：`ContextSnapshotPayload` 的 digest 只要求 64 位小写 hex；`snapshot_payload` helper 会正确生成 digest，但 public deterministic factory 仍可接收任意 `snapshot + digest` 组合。
   - 风险：context snapshot 是 I10/可复现性的锚点；digest 与内容不一致时，后续 consumed 边看似有锚，实际无法证明 LLM 输入上下文未被篡改或串用。
   - 建议：在 `validate_payload` 中重新对 canonical snapshot JSON 计算 SHA-256 并比较 digest；或把 `ContextSnapshotPayload` 构造函数收敛为 `from_active_context`，字段私有化，避免调用方手填 digest。
   - 修复：`validate_payload` 对 `ContextSnapshotPayload.snapshot` 重算 digest 并与 payload digest 比较；真实 ActiveContext snapshot 复用 active context digest body，普通 JSON snapshot 用稳定 JSON 串计算。已补 digest 与内容不一致拒绝测试，并更新测试 helper 使用 `{}` 的真实 SHA-256。

5. 中低优先级：`NodeId::parse` 接受非规范字符串和 `N0000`。
   - 位置：`workflow/graph-db/src/ids.rs`。
   - 现象：注释声明规范形式为 `N{>=4 位零填充十进制}`，但解析逻辑只检查 `N` 前缀后可解析为 `u32`，因此 `N1`、`N01`、`N0000` 都可通过。
   - 风险：正常序列化路径不会产生这些值，但外部 JSON/import/recovery 数据会进入非规范 ID，影响审计可读性和潜在唯一性假设。
   - 建议：解析时要求 `N` 后至少 4 位、全数字、值大于 0，并校验 `s == NodeId(value).as_string()`；补反序列化拒绝非规范 ID 的测试。

建议修复顺序：

1. 已完成 `decompose_assessment` / `build_decomposition` 预校验，失败无副作用。
2. 已用 SQLite immediate transaction + 唯一投影表解决多 store NodeId 重复分配。
3. 已将 replay 升级为 checked replay，拒绝坏历史事件。
4. 已校验 `ContextSnapshot.digest` 与 snapshot 内容一致。
5. 收紧 `NodeId::parse` 的规范性。

## 2026-06-04 — workflow/llm 模块

审核范围：

- `workflow/llm/src/lib.rs`
- `workflow/llm/src/client.rs`
- `workflow/llm/src/backend.rs`
- `workflow/llm/src/structured.rs`
- `workflow/llm/tests/structured.rs`

验证命令：

```bash
cargo test -p sophia-llm
cargo clippy -p sophia-llm -- -D warnings
```

结果：全部通过。

总体判断：

- `workflow/llm` 的抽象边界很清楚：后端只提供自由文本 `complete`，结构化 JSON 提取、schema 验证和重试统一放在 `complete_structured`。
- 测试覆盖了 endpoint 拼接、message 组装、OpenAI/Ollama stream 解析、schema strict、重试、后端不可用直返等关键 happy path 与部分失败路径。
- 当前实现适合作为本地/开发期轻量 LLM adapter；若作为长期运行的生产边界，还需要补响应大小、真正流式读取、错误分类和 provider-native 结构化能力等护栏。

发现：

1. 中高优先级：HTTP backend 声称使用 streaming，但实际先把完整响应读入内存。（已修复）
   - 位置：`workflow/llm/src/backend.rs` 的 `complete`。
   - 现象：请求体设置 `stream: true`，模块注释也写“都用 streaming 响应”；但实现使用 `resp.text().await` 聚合完整 body 后再解析。
   - 风险：无法在收到 `[DONE]` / `done=true` 时尽早停止，也没有增量处理或 backpressure；异常后端或代理返回超大 body 时会带来内存风险。`read_timeout` 只能限制读取空闲时间，不能限制总响应大小。
   - 建议：改用 `bytes_stream` / `chunk` 增量解析 SSE/NDJSON，遇到结束标记立即返回；同时增加最大响应字节数配置，超过上限返回 `BackendUnavailable` 或专门错误。
   - 修复：HTTP backend 已改为基于 `reqwest::Response::chunk()` 的增量行解析，OpenAI SSE 收到 `[DONE]`、Ollama NDJSON 收到 `done=true` 即返回；保留 UTF-8 split chunk 缓冲。已补 OpenAI split chunk、Ollama UTF-8 split chunk 和缺少结束标记测试。

2. 中优先级：结构化输出的“无法解析 JSON”最终被归类为 `SchemaValidation`。（已修复）
   - 位置：`workflow/llm/src/structured.rs` 的 `complete_structured`。
   - 现象：`extract_json` 失败后只是把 parse error 放进 `last_errors`，重试耗尽后统一返回 `LlmError::SchemaValidation`；测试也固定了 `not json` 最终匹配 `SchemaValidation`。
   - 风险：`client.rs` 文档把 `Parse` 映射为 `RawLlmFailureKind::ParseError`、`SchemaValidation` 映射为 `ValidationError`；当前行为会把纯解析失败误报为 schema validation，影响 RawLlmNode 失败分类和后续统计/修复策略。
   - 建议：保留对解析失败的重试，但记录最后失败类型；如果所有尝试都未提取到 JSON，最终返回 `LlmError::Parse`，如果提取成功但 schema 不通过才返回 `SchemaValidation`。
   - 修复：结构化补全保留 parse failure 重试，但记录失败类型；所有尝试都无法提取 JSON 时最终返回 `LlmError::Parse`，只要提取到 JSON 后 schema 不通过则返回 `SchemaValidation`。已补 parse/schema 混合失败回归测试。

3. 中优先级：JSON 提取用首个 `{` 到末个 `}`，对包含多段 JSON/花括号说明的响应不稳。（已修复）
   - 位置：`workflow/llm/src/structured.rs` 的 `extract_json`。
   - 现象：整体解析失败后，退化逻辑截取 `trimmed.find('{')` 到 `trimmed.rfind('}')`。
   - 风险：模型若先输出示例 JSON，再输出最终 JSON；或解释文字中包含 `{...}`，截取结果可能跨越多个对象而解析失败。后续虽会重试，但会把可恢复的正确最终对象误判为坏响应。
   - 建议：实现括号深度扫描，提取第一个完整 JSON object；或支持从 Markdown fenced code block 中优先提取 `json` 块。更稳的方案是让 prompt/backend 强制“只输出一个 JSON object”，并在解析器里拒绝多个 object。
   - 修复：`extract_json` 优先提取 markdown `json` fenced code block；否则按字符串 / 转义状态做括号深度扫描，只取第一个完整 JSON object，不再跨多段对象截取。已补多 JSON 与 fenced code block 回归测试。

4. 中低优先级：OpenAI-compatible 后端没有利用 provider-native JSON / structured output 能力。
   - 位置：`workflow/llm/src/backend.rs` 的 `OpenAiRequest` 与 `complete_structured` 边界。
   - 现象：OpenAI-compatible 请求只发送 `model/messages/temperature/stream`，结构化约束完全依赖 prompt + 本地 schema 复验。
   - 风险：对于支持 JSON mode / JSON schema response format 的后端，当前路径会增加无效输出概率与重试成本。
   - 建议：保持统一 fallback，但在 `StructuredConfig` 或 backend config 中允许启用 provider-native `response_format`；服务端约束和本地 schema 复验两者并用。

5. 低优先级：解析器测试缺少真实协议边界样例。
   - 位置：`workflow/llm/src/backend.rs` 的内嵌测试、`workflow/llm/tests/structured.rs`。
   - 现象：已有测试覆盖基本 stream chunk，但没有覆盖 SSE 注释/event 行、OpenAI error chunk、空 `choices`、content 为 `null`、Ollama error 行、超大响应截断等边界。
   - 风险：第三方 OpenAI-compatible 网关和本地 Ollama 版本差异较多，解析器在协议轻微变化时容易把可诊断错误变成泛化 parse error。
   - 建议：补协议 fixture 测试；对 provider error body 输出更清晰的 `BackendUnavailable`/`Parse` 诊断。

建议修复顺序：

1. 已改为真正增量解析 stream；最大响应大小仍可作为后续加固项。
2. 修正结构化输出的最终错误分类，区分 parse failure 与 schema validation failure。
3. 改进 JSON object 提取算法，补多 JSON / fenced code block / 花括号说明测试。
4. 为 OpenAI-compatible backend 增加可选 provider-native `response_format`。
5. 扩充 OpenAI/Ollama 协议边界 fixture 测试。

## 2026-06-04 — workflow/prompt 模块

审核范围：

- `workflow/prompt/src/lib.rs`
- `workflow/prompt/templates/*.md.jinja`
- `workflow/prompt/schemas/*.json`
- `workflow/prompt/assets/sophia_syntax_baseline.md`
- `workflow/prompt/tests/render.rs`

验证命令：

```bash
cargo test -p sophia-prompt
cargo clippy -p sophia-prompt -- -D warnings
```

结果：全部通过。

总体判断：

- `workflow/prompt` 的职责边界清楚：内嵌模板、schema 与 system preamble，渲染层无 IO，适合作为 prompt 资产的单一来源。
- `minijinja` 已启用 strict undefined，缺上下文字段会失败而不是静默渲染为空，这是重要的最佳实践。
- 测试已经覆盖模板加载、未知模板错误、核心模板 snapshot、schema 顶层 strict、语法基线 snapshot 与共享 prompt 防答案泄漏 token。
- 主要改进空间在 prompt injection 硬化、schema 编译/契约测试、以及模板名/schema 名的类型化绑定。

发现：

1. 中高优先级：模板直接插入任务内容，缺少“数据不是指令”的隔离与定界。（已修复）
   - 位置：`workflow/prompt/templates/design_solution.md.jinja`、`workflow/prompt/templates/implement_design.md.jinja`、`workflow/prompt/templates/repair_code.md.jinja` 等模板。
   - 现象：旧模板中 `objective`、`constraints`、`pseudocode`、`files`、`diagnostics` 等来自用户目标、LLM 产物或代码诊断的内容直接进入 prompt 正文。
   - 风险：若这些字段包含“忽略以上要求”“输出额外字段”等指令型文本，模型可能把数据当成高优先级指令，削弱 schema-only 输出、禁止事项和修复边界。后续 schema 可拦截形状错误，但不能保证语义不被 prompt injection 影响。
   - 修复：所有 workflow prompt 模板统一声明 `BEGIN DATA` / `END DATA` 边界规则，并把任务态内容包入带字段名的 data block；模板明确要求 data block 内文字只能作为事实输入读取，不得作为指令执行。已补覆盖全部模板的边界声明测试、指令型文本隔离测试，并更新稳定渲染快照。

2. 中优先级：schema 测试只验证“合法 JSON + 顶层 additionalProperties:false”，没有验证 JSON Schema 可编译。（已修复）
   - 位置：`workflow/prompt/tests/render.rs` 的 `schemas_are_valid_json_and_strict`。
   - 现象：测试用 `serde_json::from_str` 解析 schema，并只检查顶层 `additionalProperties`。
   - 风险：schema keyword 拼写、`oneOf` 结构、嵌套 schema 等错误要到 `workflow/llm::complete_structured` 调用 `jsonschema::validator_for` 时才暴露；这会把 prompt 资产错误推迟到运行期。
   - 建议：在 prompt crate 的 dev-dependencies 中加入 `jsonschema`，对每个内置 schema 执行 `validator_for`；同时递归检查所有 object schema 的 `additionalProperties:false` 或显式说明可扩展位置。
   - 修复：prompt crate dev-dependencies 引入 `jsonschema`；schema 测试对每个内置 schema 执行 `validator_for`，并递归检查所有 object schema 均声明 `additionalProperties:false`。

3. 中优先级：模板/schema 关系靠字符串约定，没有类型化步骤契约。（已修复）
   - 位置：`workflow/prompt/src/lib.rs` 的 `TEMPLATES` 与 `SCHEMAS`。
   - 现象：模板名是 `design_solution` / `implement_design` / `repair_code` 等，schema 名是 `design_result` / `implement_result` / `repair_result` 等；`schema_for(name)` 接受任意字符串。
   - 风险：上层目前有固定选择层，但 prompt crate 自身不能防止调用方把 `design_solution` 当 schema 名、或新增模板忘记新增 schema。此类错误通常在 LLM 调用前后才被发现。
   - 建议：引入 `PromptStep`/`PromptKind` 枚举或 `PromptSpec { template, schema, system }` 表，提供 `spec_for(step)`；测试枚举所有 step 都能 render fixture 且 schema 可编译。
   - 修复：新增 `PromptStep` / `PromptSpec` / `spec_for(step)`，把 workflow 步骤与 template/schema 绑定为显式契约；测试枚举所有步骤，确认模板与 schema 都存在。

4. 中低优先级：schema 与 Rust DTO 的一致性主要由下游测试间接覆盖。
   - 位置：`workflow/prompt/schemas/*.json` 与 `workflow/engine/src/loop_steps.rs` / `scheduler.rs` 的反序列化 DTO。
   - 现象：prompt crate 不依赖 engine，因而没有直接验证 schema 枚举值、required 字段、默认字段与 Rust DTO serde 形状完全一致。
   - 风险：例如新增 `DecisionAction`、调整 `StateAssessment` 字段或修改 design/repair 输出结构时，schema 和 Rust 类型可能单边漂移。
   - 建议：在 engine 层增加 contract test：每个步骤用 schema 通过的最小样例反序列化到对应 DTO，并把 DTO 再序列化后重新过 schema；或抽出共享 schema/DTO crate，减少双维护。

5. 低优先级：implement / repair 模板对 JSON 输出形状的强调依赖 system prompt。
   - 位置：`workflow/prompt/templates/implement_design.md.jinja` 与 `workflow/prompt/templates/repair_code.md.jinja`。
   - 现象：`implement_system_prompt` 明确要求只输出 JSON；但 `implement_design` 模板本身只写“输出候选 `.sophia` 文件集合”，不像 `repair_code` 那样直接点名 `repair_result` schema。
   - 风险：当模板被单独复用或 system prompt 组合出错时，模型更容易输出 markdown 或非 JSON。
   - 建议：模板正文也显式写出 `implement_result` / `repair_result` 的 JSON 形状，并用 snapshot 固定。

建议修复顺序：

1. 已完成模板任务态内容的统一 data block 定界，并声明“数据不是指令”。
2. 在 prompt crate 内编译所有 JSON Schema，并递归检查 strict object schema。
3. 用 `PromptSpec`/枚举替代散落字符串，绑定 template/schema/system。
4. 在 engine 层增加 schema ↔ Rust DTO 往返 contract test。
5. 补强 implement / repair 模板自身的 JSON 输出要求。

## 2026-06-04 — workflow/engine 模块

审核范围：

- `workflow/engine/src/lib.rs`
- `workflow/engine/src/step.rs`
- `workflow/engine/src/loop_steps.rs`
- `workflow/engine/src/implement_loop.rs`
- `workflow/engine/src/scheduler.rs`
- `workflow/engine/src/traversal.rs`
- `workflow/engine/src/select_materialize.rs`
- `workflow/engine/src/code_check.rs`
- `workflow/engine/src/prompts.rs`
- `workflow/engine/tests/*.rs`

验证命令：

```bash
cargo test -p sophia-engine
cargo clippy -p sophia-engine -- -D warnings
```

结果：全部通过。

总体判断：

- `workflow/engine` 是目前 workflow 层测试覆盖最扎实的模块之一：LLM step、design/implement/repair、implement-loop、scheduler、tree traversal、selection/materialize 都有集成测试覆盖。
- `run_llm_step` 明确保证 snapshot 与 prompt 同源，LLM 失败会落 `RawLlmNode`，成功产物由上层连 `consumed→ snapshot`，主路径设计清楚。
- scheduler/traversal 对“动作选择由 LLM 产生，执行层只承接”的边界划分较好，decompose 的人类审查点也比早期自动绑定更符合 graph 语义。
- 主要风险集中在跨事件/跨文件系统副作用的原子性、预算统计作用域、失败调用的 snapshot 可追溯性，以及 schema/DTO 契约测试仍在下游间接覆盖。

发现：

1. 高优先级：selection/materialize 先写文件系统，后写图事件；图写失败会留下无审计记录的物化结果。（已修复）
   - 位置：`workflow/engine/src/select_materialize.rs` 的 `run_materialization` 与 `run_selection_materialize`。
   - 现象：旧实现中 `run_materialization` 先调用 `candidate.materialize(write_root)` 写盘，再创建 `MaterializeNode` 和 `materializes→ Selection` 边。若文件写入成功后图写入失败，文件系统已经改变，但 Development Graph 没有 MaterializeNode 记录。
   - 风险：materialize 是不可逆收尾动作，审计链应比普通节点更强；旧顺序会产生“产物已写出、图上不可见”的状态。`run_selection_materialize` 还会先创建 SelectionNode，若后续 materialize payload 非法，图上会留下已选择但未物化的半进度。
   - 修复：`run_materialization` 先创建 `MaterializeNode` 和 `materializes→ Selection` 锚点，再执行文件写入，避免不可逆文件副作用完全脱离图记录；`run_selection_materialize` 在创建 Selection 前预校验 materialize payload，非法目标根不会留下 Selection/Materialize，也不会写文件。已补 split 与一体化路径的失败无副作用测试。
   - 剩余边界：当前没有引入 `MaterializeCompleted` 或 outbox/manifest 双阶段协议；若文件写入在图锚点之后失败，图上保留的是物化意图/事件锚点，不伪装成事务完成记录。

2. 高优先级：LLM 节点总数预算按全图统计，不是按当前 goal/scheduler run 统计。（已修复）
   - 位置：`workflow/engine/src/scheduler.rs` 的 `budget_exceeded` 与 `count_llm_nodes`。
   - 现象：旧实现中 `count_llm_nodes` 遍历 `store.nodes()` 中全部 LLM-provenance 节点；`SchedulerBudget` 注释却说 `max_total_llm_nodes` 是“单 goal LLM 产物节点总数上限”。
   - 风险：同一个图里已有其它目标、历史尝试或兄弟子树时，新目标可能在尚未产生任何本轮 LLM 节点前就触发预算耗尽；树遍历中后续子目标也会被前面子目标的 LLM 节点挤占预算，和“单 goal”语义不一致。
   - 修复：`run_goal_loop` 入口记录 baseline LLM 节点数，预算检查使用本轮新增 delta；预算耗尽诊断也明确为“本轮 LLM 节点总数”。已补历史 LLM 节点不消耗新 run 预算的回归测试。

3. 中优先级：失败 LLM 调用创建了 snapshot，但 RawLlmNode 无法连回该 snapshot。（已修复）
   - 位置：`workflow/engine/src/step.rs` 的 `run_llm_step` 失败分支。
   - 现象：失败路径会先建 `ContextSnapshotNode`，再在 LLM 错误时创建 `RawLlmNode` 并 `attempted→ target`；但 `RawLlmNode` 没有 `consumed→ snapshot` 边，当前 `EdgeKind::Consumed` 也不允许 RawLlm。
   - 风险：测试确认失败路径会建 snapshot，但图上无法从 RawLlmNode 找到“失败时模型看到的上下文”。这削弱了失败审计和复现能力，尤其是 parse/schema/self-check 失败时，snapshot 对排查很关键。
   - 建议：扩展 edge schema 允许 `RawLlm consumed→ ContextSnapshot`，或在 `RawLlmPayload` 中记录 snapshot id/摘要；随后更新 `validate_i6` 或新增失败调用不变量测试。
   - 修复：`EdgeKind::Consumed` 允许 `RawLlm → ContextSnapshot`；`run_llm_step` 失败分支在 `attempted→ target` 外同步追加 `consumed→ snapshot`。已补失败 RawLlm 可回溯 snapshot 的测试。

4. 中优先级：成功路径多处“建节点后逐条加边”，缺少批量原子提交。（已修复）
   - 位置：`workflow/engine/src/loop_steps.rs` 的 `design_solution`、`revise_design`、`build_code_node`、`implement_design`、`repair_code`，以及 `workflow/engine/src/scheduler.rs` 的 `make_decision`。
   - 现象：例如 `design_solution` 先创建 PseudocodeNode，再加 `consumed→ snapshot` 和 `addresses→ target`；`implement_design` 先建 CodeNode 与基础边，再加 `implements→ Pseudocode`。
   - 风险：若后续边写入失败，图中会留下缺少必要边的 LLM 节点，可能违反 I6 或让 active context/后续查询读到半成品。当前单进程内存库很少触发，但文件库/磁盘错误/未来校验增强会放大该问题。
   - 建议：依赖 graph-db 提供 transaction/batch append 后，把“节点 + 必需边”作为一个原子批次提交；短期可先把所有可预校验项前置，并补失败无副作用测试。
   - 修复：graph-db 新增 crate 内部 `append_node_with_outgoing_edges`，在 SQLite immediate transaction 中分配 NodeId、写 NodeCreated 和必需 EdgeAdded 事件；写入前复用节点契约与边契约校验。LLM factory 暴露 `decision/pseudocode/code/question_with_edges`，engine 的 decision、clarification、design、revise、implement、repair 成功产物改用单事务节点+必需边落图。

5. 中低优先级：engine 对 prompt schema 与内部 DTO 的契约没有直接回归测试。
   - 位置：`workflow/engine/src/step.rs` 的 `step_schema`，以及 `workflow/engine/src/loop_steps.rs` / `scheduler.rs` 的 `DesignResult`、`ImplementResult`、`DecisionPayload` 等。
   - 现象：测试用 prompt crate 的 schema 驱动 mock LLM 输出，但没有系统性覆盖每个 schema 的最小合法样例与 Rust DTO serde 形状互相往返。
   - 风险：schema、prompt、DTO 三者任一侧变更时，可能出现 schema 允许但 DTO 反序列化失败，或 DTO 字段默认与 schema required 不一致。
   - 建议：增加 contract test：对 `design_result`、`decompose_result`、`implement_result`、`repair_result`、`decision` 各提供最小合法样例，先过 `jsonschema`，再反序列化到 DTO；对关键 DTO 再序列化回 JSON 并复验 schema。

6. 低优先级：`code_check::domain_of_path` 对非 domain-first 路径没有结构化拒绝。（已修复）
   - 位置：`workflow/engine/src/code_check.rs`。
   - 现象：domain 从 `path.split('/').next().unwrap_or("")` 推导；空路径或无 `/` 路径会得到空/裸 domain，之后交给 `sophia_check::check_program`。
   - 风险：实现阶段 system prompt 要求 domain-first 布局，但 engine 的确定性 check 不先诊断路径形状，错误可能变成较晚的 HIR/semantic 问题。
   - 建议：候选文件来自 LLM/artifact，是系统外输入进入 gate 的第一站，应在 code_check 阶段增加路径约束诊断：非空、相对路径、至少两段、`.sophia` 后缀、无 `..`。通过 gate 后，后续同进程模块可依赖该契约；若从持久化 artifact 重新加载，则再次按边界处理。
   - 修复：`code_check` 在语法/语义前先执行 `validate_candidate_path`，非法路径输出 `PATH` 诊断并阻断后续阶段；路径契约与 artifact 写入 / 加载复用。

建议修复顺序：

1. 已完成 selection/materialize 图记录与文件写入顺序修正，并补失败无副作用测试。
2. 已完成 scheduler LLM 节点预算作用域修正，按本轮 delta 计数。
3. 已完成 RawLlmNode 可追溯到失败调用的 ContextSnapshot。
4. 已在 graph-db 提供事务/batch append，并收敛 engine 的“节点 + 必需边”原子提交。
5. 增加 prompt schema ↔ engine DTO contract test。
6. 为 code_check 增加候选文件路径形状诊断。

## 2026-06-04 — lsp 与 cli 模块

审核范围：

- `lsp/src/lib.rs`
- `lsp/src/analysis.rs`
- `lsp/src/convert.rs`
- `lsp/src/server.rs`
- `lsp/tests/analysis.rs`
- `cli/src/main.rs`
- `cli/src/commands.rs`
- `cli/src/project.rs`
- `cli/src/render.rs`
- `cli/src/graph_cmd/mod.rs`
- `cli/src/graph_cmd/gate.rs`
- `cli/src/verifier_store.rs`
- `cli/tests/*.rs`

验证命令：

```bash
cargo test -p sophia-lsp -p sophia-cli
cargo clippy -p sophia-lsp -p sophia-cli -- -D warnings
```

结果：全部通过。

总体判断：

- LSP 不是纯占位：已有协议无关 `Workspace`、syntax/HIR/semantic 诊断、hover、goto definition、UTF-16 位置换算和 tower-lsp 外壳；但仍是起步版，全量重算、无持久 workspace index。
- CLI 是较厚的协调层，确定性命令覆盖 check/build/run/smoke/repair-context，graph 命令覆盖 design/implement-loop/select/materialize；WASM build manifest 和 registry fingerprint 校验是加分点。
- CLI 测试覆盖面很好：二进制级 pipeline 测试、WASM backend、manifest drift、graph append、LLM 失败 RawLlmNode、hidden case audit gate 都有回归保护。
- 主要风险集中在 graph 命令的图事件与文件 artifact 非原子、LSP 对 workspace-level HIR 错误的降级处理，以及路径/位置换算边界。

发现：

1. 高优先级：`graph design` / `implement-loop` 先写图节点，后写 artifact 文件；文件写失败会留下不可继续的图节点。
   - 位置：`cli/src/graph_cmd/mod.rs` 的 `design`、`write_pseudo_artifact`、`implement_loop`、`write_code_artifacts`。
   - 现象：`engine::design_solution` 成功后已创建 `PseudocodeNode` 与边，随后 CLI 才把 `.pseudo` 和 `.libs` 写到 `sophia-runs/graph/artifacts/`；`implement-loop` 通过后也先已有 `CodeNode`，再写候选文件正文。
   - 风险：若磁盘写入失败、权限不足、进程中断，会留下图上存在但 artifact 缺失的 Pseudocode/Code 节点；后续 `graph implement-loop` / `select` / `materialize` 会失败，且图上难以区分“LLM 成功但 artifact 丢失”和“正常产物”。
   - 建议：把 artifact 写入纳入可恢复协议：先写临时 artifact，成功后再提交图节点；或图上增加 artifact-write Diagnostic/Materialize-like 状态。短期至少在写 artifact 失败时追加一个 DiagnosticNode 标记产物不完整，并提供 `graph repair-artifacts` / `graph validate-artifacts` 检查命令。

2. 高优先级：graph LLM prompt 路径使用 `standard_registry`，不会暴露项目三方库资产。（已修复）
   - 位置：`cli/src/graph_cmd/mod.rs` 的 `render_design_request` 与 `CliImplementPrompts::system`。
   - 现象：旧实现中 design prompt 的 `stdlib_catalog` 来自 `sophia_stdlib::standard_registry().catalog()`；implement/repair system prompt 的库资产也来自 `standard_registry().preamble(...)`。普通 CLI `check/build/run` 已有 `full_registry_for(root)` 支持三方库。
   - 风险：项目通过 `sophia_libs/` 或 `SOPHIA_LIB_PATH` 引入三方库时，graph design 阶段看不到库目录，implement 阶段也注入不了三方库 preamble；LLM 可能无法正确使用项目库，或生成与确定性 build/run 不一致的方案。
   - 修复：`graph design` 与 `graph implement-loop` 均通过 `commands::library_registry(root)` 构建项目完整库注册表；design prompt 使用 `registry.catalog()`，implement/repair system prompt 使用同一 `registry.preamble(libs)`。engine 的 design/revise 唯一路径接收 `LibrarySelectionPolicy`，把允许库集合编入 `design_result` schema，未知库在结构化输出校验阶段走 RawLlm 失败路径，不创建 PseudocodeNode；`implement-loop` 读取 `.libs` 后仍按当前 registry 硬校验，防止旧 artifact 或 registry 漂移被静默忽略。已补 design catalog、implement system prompt、未知库前置拒绝与 `.libs` 拒绝测试。

3. 中优先级：LSP 在 workspace index 构建失败时退化为空 index，会吞掉全局 HIR 错误。（已修复）
   - 位置：`lsp/src/analysis.rs` 的 `Workspace::diagnostics`。
   - 现象：`resolve_program(&inputs, &registry)` 返回 `Err` 时直接使用 `AsgIndex::new(&registry)`，随后按 item 调 `resolve_item`。
   - 风险：重名顶层节点、一文件多节点等 workspace-level index 错误可能不会以 LSP 诊断展示；用户在编辑器里看不到 CLI check 会报出的结构性错误，形成工具反馈不一致。
   - 建议：把 `resolve_program` 的 build error 转成每个相关文档的 HIR diagnostic，或至少在当前文档发布一个 workspace-level diagnostic；不要静默降级为空 index。
   - 修复：LSP 不再退化为空 index；`resolve_program` build error 会返回当前文档的 `WorkspaceIndex` HIR diagnostic，并停止后续语义分析。已补重复顶层节点的 workspace-level 诊断测试。

4. 中优先级：LSP `position_to_byte` 对尾随空行位置换算有边界错误。（已修复）
   - 位置：`lsp/src/convert.rs` 的 `position_to_byte`。
   - 现象：当源码以换行结尾、LSP 位置落在最后一个空行时，循环结束后 `cur_line == pos.line`，但 `line_start` 仍可能保持 0；例如 `"a\n"` 的 line=1/character=0 应映射到 `source.len()`，当前逻辑可能回到文件开头。
   - 风险：hover/goto 在尾随空行或空行边界可能查询到错误 identifier，表现为跳转/悬浮错位。
   - 建议：重写为按 `split_inclusive('\n')` 或显式维护每行起始 byte 表；补 trailing newline、连续空行、emoji/surrogate pair 的 roundtrip 测试。
   - 修复：`position_to_byte` 改为先构建行起始 byte 表，再在目标行内按 UTF-16 code unit 前进；补尾随空行、连续空行与 emoji/surrogate pair 边界测试。

5. 中优先级：`graph select/materialize` 加载 artifact 时信任图内 `CodePayload.files`，没有再次做路径逃逸校验。（已修复）
   - 位置：`cli/src/graph_cmd/gate.rs` 的 `load_candidate_files`。
   - 现象：函数从 CodeNode payload 取相对路径后直接 `base.join(&rel)` 读取。正常路径由 `write_code_artifacts` 写入时校验过，但 graph-db replay 当前不是 checked replay，且 CodePayload 本身没有路径约束。
   - 风险：若图数据库被旧版本、手工编辑或并发问题写入恶意路径，select/materialize gate 可能读取 artifacts 目录外文件。
   - 建议：这是从 SQLite/artifact 持久化重新进入系统的边界，应在 `load_candidate_files` 复用 `write_code_artifacts` / materialize 同等路径校验，拒绝绝对路径、`..`、空路径、反斜杠和非 `.sophia` 后缀；同一进程内刚由 `write_code_artifacts` 产出的值则可依赖上游契约。
   - 修复：engine 导出 `validate_candidate_path`，`code_check`、`write_code_artifacts` 与 `load_candidate_files` 复用同一候选路径契约：非空、相对路径、正斜杠、至少三段 domain-first、无 `.` / `..`、`.sophia` 后缀。已补持久化逃逸路径拒绝测试。

6. 中低优先级：项目扫描递归跟随目录 symlink，可能遇到循环或扫描项目外文件。
   - 位置：`cli/src/project.rs` 的 `collect_sophia_files`。
   - 现象：使用 `path.is_dir()` 递归，`Path::is_dir` 会跟随 symlink。
   - 风险：`domains/` 下若存在指向父目录或外部目录的 symlink，扫描可能递归爆炸或把项目外 `.sophia` 纳入 check/build/run。
   - 建议：使用 `symlink_metadata` 判断 file type，默认不跟随 symlink；如需支持 symlink，维护 visited canonical dirs 集合并限制在项目根内。

7. 低优先级：LSP 符号表按裸名称索引，会覆盖跨 domain 同名符号。
   - 位置：`lsp/src/analysis.rs` 的 `symbols`、`hover`、`goto_definition`。
   - 现象：`symbols()` 返回 `BTreeMap<String, SymbolDef>`，同名顶层节点后插入覆盖先前定义。
   - 风险：在多 domain 或重名未被及时诊断时，hover/goto 可能跳到错误文件。当前 CLI/HIR 会约束一部分重名问题，但 LSP 降级为空 index 时更容易暴露该问题。
   - 建议：符号 key 使用 `(domain, name)` 或 HIR canonical name；identifier 查询结合当前文档 domain 和 resolved symbol，而不是裸文本全局查找。

建议修复顺序：

1. 为 graph LLM 产物建立 artifact 写入与图事件的一致性/恢复机制。
2. 已完成 graph prompt 路径改用项目 full registry，并验证 `.libs` 库名。
3. LSP 不再吞掉 `resolve_program` workspace-level 错误，转为可见诊断。
4. 修复 LSP 位置换算尾随空行边界并补测试。
5. 在 graph gate 从持久化 artifact 重新加载时执行路径安全校验。
6. 项目扫描默认不跟随 symlink，或显式做循环/根目录限制。
7. LSP 符号查询改用 domain-aware/canonical symbol key。
