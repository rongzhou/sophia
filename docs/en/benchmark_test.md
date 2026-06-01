# Sophia Benchmark Test Guide (benchmark test)

> The third of Sophia’s three test categories. Benchmarks compare, across multiple small programming tasks, “LLM directly writes Python” (`baseline` mode) versus the “Sophia workflow” (`sophia` mode) on two core metrics—success rate and wall time. They are examples (do not enter the `cargo test` gate; cleanly skip without an LLM key or without `python3`). This is a test guide: it clarifies what benchmarks measure, how to run them, how to organize them with discipline, and what tasks are included.

---

## I. Positioning

### 1.1 Questions to answer

Given the same task set, the same LLM, and the same set of hidden validation cases, compare two core metrics:

- Success rate: whether the code produced in a mode passes all hidden validation cases for that task (pass/fail; can aggregate pass ratio across multiple runs).
- Wall time: the real elapsed time from receiving the problem to producing a candidate that can be adjudicated.

We do not invent a third composite metric (no “intelligence score”); we only report these two and aggregate by task/mode.

### 1.2 Modes under comparison

| mode | Solution path | Artifact | Adjudication backend |
| --- | --- | --- | --- |
| `sophia` | Sophia workflow (design → implement → check → [repair] → candidate) | Sophia-Core source | v0 interpreter (reuse `runtime::run_action` / `runtime::verify`) |
| `baseline` | LLM writes a self-contained Python module directly | Python source | External `python3` subprocess |

`sophia` is the subject under test; `baseline` offers a reference of “the same model writes a mainstream language directly.” The language is a parameter of `baseline` (currently Python only; see §III.1), not a separate mode.

### 1.3 What not to test

- Not verifying the correctness of language components—that’s unit tests (see `docs/unit_test.md`).
- Not verifying that a real LLM loop runs end to end—that’s e2e (see `docs/e2e_test.md`).
- Not inventing new comparison metrics; only success rate + wall time.

### 1.4 Mocking policy: no mocks; pure-logic task set

Benchmarks in principle do not mock. They aim to compare true capabilities of both paths; mocks would mask errors and skew the comparison (real I/O is uncertain and unfair between modes). Therefore:

- The benchmark task set is all pure logic, no I/O, deterministic (L1→L6), so both modes are comparable under deterministic input→output semantics.
- Network/file tasks are excluded from benchmarks—their end-to-end validation with real I/O is done in e2e (see `e2e_test.md` G2-03 network / G5-01 file). This avoids the shortcut of “mocking to make tasks deterministic.”

### 1.5 Case selection philosophy: expressiveness ladder (unlike e2e’s coverage)

Benchmarks emphasize expressiveness: tasks are organized as a monotonically increasing difficulty ladder (L1→L6). Each level stacks new mechanisms/deeper combinations on top of lower-level capabilities. The goal is to reveal the capability divergence between the Sophia workflow and direct Python as difficulty rises. This differs from e2e’s coverage philosophy (spread across orthogonal capability dimensions as regression gates; see `e2e_test.md`).

The only shared base is the same generalizable, anti-cheating prompts + scaffolding (`sophia_syntax_baseline` + the same anti-leak discipline). Improvements to the base affect both; tasks are deliberately non-overlapping to avoid cross-contamination.

### 1.6 Task size constraints

Tasks are strictly constrained to what the v0 starting subset can express and the interpreter can execute: interpreter supports `And/Or/Eq/Ne/<,<=,>,>=`, binary `+ - *`, unary `-x` (no division/modulo). Going beyond would make `sophia` mode fail due to language not yet supporting it—that’s a language capability issue, not a workflow issue, and would pollute the comparison.

---

## II. Running

