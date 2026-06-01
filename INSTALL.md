# 安装与构建

Sophia 是一个 Rust 工作区（Cargo workspace）。本文档说明如何从源码构建、测试与运行。

## 前置条件

| 依赖 | 版本 / 说明 |
| --- | --- |
| Rust 工具链 | `rustc` ≥ **1.95**（edition 2021），含 `cargo`。推荐用 [rustup](https://rustup.rs/) 安装。项目跟随最新稳定版，CI 守护此 MSRV（见 `Cargo.toml` `rust-version`） |
| C 编译器 | 用于编译 vendored 的 Tree-sitter parser（`core/syntax/src/parser.c`）。Linux 用 `gcc`/`clang`，macOS 用 Xcode Command Line Tools |
| `python3`（可选） | **仅** benchmark 的 `baseline` mode 需要——它执行 LLM 生成的 Python 候选。普通构建 / 测试 / e2e 不需要；缺失时 benchmark 自动只跑 `sophia` mode。作为运行期外部工具调用，**不进** Cargo 依赖树 |

说明：

- **SQLite 无需系统安装**——`workflow/graph-db` 使用 `rusqlite` 的 `bundled` 特性，随构建自带。
- **普通构建无需 Tree-sitter CLI**——grammar 已生成为 vendored `parser.c`（ABI 15），`build.rs` 只编译它。只有在**修改 `grammar.js`** 时才需要 `tree-sitter-cli` 0.26.x 重新生成（见下文「修改语法」）。
- 网络：核心构建只需 crates.io。运行调用 LLM 的工作流命令 / e2e 测试时才需要网络与 API key。

## 构建

```bash
# 克隆后在仓库根目录
cargo build --workspace
```

发布优化构建：

```bash
cargo build --workspace --release
# 二进制位于 target/release/sophia
```

## 测试

```bash
# 全工作区单元 / 集成测试（确定性，不调用 LLM）
cargo test --workspace
```

代码风格与静态检查（与 CI 一致）：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## 安装 `sophia` 命令

```bash
# 从工作区安装 CLI 到 ~/.cargo/bin
cargo install --path cli

# 之后可直接使用
sophia --help
```

或不安装、直接通过 cargo 运行：

```bash
cargo run -p sophia-cli -- --help
```

## 端到端（真实 LLM）测试（可选）

e2e 测试发起真实网络请求、消费非确定 LLM 输出，因此是 `example`，**不进** `cargo test` 门禁。

```bash
export SOPHIA_LLM_API_KEY=<your-key>     # OpenAI 兼容模式需要；不落盘 / 不进图 / 不打印
cargo run -p sophia-cli --example e2e -- --list          # 列出全部用例（不需 key）
cargo run -p sophia-cli --example e2e -- --case G1-01    # 跑单个用例
cargo run -p sophia-cli --example e2e -- --group g1      # 跑某组
cargo run -p sophia-cli --example e2e -- --llm-mode ollama --case G1-01
```

可选环境变量：`SOPHIA_LLM_MODE`、`SOPHIA_LLM_MODEL`、`SOPHIA_LLM_BASE_URL`、`SOPHIA_LLM_MAX_REPAIRS`。
OpenAI 兼容模式未设置 `SOPHIA_LLM_API_KEY` 时干净跳过；Ollama 模式默认本地
`http://localhost:11434`、模型 `qwen3.6:latest`，无需 API key。批量串行执行见 `scripts/run_e2e.sh`。
详见 `docs/e2e_test.md`。

## 编程能力基准测试（可选）

横向对比「LLM 直接写 Python」与「Sophia 工作流」在多组小题上的**成功率 + 耗时**。同样是 `example`，**不进** `cargo test` 门禁。

```bash
export SOPHIA_LLM_API_KEY=<your-key>     # OpenAI 兼容模式需要
cargo run -p sophia-cli --example benchmark -- --list            # 列出题目（不需 key）
cargo run -p sophia-cli --example benchmark -- --task abs_difference   # 跑单题（两 mode）
cargo run -p sophia-cli --example benchmark -- --level l1         # 跑某分级
cargo run -p sophia-cli --example benchmark -- --mode sophia      # 只跑某 mode
cargo run -p sophia-cli --example benchmark -- --llm-mode ollama --mode sophia
```

`baseline` mode 执行 LLM 生成的 Python，需 `python3`（缺失时自动只跑 `sophia` mode）。产物落 `sophia-runs/benchmark/<label>/{runs.jsonl,summary.md}`（成功率 / 耗时两项核心指标）。批量串行执行见 `scripts/run_benchmark.sh`。详见 `docs/benchmark_design.md`。

> 凭证管理：可把 key 放进 `.secrets/llm.env`（已被 `.gitignore` 忽略），用 `set -a; source .secrets/llm.env; set +a` 载入，避免明文出现在命令历史。

## 修改语法（仅贡献者）

若改动 `core/syntax/grammar.js`，需用对齐版本的 Tree-sitter CLI 重新生成 parser：

```bash
# 需要 tree-sitter-cli 0.26.x（与 tree-sitter crate 0.26 + ABI 15 三者对齐）
cd core/syntax
tree-sitter generate --abi 15
```

版本对齐是硬约束（见 `docs/engineering_notes.md`）：tree-sitter crate、CLI、生成的 `parser.c` ABI 必须一致。

## 故障排查

- **链接器 / C 编译错误**：确认已安装 C 编译器（`cc --version` 或 `gcc --version`）。
- **`rustc` 版本过低**：`rustup update` 升级到 ≥ 1.80。
- **首次构建较慢**：`rusqlite` 的 `bundled` 特性会编译 SQLite，属正常一次性开销。
