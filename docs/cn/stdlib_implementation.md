# Sophia 库实现

> 本文档与 `language_implementation.md` 对称：定**库插件的实现机制**——清单解析与注册表、各层如何
> 消费注册表、host 注入接缝（路线 B）、crate 分层、测试边界。设计动机与边界见 `stdlib_design.md`；
> 具体库的语言契约见各库文档（如 `http_lib.md`）。
>
> **状态：活文档。** 当前已落地：库插件 P1（清单驱动 + `LibraryRegistry` + 路线 B host + 标准库 crate）。

---

## 一、crate 分层

```
sophia-library  (core 层，契约类型)
  src/typedesc.rs   TypeDesc / Scalar（受限中立类型 mini-DSL）
  src/manifest.rs   RawManifest（library.toml 反序列化形态）
  src/registry.rs   LibraryRegistry / OpContract / LibraryContent / SophiaSource
  src/error.rs      LibraryError（清单 / 冲突 / ABI 错误）
  —— 无 runtime::Value，core/* 可依赖；零文件 IO（只解析传入清单字符串）

sophia-stdlib   (内容层)
  libs/http/{library.toml, http.md}      标准库内容（include_str! 进二进制）
  libs/file/{library.toml, file.md}
  src/lib.rs          standard_registry()（内置清单 → LibraryRegistry）
  src/native_host.rs  register_native_hosts（真实 reqwest/std::fs）+ mock_host（确定性桶）
  —— 依赖 sophia-library + sophia-runtime；归 core 之上、协调层之下（可做 IO）

sophia-runtime  (host 注册表归属)
  src/host.rs   HostFn trait + HostRegistry（路线 B：(family,op) → Box<dyn HostFn>）
```

依赖方向（无环）：`sophia-library ← core/hir, core/semantic, sophia-runtime`；
`sophia-runtime, sophia-library ← sophia-stdlib`；`sophia-stdlib ← cli / tools/check / tools/codegen / lsp`。

`core` **不依赖 `sophia-stdlib`**：`core/hir` 的 `AsgIndex::with_libraries(&registry)` 与 `core/semantic`
的 `TypeChecker::new(model, ast, &index)` 只消费 `&LibraryRegistry` / `&AsgIndex`（携库契约）只读参数；
core 的确定性测试用**内联清单**经 `sophia-library` 构建夹具注册表，不碰 stdlib、不碰文件系统。

---

## 二、清单 → 注册表 → 各层

### 2.1 清单解析（`sophia-library`）

`LibraryRegistry::build(Vec<LibraryContent>)` 解析每个库的 `library.toml`，校验：目录名 = 清单 name、
`abi_version` 受支持、库名 / family / domain 唯一、TypeDesc 形状合法。冲突 / 非法一律 `Err`（不静默
覆盖）。产物按库名 / `family.op` 字典序聚合（确定性）。

### 2.2 各层消费注册表（替代硬编码）

| 触点 | 实现 |
| --- | --- |
| HIR effect 符号表 + 特殊根 | `AsgIndex::with_libraries(registry)` 把 effectful op 灌入 `effect_ops`（arity=0）+ family 灌入 `library_families` + op 契约灌入 `library_ops`（均 `#[serde(skip)]` 派生符号表） |
| HIR 特殊根放行 | `resolve` 用 `index.is_library_family(family)`（替代 `File`/`Http` 字面量白名单） |
| 语义签名校验 | `type_layer::infer_effect_op` 用 `index.library_op(family, op)` 取 `OpContract`，把 TypeDesc 转 `Ty`（`typedesc_to_ty`）做**表驱动**形参 / 返回类型校验（intent 严格相等复用既有 `assignable_to`） |
| 运行时分派 | 解释器 `try_effect_op` 用 `host.has_op(family, op)` 判定特殊根 effect op、`host.call(family, op, args)` 委派（路线 B） |
| codegen | `CodegenInput` 持 `lib_index`（`AsgIndex::new().with_libraries(registry)`）供 emit 重算 TypeTable；host import 名 = `host_fn` |
| 提示词 | `registry.catalog()`（design）/ `registry.preamble(libs)`（implement） |

