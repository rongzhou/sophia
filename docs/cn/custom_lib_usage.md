# 三方库本地调试与生成

本文说明如何在项目内目录调试三方库发现、用本地 LLM 生成候选库代码，以及查看 Sophia workflow
中间产物。默认示例使用项目内 `sophia-runs/custom-lib-debug/`，该目录用于调试产物；用户也可以把
`DEBUG_ROOT` 改成任意项目根，或用 `SOPHIA_LIB_PATH` 指向额外三方库根。

---

## 一、目录约定

三方库默认放在被调试项目根的 `sophia_libs/` 下：

```text
<project-root>/
  sophia_libs/
    <libname>/
      library.toml
      <libname>.md
      src/*.sophia
      host.wasm        # 可选，仅 WASM-effect 三方库
```

本地调试建议在当前仓库内准备一个独立项目根：

```bash
export SOPHIA_REPO="$(pwd)"
export DEBUG_ROOT="$SOPHIA_REPO/sophia-runs/custom-lib-debug/json_project"
mkdir -p "$DEBUG_ROOT/sophia_libs"
```

如三方库不在项目内，可额外设置：

```bash
export SOPHIA_LIB_PATH="$SOPHIA_REPO/sophia-runs/custom-lib-debug/extra_libs"
```

CLI 生产命令会合并标准库、`$DEBUG_ROOT/sophia_libs/` 与 `$SOPHIA_LIB_PATH`。

---

## 二、调试三方库发现

可以先复制一个现有三方库 fixture 到调试项目根：

```bash
cp -R "$SOPHIA_REPO/stdlib/tests/fixtures/sophia_libs/json" \
  "$DEBUG_ROOT/sophia_libs/json"
```

检查库清单、库源码和用户项目是否能一起通过静态检查：

```bash
target/debug/sophia check "$DEBUG_ROOT"
```

运行库 action：

```bash
target/debug/sophia run --root "$DEBUG_ROOT" ValidateJson \
  --arg 'text:{"ok":true}'
```

查看执行 trace：

```bash
target/debug/sophia run --root "$DEBUG_ROOT" ValidateJson \
  --arg 'text:{"ok":true}' \
  --trace
```

---

## 三、用 workflow 生成候选库代码

初始化 Development Graph：

```bash
target/debug/sophia graph init --root "$DEBUG_ROOT"
```

创建业务目标。目标只写需求，不写 Sophia 类型、语言特征或答案：

```bash
target/debug/sophia graph start \
  "Design a JSON text validator" \
  --description "Create a small validator for JSON text. It should decide whether an input text is a valid JSON value, accept objects, arrays, strings, numbers, booleans, null, and surrounding whitespace, and return either success with the final position or failure with the error position and a short reason." \
  --root "$DEBUG_ROOT"
```

### 3.1 分步调试路径

先生成伪代码：

```bash
target/debug/sophia graph design N0001 \
  --root "$DEBUG_ROOT" \
  --mode ollama \
  --model qwen3.6:latest
```

成功后伪代码会写到：

```text
$DEBUG_ROOT/sophia-runs/graph/artifacts/<PseudoId>.pseudo
$DEBUG_ROOT/sophia-runs/graph/artifacts/<PseudoId>.libs   # 若 design 选择了库
```

再用伪代码实现候选代码：

```bash
target/debug/sophia graph implement-loop N0001 \
  --pseudo N0005 \
  --root "$DEBUG_ROOT" \
  --mode ollama \
  --model qwen3.6:latest \
  --call-timeout-secs 90 \
  --max-repairs 2
```

候选代码会写到：

```text
$DEBUG_ROOT/sophia-runs/graph/artifacts/<CodeId>/...
```

### 3.2 自动驱动路径

也可以让 LLM 每轮通过 DecisionNode 自主选择下一步动作。代码只提供候选动作与预算，是否拆解由 LLM
的 `selected_action` 决定：

```bash
target/debug/sophia graph drive N0001 \
  --root "$DEBUG_ROOT" \
  --mode ollama \
  --model qwen3.6:latest \
  --call-timeout-secs 90 \
  --auto-accept-decompositions \
  --max-decisions 4 \
  --max-depth 2 \
  --max-goals 8 \
  --max-repairs 1
```

`--auto-accept-decompositions` 表示调用方代表人类审查者接受 LLM 已经产生的拆解；它不替 LLM 选择
`decompose`。

