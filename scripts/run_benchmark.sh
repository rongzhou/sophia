#!/usr/bin/env bash
#
# Sophia 编程能力基准测试：串行批量执行器（手动离线运行，见 docs/benchmark_design.md）。
#
# 为什么串行：真实 LLM 调用慢且公网端点偶发抖动；逐题执行（一次一题）便于观察、隔离失败、
# 避免并发放大不稳定。每题的完整输出落盘到日志目录，最后打印汇总。
#
# 与 run_e2e.sh 同构（刻意不抽象共享）：benchmark 与 e2e 是两套独立入口、独立产物。
#
# 用法：
#   export SOPHIA_LLM_API_KEY=<key>            # OpenAI 兼容模式需要；不落盘 / 不进图
#   export SOPHIA_LLM_MODE=ollama              # 使用本地 Ollama 时无需 API key
#   scripts/run_benchmark.sh                   # 串行跑全部题 × 两 mode
#   scripts/run_benchmark.sh --level l1        # 只跑某分级（l1 / l2 / l3 / l4）
#   scripts/run_benchmark.sh --tasks abs_difference safe_divide   # 只跑指定题
#   scripts/run_benchmark.sh --mode sophia     # 只跑某 mode（sophia / baseline）
#
# 可选环境变量（透传给 example）：SOPHIA_LLM_MODE / SOPHIA_LLM_MODEL / SOPHIA_LLM_BASE_URL。
# baseline mode 需要 python3；缺失时 example 自动只跑 sophia。
#
# 退出码：example 全部成功执行为 0（benchmark 是度量而非门禁，失败题如实记录在产物里）。

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

LLM_MODE="$(echo "${SOPHIA_LLM_MODE:-openai}" | tr '[:upper:]' '[:lower:]')"
if [[ "$LLM_MODE" != "ollama" && -z "${SOPHIA_LLM_API_KEY:-}" ]]; then
  echo "未设置 SOPHIA_LLM_API_KEY，无法运行真实 LLM 基准测试。" >&2
  echo "  export SOPHIA_LLM_API_KEY=<key>   # 或 set -a; source .secrets/llm.env; set +a" >&2
  echo "  或使用本地 Ollama：export SOPHIA_LLM_MODE=ollama" >&2
  exit 1
fi

LOG_DIR="${SOPHIA_BENCH_LOG_DIR:-sophia-runs/benchmark-logs}"
mkdir -p "$LOG_DIR"
STAMP="$(date +%Y%m%d_%H%M%S)"

# 透传给 example 的 mode 过滤（默认两 mode 都跑）。
MODE_ARGS=()

# 先编译一次，避免每题重复编译刷屏。
echo "==> 预编译 benchmark example…"
if ! cargo build -q -p sophia-cli --example benchmark; then
  echo "编译失败，终止。" >&2
  exit 1
fi

# 解析要跑哪些题 id。
declare -a TASK_IDS
case "${1:-}" in
  --tasks)
    shift
    TASK_IDS=("$@")
    ;;
  --level)
    LEVEL="${2:-}"
    [[ -z "$LEVEL" ]] && { echo "--level 需要一个分级（如 l1）" >&2; exit 1; }
    while read -r id _rest; do
      [[ -z "$id" ]] && continue
      TASK_IDS+=("$id")
    done < <(cargo run -q -p sophia-cli --example benchmark -- --level "$LEVEL" --list)
    ;;
  --mode)
    MODE="${2:-}"
    [[ -z "$MODE" ]] && { echo "--mode 需要 sophia 或 baseline" >&2; exit 1; }
    MODE_ARGS=(--mode "$MODE")
    while read -r id _rest; do
      [[ -z "$id" ]] && continue
      TASK_IDS+=("$id")
    done < <(cargo run -q -p sophia-cli --example benchmark -- --list)
    ;;
  "")
    # 全部题：用 --list 枚举（每行：<id> [级别] <标题>）。
    while read -r id _rest; do
      [[ -z "$id" ]] && continue
      TASK_IDS+=("$id")
    done < <(cargo run -q -p sophia-cli --example benchmark -- --list)
    ;;
  *)
    echo "未知参数：${1}（用法见脚本头部注释）" >&2
    exit 1
    ;;
esac

if [[ "${#TASK_IDS[@]}" -eq 0 ]]; then
  echo "没有匹配的题目。" >&2
  exit 1
fi

echo "==> 将串行执行 ${#TASK_IDS[@]} 道题：${TASK_IDS[*]}"
echo "==> 日志目录：$LOG_DIR"
echo "==> 产物：sophia-runs/benchmark/<label>/{runs.jsonl,summary.md}"
echo

declare -a OK ERR
for id in "${TASK_IDS[@]}"; do
  log="$LOG_DIR/${STAMP}_${id}.log"
  printf "──── %-20s 运行中… " "$id"
  # 每题用题 id 作 --label，产物各自归档；example 退出码非 0 视为执行错误（非"题没做对"）。
  if cargo run -q -p sophia-cli --example benchmark -- \
      --task "$id" --label "$id" "${MODE_ARGS[@]}" >"$log" 2>&1; then
    echo "DONE  (日志：$log)"
    OK+=("$id")
  else
    echo "ERROR (日志：$log)"
    ERR+=("$id")
  fi
done

echo
echo "════════ 执行完成：${#OK[@]}/${#TASK_IDS[@]} 题正常执行 ════════"
echo "（成功率 / 耗时见各题 sophia-runs/benchmark/<题id>/summary.md）"
for id in "${ERR[@]:-}"; do [[ -n "$id" ]] && echo "  ✗ 执行错误：$id"; done

[[ "${#ERR[@]}" -eq 0 ]]