`Console`（`print`）仍由 `hir::builtins::BUILTIN_EFFECT_OPS`（唯一内置 effect 族）承载、`HostRegistry::
console_write` 捕获——语言内置，不经库注册表。

### 2.3 入口 registry 来源

- **确定性子门禁**（`tools/check::check_program`、`tools/codegen`、LSP、graph gate hidden-case 模型构建、
  graph design/implement-loop 的库目录/资产渲染）：用 `sophia_stdlib::standard_registry()`（标准库；三方
  发现是协调层启动行为，不进核心确定性门禁）。
- **CLI 生产命令**（`check` / `run` / `index` / `graph`〔ASG 摘要〕 / `context` / `repair-context`）：用
  `library_registry(root)` = `sophia_stdlib::full_registry_for(root)`（标准库 + 以**项目根**解析
  `<root>/sophia_libs/` + `$SOPHIA_LIB_PATH` 发现合并）。发现失败诚实报错退出（不静默跳过 / 部分加载）。
- **三方发现入口**（启动时一次性，P2）：`sophia_stdlib::full_registry_for(project_root)`（以项目根解析
  `<root>/sophia_libs/` + `$SOPHIA_LIB_PATH`，CLI 用）/ `project_roots(project_root)`（只计算发现根）/
  `full_registry_from(roots)`（指定根，供测试 fixture 显式构建、确定）。失败 `DiscoverError`。
- **库 Sophia 源码装入**：`hir::LibrarySources::from_registry(&reg)` 把 `registry.sophia_sources()` 的库
  `.sophia` 解析为 owned AST；调用方把 `program_inputs()` 并入用户 inputs（resolve / index）、`asts()`
  并入用户 AST（`analyze_program` / `run_action`）——库节点就是「更多 Sophia 代码」。CLI 各命令经
  `commands::library_context(root)`（返回 `(registry, LibrarySources)`）在命令函数作用域持有库 AST（owner），
  再并入用户 inputs / asts。`tools/check::check_program` 同样并入（标准库当前无源码节点，纯 Sophia 库有）。
- **strip-assist 门禁 registry 对称**：`check_strip_assist_equivalence(sources, registry, index)` 两侧
  （original / stripped）用同一 `registry` 并对称并入同一批库 `LibrarySources`，否则用户引用库节点会让
  strip 前后名称解析不对称、误判不等价。指纹只覆盖用户源码（库源码两侧相同、相消）。
- **core 单测**：内联清单经 `LibraryRegistry::build` 构建夹具（如 hir/semantic 测试的中性 File/Http 清单），
  不依赖 stdlib。

### 2.4 跨 domain 豁免（纯 Sophia 库）

库 Sophia 节点登记到「库名即 domain」（隔离）。用户跨 domain 调库节点（如 `SophiaDigest`）经
`AsgIndex.library_domains`（`with_libraries` 从 `registry.sophia_sources()` 填充）+
`resolve::check_cross_domain_domain` 对库 domain 跳过 `ImplicitCrossDomain` 豁免（用户↔用户跨 domain 仍
受检）。库节点本身走与用户码同一套静态检查，无特权——豁免只影响可见性。

---

## 三、host 注入接缝（路线 B）

`runtime::HostRegistry` 是 `(family, op) → Box<dyn HostFn>` 注册表 + console 捕获。`HostFn::call(&[Value])
-> Result<Value, String>`；闭包经 `HostRegistry::register_fn` 注册，内部包装成 `HostFn`。

- **执行入口**：`run_action`（空注册表，仅 Console / 纯逻辑）；`run_action_with_host(.., &mut HostRegistry)`
  （注入库 host）。
- **标准库 host**（`sophia-stdlib::native_host`）：`register_native_hosts(&mut host)` 注册真实 `reqwest`
  `Http.Get` / `std::fs` `File.Read`·`File.Write`；`mock_host()` / `register_mock_hosts` 注册确定性内存桶
  （`MockBuckets::seed_http`/`seed_file`，测试 / 差测试用）。