```bash
export SOPHIA_LLM_API_KEY=<key>          # Required for OpenAI-compatible mode; not persisted / not stored in graph / not printed
cargo run -p sophia-cli --example benchmark -- --list                    # List all tasks (no key required)
cargo run -p sophia-cli --example benchmark -- --task abs_difference      # Run a single task
cargo run -p sophia-cli --example benchmark -- --level L4                 # Run a level
cargo run -p sophia-cli --example benchmark -- --mode sophia --runs 3     # Run only a mode, 3 times each
cargo run -p sophia-cli --example benchmark -- --llm-mode ollama --mode sophia
```

Batch runner `scripts/run_benchmark.sh` launches each task in its own process; logs are saved (isomorphic to `run_e2e.sh`).

Args: `--task` (by id) / `--level` (L1–L6) / `--mode` (sophia | baseline) / `--runs` (per-task repetitions) / `--label` (artifact subdir, default `default`) / `--list`, and LLM backend args `--llm-mode` (openai | ollama) / `--llm-model` / `--llm-base-url` / `--llm-api-key`. Environment variables are the same as e2e: `SOPHIA_LLM_MODE`, `SOPHIA_LLM_MODEL`, `SOPHIA_LLM_BASE_URL`, `SOPHIA_LLM_TIMEOUT_SECS`, `SOPHIA_LLM_API_KEY`. In OpenAI-compatible mode, absence of `SOPHIA_LLM_API_KEY` leads to clean skipping; in Ollama mode the default base URL is `http://localhost:11434` and the default model is `qwen3.6:latest`, with no API key needed. `--llm-timeout-secs` / `SOPHIA_LLM_TIMEOUT_SECS` denote idle connection/response-stream timeouts. Both OpenAI-compatible and Ollama use streaming and do not cap total generation time. Ollama does not retry by default (to avoid repeated local generations after timeouts). If `python3` is missing, baseline mode is skipped (only sophia runs; `python3` is only a runtime external tool and not a Cargo dependency).

Execution of LLM-generated Python is protected by a hard timeout (5s per case), a restricted temporary working directory, and cleanup after use.

---

## III. Discipline

### 3.1 Baseline is Python only (project-inherent asymmetry in execution)

`baseline` must truly execute LLM-generated code, whereas the current workspace is pure Rust and does not depend on Python. This creates a real, project-inherent engineering asymmetry between `sophia` (adjudication reuses `runtime::verify`, with zero new execution capability) and `baseline` (subprocess execution from scratch + cross-language comparison). Hence baseline is Python only (`python3` has the lightest dependency and is almost universally available).

### 3.2 Anti-answer leakage (same origin as e2e)

- Goes into prompts (the problem): `prompt_goal` / entry contract (`entry`) / `public_forbidden`. In `sophia` mode we additionally inject the shared syntax baseline (implement/repair only; not in design) + on-demand standard library assets (the current task set is all pure logic; design chooses no libraries; zero injection). The implement phase also injects the current root objective as semantic context to anchor naming/intent to the problem statement.
- Never goes into prompts (answers): the entirety of `hidden_cases`, baseline runner fixtures, source snippets/implementation hints.
- Structural defenses: `Problem::public_brief()` exposes only public fields at the type level so that prompt-assembly functions cannot see `hidden_cases` (enforced by function signatures, not self-discipline). Anti-leak assertions guard that shared assets contain no task-domain tokens.

### 3.3 Symmetry in cross-language adjudication (error identity)

Sophia’s errors are two-level structures (`error <type> { variant <variant> }`), and hidden cases adjudicate by variant name (`raise OverLimit` matches `Raises("OverLimit")`). Python exceptions are single-level. Alignment rule (anti-cheating, generalizable): when the problem describes errors as a two-level structure with variants, the Python exception class name must take the most specific level—the variant name (the variant name is public in the problem, so it doesn’t leak hidden cases). For fallible returns (`one of`), the failure member is adjudicated symmetrically across modes by a `{variant, fields}` object (baseline contract: on failure, return that dict instead of raising). No dual-acceptance fallback (consistent with the single path).

### 3.4 Honest adjudication

