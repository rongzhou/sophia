#!/usr/bin/env bash
#
# Sophia v0 e2e 真实 LLM 测试：串行批量执行器（手动离线运行，见 docs/e2e_test_design.md）。
#
# 为什么串行：真实 LLM 调用慢且公网端点偶发抖动；逐个执行（一次仅一个用例）便于观察、
# 隔离失败、避免并发放大不稳定。每个用例的完整输出落盘到日志目录，最后打印汇总。
#
# 用法：
#   export SOPHIA_LLM_API_KEY=<key>          # OpenAI 兼容模式需要；不落盘 / 不进图
#   export SOPHIA_LLM_MODE=ollama            # 使用本地 Ollama 时无需 API key
#   scripts/run_e2e.sh                       # 串行跑全部用例
#   scripts/run_e2e.sh g1                    # 只跑某组（g1 / g2 / r / ...）
#   scripts/run_e2e.sh --cases G1-01 G2-02   # 只跑指定用例
#
# 可选环境变量（透传给 example）：
#   SOPHIA_LLM_MODE / SOPHIA_LLM_MODEL / SOPHIA_LLM_BASE_URL / SOPHIA_LLM_MAX_REPAIRS
#
# 退出码：全部用例通过为 0，否则为 1。

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

LLM_MODE="$(echo "${SOPHIA_LLM_MODE:-openai}" | tr '[:upper:]' '[:lower:]')"
if [[ "$LLM_MODE" != "ollama" && -z "${SOPHIA_LLM_API_KEY:-}" ]]; then
  echo "未设置 SOPHIA_LLM_API_KEY，无法运行真实 LLM e2e 测试。" >&2
  echo "  export SOPHIA_LLM_API_KEY=<key>" >&2
  echo "  或使用本地 Ollama：export SOPHIA_LLM_MODE=ollama" >&2
  exit 1
fi

LOG_DIR="${SOPHIA_E2E_LOG_DIR:-sophia-runs/e2e-logs}"
mkdir -p "$LOG_DIR"
STAMP="$(date +%Y%m%d_%H%M%S)"

# 先编译一次，避免每个用例重复编译刷屏。
echo "==> 预编译 e2e example…"
if ! cargo build -q -p sophia-cli --example e2e; then
  echo "编译失败，终止。" >&2
  exit 1
fi

# 解析要跑哪些用例 ID。
declare -a CASE_IDS
if [[ "${1:-}" == "--cases" ]]; then
  shift
  CASE_IDS=("$@")
else
  GROUP_FILTER="${1:-}"
  # 用 --list 枚举全部用例（每行：<ID> <标题>）。
  while read -r id _rest; do
    [[ -z "$id" ]] && continue
    if [[ -n "$GROUP_FILTER" ]]; then
      # 组前缀匹配：g1 → G1-*，r → R-*（大小写不敏感）。
      prefix="$(echo "$GROUP_FILTER" | tr '[:lower:]' '[:upper:]')-"
      [[ "$id" == "$prefix"* ]] || continue
    fi
    CASE_IDS+=("$id")
  done < <(cargo run -q -p sophia-cli --example e2e -- --list)
fi

if [[ "${#CASE_IDS[@]}" -eq 0 ]]; then
  echo "没有匹配的用例。" >&2
  exit 1
fi

echo "==> 将串行执行 ${#CASE_IDS[@]} 个用例：${CASE_IDS[*]}"
echo "==> 日志目录：$LOG_DIR"
echo

declare -a PASSED FAILED
for id in "${CASE_IDS[@]}"; do
  log="$LOG_DIR/${STAMP}_${id}.log"
  printf "──── %-8s 运行中… " "$id"
  if cargo run -q -p sophia-cli --example e2e -- --case "$id" >"$log" 2>&1; then
    echo "PASS  (日志：$log)"
    PASSED+=("$id")
  else
    echo "FAIL  (日志：$log)"
    FAILED+=("$id")
  fi
done

echo
echo "════════ 汇总：${#PASSED[@]}/${#CASE_IDS[@]} 通过 ════════"
for id in "${PASSED[@]}"; do echo "  ✓ $id"; done
for id in "${FAILED[@]:-}"; do [[ -n "$id" ]] && echo "  ✗ $id"; done

[[ "${#FAILED[@]}" -eq 0 ]]