- **三方 WASM host**（`sophia-stdlib::register_wasm_library_hosts(&mut host, &registry)`）：遍历注册表里
  携带 `host.wasm` 字节的库（= 三方 WASM-effect 库），为其每个 effect-op 注册一个 `runtime::WasmHostFn`
  ——内部持 `host.wasm` 的 `wasmi` 实例（`wasmi` 已提为 `sophia-runtime` 正式依赖），按统一字节 ABI 转发；
  `host.wasm` 导出 `memory` + 每 op 一个与 `host_fn` 同名的函数（标量 i64 直传 / 文本经 ptr-len + stash +
  read_copy，见 `stdlib_design.md` §五.4 / §六.1）。当前 ABI 子集 `(Int, Int) -> Int`；超出子集的签名 /
  装载失败诚实 `Err`（不静默跳过、不伪造 host）。区分准则 = 装载方式（注册表是否持 `host.wasm`），与标准库
  native host 互补不重叠。仅加载三方 WASM 库时实例化，标准库 native host 零 wasm 开销。
- **CLI `sophia run` 组装**（`commands::run_with_host`）：先 `register_wasm_library_hosts`（无三方 WASM 库时
  no-op），再据入口 action 声明 effect 按需 `register_native_hosts`（`Http.Get` / `File.*` 才注册，纯逻辑
  零开销）。三方 WASM op 多为 `effectful=false`、不经声明 effect 体现，故其 host 无条件注册。

**诚实性红线**：mock 未命中 / 真实失败（网络非 2xx / 超时 / 文件不存在 / 非 UTF-8）/ wasm trap 一律 `Err`，
解释器物化为 `RuntimeError`（硬错误中止），绝不伪造成功 / 编造默认响应。

---

## 四、测试边界

- **真实外部调用（真实网络 / 文件）绝不进 `cargo test`**（与 e2e / benchmark 真实 LLM 同策略）。真实 host
  靠 e2e（真实 IO）+ 人工验证；确定性测试一律用 `mock_host`。
- **库契约 / 注册表测试**（确定性，进 `cargo test`）：`sophia-library` 的注册表构建 / 冲突 / TypeDesc 解析；
  `sophia-stdlib` 的 `standard_registry` 含 File/Http + 目录 / preamble + 资产防泄漏（不含任务 token）+
  mock host 往返 / 未命中诚实 Err。
- **解释器分派机制**（`runtime/tests/interpret.rs`）：用**中性测试库 `Vault`**（内联清单 + 注册闭包）验证
  `Lib.Op(args)` 分派——runtime 不依赖 sophia-stdlib（后者反向依赖 runtime），故用中性库测机制、不测具体
  标准库语义（那归 stdlib 测试）。

---

## 五、新增一个库的实现清单

1. **建库目录** `libs/<lib>/`（标准库在 `sophia-stdlib`；三方在三方根）：`library.toml`（清单）+
   `<lib>.md`（资产，按 `stdlib_design.md` §3.1 结构）+ 可选 `src/*.sophia`（Sophia 源码库）+ 可选
   `host.wasm`（三方 WASM-effect 库）。
2. **登记**：标准库在 `sophia-stdlib::STDLIB_LIBS` 加一行（库名 + `include_str!` 清单 + 资产）；三方放入
   `./sophia_libs/` 或 `$SOPHIA_LIB_PATH`。
3. **host**（若 effectful）：标准库在 `register_native_hosts` 注册 native 闭包 + 在 `register_mock_hosts`
   注册 mock；三方提供 `host.wasm`。
4. **契约文档**（若引入新 effect 族）：新建 `<lib>_lib.md`，在 `stdlib_design.md` §八库清单登记一行。
5. **测试**：清单构建 / 资产防泄漏 / mock host 往返；端到端验收落 e2e（真实 IO）。

> **零改语言核心**：上述步骤都**不动** `core/*` / `runtime` 的代码——这正是库插件模型的目标。`core` 改动
> 只在「扩 TypeDesc mini-DSL」（某库需更复杂签名）时才需要，且须先过设计门。

---

## 六、变更记录