Adjudication never fabricates results: `sophia` reuses `runtime::run_hidden_cases` (actually runs the interpreter and compares against `ExpectedOutcome`); for `baseline`, hard errors compiling/executing the subprocess count as failure with the reason recorded; timeouts also count as failure.

---

## IV. Task ladder

The task set is a monotonically increasing difficulty ladder (each level accumulates mechanisms). Each task provides: scenario, stacked capabilities, entry, and hidden-case highlights. Only the problems are described; no answers are included. Hidden cases are the answers and never go into prompts.

### L1 Floor: pure scalar logic

| id | Scenario | Stacked capabilities | Entry | Hidden-case highlights |
| --- | --- | --- | --- | --- |
| `abs_difference` | Absolute difference of two numbers | Int, arithmetic (incl. unary negation), pure function | `AbsDifference(left, right)` | `(9,2)→7`; `(2,9)→7`; `(5,5)→0` |
| `within_budget` | Budget check | Bool, comparison (`<=`) | `WithinBudget(spent, budget)` | `(80,100)→true`; `(100,100)→true`; `(120,100)→false` |

### L2 + structural modeling

| id | Scenario | Stacked capabilities | Entry | Hidden-case highlights |
| --- | --- | --- | --- | --- |
| `rectangle_area` | Rectangle area | Entity with multiple fields + field access | `RectangleArea(rect)` | `6×6→36`; `10×3→30`; `1×1→1` |
| `traffic_next` | Next traffic light state | State with multiple values + exhaustive `match` | `NextLight(current)` | `Red→Green`; `Green→Yellow`; `Yellow→Red` |

### L3 + cross-action composition

| id | Scenario | Stacked capabilities | Entry | Hidden-case highlights |
| --- | --- | --- | --- | --- |
| `discounted_total` | Discounted total | One action calls another | `NetTotal(unit_price, quantity, discount)` | `(10,5,8)→42`; `(7,3,0)→21`; `(100,1,1)→99` |

### L4 + error algebra

| id | Scenario | Stacked capabilities | Entry | Hidden-case highlights |
| --- | --- | --- | --- | --- |
| `checked_subtract` | Checked subtraction | Error variant + `raise` + conditional branching | `RemoveStock(available, requested)` | `(50,8)→42`; `(8,8)→0`; `(5,12)→raise Insufficient` |

### L5 Combine all mechanisms

| id | Scenario | Stacked capabilities | Entry | Hidden-case highlights |
| --- | --- | --- | --- | --- |
| `checkout_limit` | Checkout limit validation | Entity parameter + cross-call (`LineAmount`) + error algebra + scalar arithmetic all-in-one | `Checkout(line, credit_limit)` | `6×7=42, limit 100 → 42`; `5×5=25, limit 25 → 25` (boundary); `9×4=36, limit 30 → raise OverLimit` |

L5 is the ladder’s top-level combination: stacking L1–L4 mechanisms into one task; divergence in expressiveness is most likely to appear here.

### L6 Fallible return `one of`

| id | Scenario | Stacked capabilities | Entry | Hidden-case highlights |
| --- | --- | --- | --- | --- |
| `clamp_or_reject` | Restricted value | `one of { Int, OutOfRange }`: failure is a return value (recoverable), unlike L4’s `raise` (unrecoverable interruption) | `ClampOrReject(n, limit)` | `(3,10)→3`; `(0,10)→0` (lower bound); `(10,10)→10` (upper bound); `(15,10)→ return OutOfRange{value:15}` (not a raise) |

L6 is the top of the “fallible modeling” dimension after F1 (fallible returns via `one of`): the caller must match both the success member (bare Int) and the failure member (OutOfRange). Failure is a value and can be further processed by the caller—this is a capability uplift vs v0.

> L6 keeps only the pure-logic task `clamp_or_reject`. Network/file fallible modeling is not benchmarked (no mocks; real I/O would be uncertain/unfair); it is moved to e2e with real I/O (`e2e_test.md` G2-03 / G5-01).