`--call-timeout-secs` 是**单次 LLM 调用墙钟上限**。它不同于连接 / 响应读取空闲超时：即便后端持续
stream token、没有长期空闲，只要一次生成超过该上限仍会被中止，并以 LLM 后端失败路径落
`RawLlmNode`，供后续查看和重试。也可用环境变量：

```bash
export SOPHIA_LLM_CALL_TIMEOUT_SECS=90
```

设为 `0` 表示关闭单次调用墙钟上限；调试复杂目标时不建议关闭。

Ollama 后端的结构化步骤会把 workflow schema 作为请求 `format` 发送给后端；这能减少自由文本长篇
输出和 JSON 提取失败。若仍超时，优先检查对应 `RawLlmNode` 的 `operation`，确认卡在
`implement_design` 还是 `repair_code`。

---

## 四、查看中间产物

列出图节点：

```bash
target/debug/sophia graph nodes --root "$DEBUG_ROOT"
```

查看完整事件流：

```bash
sqlite3 "$DEBUG_ROOT/sophia-runs/graph/dev_graph.sqlite" \
  'select seq, payload from graph_events order by seq;'
```

只看 DecisionNode：

```bash
sqlite3 "$DEBUG_ROOT/sophia-runs/graph/dev_graph.sqlite" \
  "select seq, payload from graph_events where payload like '%\"payload_kind\":\"decision\"%' order by seq;"
```

只看诊断：

```bash
sqlite3 "$DEBUG_ROOT/sophia-runs/graph/dev_graph.sqlite" \
  "select seq, payload from graph_events where payload like '%\"payload_kind\":\"diagnostic\"%' order by seq;"
```

只看拆解：

```bash
sqlite3 "$DEBUG_ROOT/sophia-runs/graph/dev_graph.sqlite" \
  "select seq, payload from graph_events where payload like '%\"payload_kind\":\"decomposition\"%' order by seq;"
```

查看 artifacts：

```bash
find "$DEBUG_ROOT/sophia-runs/graph/artifacts" -maxdepth 8 -type f
```

查看某个伪代码：

```bash
sed -n '1,220p' "$DEBUG_ROOT/sophia-runs/graph/artifacts/N0005.pseudo"
```

查看某个候选 CodeNode 的全部文件：

```bash
find "$DEBUG_ROOT/sophia-runs/graph/artifacts/N0012" \
  -type f \
  -print \
  -exec sed -n '1,220p' {} \;
```

---

## 五、把候选代码整理成三方库

候选通过 `code_check` 后，可以整理到三方库目录：

```bash
mkdir -p "$DEBUG_ROOT/sophia_libs/json/src"
cp -R "$DEBUG_ROOT/sophia-runs/graph/artifacts/N0012/"* \
  "$DEBUG_ROOT/sophia_libs/json/src/"
```

然后补齐：

```text
$DEBUG_ROOT/sophia_libs/json/library.toml
$DEBUG_ROOT/sophia_libs/json/json.md
```

再验证：

```bash
target/debug/sophia check "$DEBUG_ROOT"
target/debug/sophia run --root "$DEBUG_ROOT" ValidateJson --arg 'text:{"ok":true}'
```

---

## 六、常见卡点

- **`PseudoCheck` 失败**：查看 DiagnosticNode；伪代码正文必须包含
  `<!-- sophia-pseudo: v1 -->`、`# Purpose`、`# Inputs`、`# Outputs`、`# Algorithm`、
  `# Constraints`、`# Forbidden`。
- **LLM 选择 `decompose`**：这是 DecisionNode 的结果，不是代码替它决定。用 `graph nodes` 和
  `graph_events` 查看 `Decision`、`Decomposition`、`AcceptanceEvent`。
- **实现阶段长时间不返回**：说明单次 LLM 生成尚未结束。优先加 `--call-timeout-secs 60` 或
  `--call-timeout-secs 90`，让该次调用诚实失败并落 `RawLlmNode`；再通过图节点确认卡在
  `snapshot:implement_design` 还是已经进入 `code_check` / `repair_code`。`--max-repairs` 只限制
  调用返回之后的修复轮数，不能中止正在生成的一次 LLM 调用。
- **三方库未被发现**：确认库位于 `$DEBUG_ROOT/sophia_libs/<libname>/`，且 `library.toml` 中
  `[library].name` 与目录名一致；项目外库确认 `SOPHIA_LIB_PATH` 指向的是库根目录，而不是单个库目录。