- 2026-05-31 — 建立库实现文档：提示词资产布局与 API、两阶段按需选取贯通、host 注入接缝、测试边界。
- 2026-05-31 — **库插件 P1 落地（消化原 `library_plugin.md`）**。重写为清单 → 注册表 → 各层消费的实现
  视图：新增 `sophia-library`（契约类型 + 清单解析）+ `sophia-stdlib`（内容 + native/mock host）两 crate；
  `AsgIndex::with_libraries` 注入库 effect/特殊根/op 契约；`type_layer::infer_effect_op` 表驱动化
  （TypeDesc → Ty）；host 改路线 B（`HostRegistry: (family,op) → Box<dyn HostFn>`，`runtime` 不内置具体库）；
  `File`/`Http` 从 `core` 硬编码迁入 `sophia-stdlib/libs/`；prompt crate 去 stdlib 内容（库目录 / 资产改由
  注册表提供）；CLI 的 `CliHost` 删除、改 `register_native_hosts`。core 单测用内联清单夹具、不依赖 stdlib。
  纯重构、零行为变化（标准库 File/Http 端到端语义不变），全工作区测试全绿。新增库**零改语言核心**。
- 2026-05-31 — **库插件 P2 落地（三方动态发现 + 两演示库，消化原 `library_plugin_p2.md`）**。新增
  `sophia-stdlib::discover`（`full_registry_for` / `full_registry_from` / `project_roots` / `DiscoverError`，
  扫约定根 → 读清单 + 资产 + `.sophia` 源码 + `host.wasm` → 合并注册表，确定性排序）；`hir::LibrarySources`
  （库 Sophia 源码解析为 owned AST，并入 index / model / 执行）+ `HirError::LibrarySourceParse`；
  `AsgIndex.library_domains` + `is_library_domain` + `resolve` 跨 domain 豁免；`runtime::WasmHostFn`
  （`wasmi` 加载 `host.wasm`，`(i64,i64)->i64` 形态，`wasmi` 提为 runtime 正式依赖 + `wasm-encoder` dev-dep）。
  两演示库 fixture（`stdlib/tests/fixtures/sophia_libs/hash_sophia`、`hash_wasm`）+ 集成测试
  `stdlib/tests/library_demo.rs`（`host.wasm` 由 wasm-encoder 测试时生成、`.gitignore` 忽略）。验收发现 +
  跨 domain 豁免 + 两库等价 digest，全确定进门禁。CLI 生产接线（`full_registry_for(root)` + 库源码并入命令 inputs +
  `sophia run` 注册三方 WASM host）列为后续项。
- 2026-05-31 — **CLI 生产路径接线落地（P2 收尾）**。`discover` 新增 `project_roots(root)` /
  `full_registry_for(root)`（以项目根而非进程 CWD 解析 `<root>/sophia_libs/`，CLI 命令以 `--root` 定位项目）；
  `native_host` 新增 `register_wasm_library_hosts(host, registry)`（遍历注册表 `host.wasm` 库注册 `WasmHostFn`，
  ABI 子集 `(Int,Int)->Int` 校验 + 装载失败诚实 `Err`）。`commands` 的 `library_registry` 改签名为
  `library_registry(root) -> Result<LibraryRegistry>`（= `full_registry_for`）+ 新增 `library_context(root)`
  返回 `(registry, LibrarySources)`；`check` / `run` / `index` / `graph` / `context` / `repair-context` 经它
  发现三方库并把库 `.sophia` 并入 inputs/asts；`run_with_host` 统一注册 WASM host（无条件）+ native host（按
  入口 effect 按需），替换原 `run_with_default_host` / `run_with_real_host`。`tools/check` 的
  `check_strip_assist_equivalence(sources, registry, index)` 改 registry-aware（两侧对称并入库源码），
  `check_program` 并入库源码。graph gate / design / implement-loop 与 LSP / codegen 仍用 `standard_registry`
  （确定性子门禁）。手动 smoke 验证纯 Sophia + WASM 两库经 CLI `check`/`run` 得同一 digest。372 passed
  （+3 WASM host 注册单测）/ 0 failed，clippy 0 警告，fmt clean。