---

## V. Adjudication and timing

### 5.1 Adjudication (pass = all hidden cases passed)

- `sophia`: reuse `runtime::verify::run_hidden_cases`—each `HiddenCase` is actually executed on the v0 interpreter and compared with `ExpectedOutcome`; benchmarks add zero new execution capability. Failure members of fallible returns are `Value::ErrorValue` and are compared for value equality with the expected error member.
- `baseline`: the candidate module is written to a restricted temp directory, and a benchmark-owned deterministic runner script (not LLM-generated) imports it, invokes the entry function `run_action(input)` per case, and prints either `{"ok":true,"result":...}` or `{"ok":false,"error":"<ClassName>"}` to stdout. The Rust side compares via a unified `Value ↔ JSON` contract (`Returns` compares JSON values; `Raises` compares exception class names; fallible `one of` members compare `{variant, fields}` dicts).

### 5.2 Timing (fair only when measured uniformly across modes)

`wall_time` = LLM + workflow wall time from receiving the problem to producing an adjudicable candidate (`sophia` = design + implement + [repair] + deterministic check; `baseline` = a single LLM call with self-check retries). The adjudication execution itself (interpreter/subprocess runs of hidden cases) is excluded from `wall_time`. Timing reflects real LLM + network; cross-run variance is normal. It is for comparison only and is not part of any deterministic assertions/snapshots.

---

## VI. Artifacts

- Per-run records: one structured record per (task, mode, run index)—`id` / `level` / `mode` / `language` (baseline is `python`; sophia is `null`) / `model` / `passed` / `wall_time_ms` / `failure` (null on success) / per-hidden-case `passed` details. JSON Lines saved to `sophia-runs/benchmark/<label>/runs.jsonl` (append-only; gitignored).
- Aggregated report: a per-(task × mode) table of success rate and average time, printed to stdout and saved as `summary.md`. Columns are precisely the core metrics: `level | task | mode | runs | passed | success_rate | avg_wall_time_ms`.

---

## VII. Engineering structure

Implementation is under `cli/examples/benchmark/` (a multi-file example symmetric to e2e, not part of the `cargo test` gate):

```
cli/examples/benchmark/
├── main.rs          ← Entry: arg parsing; mode selection; clean skip when no key / no python3; drive per (task×mode×run); summarize
├── problem.rs       ← Problem / EntrySig / Param / NeutralTy / Level / PublicBrief (public_brief() isolates hidden_cases at the type level; structural defense)
├── problems.rs      ← Task set L1→L6 difficulty ladder (all-new tasks; cumulative mechanisms; reuses runtime::Value / HiddenCase)
├── value_json.rs    ← value_to_json: runtime::Value → language-neutral JSON contract (shared by both modes)
├── retry.rs         ← Bounded-retry client wrapper (tolerates transient public-network jitters; isomorphic to e2e; intentionally not shared)
├── sophia_mode.rs   ← sophia mode: compact closed loop (design→implement-loop→runtime::verify); does not reuse the e2e harness (no premature abstraction); discipline consistent with e2e (same syntax-baseline asset)
├── baseline_py.rs   ← baseline (Python) mode: structured generation + python3 subprocess runner fixture + restricted temp dir + hard timeout + Value↔JSON mapping + attribution
└── report.rs        ← RunRecord / Mode / runs.jsonl writer / summary renderer

scripts/run_benchmark.sh   ← Sequential batch runner (one process per task; logs saved; isomorphic to run_e2e.sh)
```

Design discipline: While the `sophia` mode’s closed loop overlaps with the e2e harness, keep them separate for now—do not pre-abstract (YAGNI; under a single path, accept some small duplication rather than introducing abstractions for reuse). Adding a task = add a `Problem` in `problems.rs` and register it in `all_problems()`; if introducing new domain terms, register tokens in the anti-leak assertion in `render.rs`.
