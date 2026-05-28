# Benchmark Run Commands

This project treats `experiment run --task <task.json>` as the source of each benchmark result. Suite commands are serial convenience wrappers: they load a suite, run each task through the same single-task path, write JSONL records, and continue after failures so the full regression surface is visible.

## Current Benchmark Layout

- `benchmarks/L1`: linear tasks without loops or branches.
- `benchmarks/L2`: single-loop, list, and effect tasks.
- `benchmarks/L3`: branch and `match` tasks, including effect, pure-return, `Optional`, and `state` variants.
- `benchmarks/L4`: goal-workflow translations captured as single-task contracts.
- `benchmarks/L5`: change-application tasks captured as single-task contracts.
- `benchmarks/category_a`: cross-action / entity pipeline tasks.

List registered tasks:

```bash
node dist/cli/main.js experiment list --suite benchmarks
```

## Single Task

Use this when debugging a failure or producing a paper-grade task record:

```bash
node dist/cli/main.js experiment run \
  --task benchmarks/L2/rabbit_ten/task.json \
  --model qwen3.6:latest \
  --mode full \
  --max-design-revisions 2 \
  --max-repairs 2 \
  --ollama-timeout-ms 900000 \
  --out sophia-runs/results/rabbit-ten-full.jsonl
```

Summarize one or more JSONL files:

```bash
node dist/cli/main.js experiment summarize \
  --inputs sophia-runs/results/rabbit-ten-full.jsonl
```

## Serial Suite

Run all benchmarks under `benchmarks/`:

```bash
node dist/cli/main.js experiment run-suite \
  --suite benchmarks \
  --model qwen3.6:latest \
  --mode full \
  --max-design-revisions 2 \
  --max-repairs 2 \
  --ollama-timeout-ms 900000 \
  --out-dir sophia-runs/results/all-benchmarks-full
```

The suite runner writes:

- `results.jsonl`: one record per task run, including `design_revisions_used`, `repairs_used`, workspace paths, and hidden verification status.
- `summary.md`: the same summary table produced by `experiment summarize`.

By default, `experiment run` and `experiment run-suite` overwrite target JSONL files to avoid historical contamination. Pass `--append` explicitly to append.

If any task fails, the command exits non-zero after attempting the full suite.

## Direct TypeScript Baseline

Run the same suite in the direct TypeScript baseline mode:

```bash
node dist/cli/main.js experiment run-suite \
  --suite benchmarks \
  --model qwen3.6:latest \
  --mode direct-ts \
  --ollama-timeout-ms 900000 \
  --out-dir sophia-runs/results/all-benchmarks-direct-ts
```

## 2026-05-26 Notes

Task counts evolve as benchmarks are consolidated. Use `experiment list --suite benchmarks` to view the current suite, and `experiment summarize` to compare modes. These numbers indicate implementation and workflow health signals, not the central project claim. The central claim is that programming ability need not be entirely internalized through code pretraining, and that an LLM-native graph language plus heuristic node workflow is viable.

Direct-ts failures:

- `build_three_numbers`: typechecked and ran, but returned four numbers instead of three.
- `optional_label_default`: typechecked and ran, but confused present optional text with missing text.

Sophia full average wall time was higher because the full workflow includes design, pseudocode checking, implementation, deterministic Sophia checks, audit, TypeScript lowering/typecheck, and hidden verification.
