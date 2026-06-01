# Sophia End-to-End Test Guide (e2e test)

> The second of Sophia’s three test categories. e2e tests verify that the complete v0 workflow loop is usable end to end under a real LLM + real I/O. They are examples (do not enter the `cargo test` gate and cleanly skip without an API key), run manually or on schedule as needed. This is a test guide: it clarifies what e2e tests cover, how to run them, how to organize them with discipline, and the cases included.

---

## I. Positioning

### 1.1 What to test

e2e verifies that the following loop runs through end to end under a real LLM:

```
Human Objective (Objective + acceptance criteria)
   → design_solution (real LLM)        → semantic pseudocode
   → implement_design (real LLM)       → candidate .sophia
   → code_check (real tools/check)     → diagnostics
   → [repair (real LLM) ⟲ within budget] → repaired candidate
   → v0 interpreter (sophia_runtime)   → compare against expected outcome
```

Each case has a clear executable success criterion (check passes + interpreter’s return/raise/console matches expectations) but allows cross-run LLM phrasing variance (no verbatim assertions).

### 1.2 What not to test

- Not testing the LLM’s “intelligence ceiling”: case sizes stay within what the v0 starting subset can express and the interpreter can execute.
- Not a replacement for unit tests: correctness of checker/interpreter/graph invariants is guaranteed by each crate’s unit tests (see `docs/unit_test.md`); e2e only validates that “wiring them together + real LLM + real I/O” works.
- Not comparing success rate/time with Python—that’s benchmark (see `docs/benchmark_test.md`).

### 1.3 Mocking policy: no mocks; always real I/O

e2e does not allow mocking. Its goal is to validate real behavior; mocks would mask errors.

- Real LLM: true calls against an OpenAI-compatible endpoint (cleanly skip without a key; do not fabricate responses).
- Real network: cases needing `Http.Get` hit a stable public site (e.g., `example.com`), executed via the CLI’s real `CliHost` (`reqwest`), no mocks.
- Real files: cases needing `File.Read`/`File.Write` read/write real temporary files (OS temp dir), executed via `CliHost` (`std::fs`), not in-memory buckets.

> The harness injects the real host automatically per the entry action’s declared effects: if the entry declares `Http.Get` / `File.Read` / `File.Write`, use the real `CliHost`; otherwise (pure logic / `Console.Write`), use the default in-memory host. Real-host failures return `Err` and block, counted as failures as-is—never fabricate success.
>
> The harness’s LLM driver runs under a Tokio async shell; ultimately the v0 interpreter and the real File/Http hosts still keep a synchronous contract. Executions that require real I/O are put into Tokio blocking threads to run to completion and drop cleanly, avoiding `reqwest::blocking` tearing down its internal runtime within an async context.

### 1.4 Case selection philosophy: coverage (different from benchmark’s expressiveness ladder)

e2e emphasizes coverage—grouped by orthogonal capability dimensions (syntax/effects/heuristics/error algebra/File/goal tree). Each group nails a capability as a correctness/regression gate. This differs from benchmark’s expressiveness ladder (monotonically increasing difficulty to expose divergence from Python; see `benchmark_test.md`). The only shared base is the same generalizable, anti-cheating prompts + scaffolding (`sophia_syntax_baseline` + anti-leak discipline). Cases are deliberately non-overlapping.

---

## II. Running

```bash
export SOPHIA_LLM_API_KEY=<key>          # Required for OpenAI-compatible mode; not persisted / not stored in graph / not printed
cargo run -p sophia-cli --example e2e -- --list         # List all case IDs (no key required)
cargo run -p sophia-cli --example e2e -- --case G1-02   # Run a single case (preferred)
cargo run -p sophia-cli --example e2e -- --group g2     # Run a group (sequential within a single process)
cargo run -p sophia-cli --example e2e -- --llm-mode ollama --case G1-02
```

Batch runner `scripts/run_e2e.sh` runs each case in its own process sequentially; outputs are saved under `sophia-runs/e2e-logs/`:

```bash
scripts/run_e2e.sh                       # Run all sequentially
scripts/run_e2e.sh g1                    # Run a specific group
scripts/run_e2e.sh --cases G1-01 G2-02   # Run specified cases only
```

Environment variables:
- `SOPHIA_LLM_MODE` (`openai` / `ollama`, default `openai`)
- `SOPHIA_LLM_MODEL` (OpenAI default `deepseek-ai/deepseek-v4-flash`; Ollama default `qwen3.6:latest`)
- `SOPHIA_LLM_BASE_URL` (OpenAI default is an NVIDIA OpenAI-compatible endpoint; Ollama default `http://localhost:11434`)
- `SOPHIA_LLM_TIMEOUT_SECS` (idle response read timeout; OpenAI default 120; Ollama default 300)
- `SOPHIA_LLM_MAX_REPAIRS` (default 0 = require one-shot success; R-class cases set a positive budget)
- `SOPHIA_E2E_LOG_DIR` (batch script log dir, default `sophia-runs/e2e-logs`)

You may also override via args: `--llm-mode` / `--llm-model` / `--llm-base-url` / `--llm-api-key` / `--llm-timeout-secs`. In OpenAI-compatible mode, if `SOPHIA_LLM_API_KEY` is not set, the example cleanly skips and exits successfully (CI-safe). Ollama mode needs no API key. The batch script errors out in OpenAI mode without a key (its premise is to truly run), while Ollama mode does not check a key. Both OpenAI-compatible and Ollama use streaming; the timeout semantics are “connection/response stream idle too long,” not a cap on total generation time. The OpenAI-compatible remote has bounded retries by default; Ollama does not retry by default to avoid duplicating local generations after a timeout.

---

## III. Discipline

### 3.1 Anti-answer leakage (first principle)

May give to the LLM: task requirements (objective/description/acceptance criteria, including the task’s own domain terms—this is the problem, not the answer), generalizable language facts (the shared syntax baseline `sophia_syntax_baseline`, containing only standard syntax + neutral examples), real diagnostics (from `tools/check`, “what’s wrong,” not “what to change to”), and name-fidelity rules (explicit names given in the problem statement must be used verbatim).

Must not give to the LLM: source code/snippets of the target program, implementation hints tailored to the specific task, or anything that degenerates the case into copy-paste.

Structural defenses (not relying on self-discipline): (i) the syntax baseline is a single shared asset guarded by snapshots + anti-leak assertion tests (asserting that the baseline contains no task tokens; see `workflow/prompt/tests/render.rs`)—adding new cases with new domain terms must register those tokens in that assertion; (ii) a case’s “expected outcomes” and “defective seed candidates” exist only inside the harness and are not fed to the LLM; (iii) the design phase does not inject the syntax baseline (pseudocode semantics > format); the baseline is only injected into implement/repair.

### 3.2 Two-stage, on-demand injection of standard library assets

During design, inject the library catalog (`stdlib_catalog`, one-line purpose per library); the LLM chooses libraries in the pseudocode `libraries` field. During implement/repair, inject the full library descriptions per selection via `stdlib_preamble(libraries)` (e.g., `["http"]` → `assets/stdlib/http.md`). Library selection is the LLM’s design decision and is not predeclared in case metadata (predeclaration would leak the solution). Library assets follow the same snapshots + anti-leak discipline as the baseline. See `docs/stdlib_design.md`.

### 3.3 All intermediates stay in memory

The harness uses `GraphStore::open_in_memory`; candidate `.sophia` contents are in-memory `Vec<(path, contents)>`; nothing is written to the filesystem during the workflow; the only visible form is stdout (saved to text logs by `run_e2e.sh`). This makes each case self-contained and leaves no residue. (By contrast, the CLI `graph` path writes `.pseudo`/candidates to `sophia-runs/graph/artifacts/`.)

> Note: §3.3 concerns workflow intermediates (graph/pseudocode/candidate source) staying in memory; executions of cases that need real I/O (G2-03 network, G5-01 file) still hit real sites/real temp files (§1.3). These are not contradictory.

---

## IV. Case inventory

Cases are grouped by capability dimensions. Each gives: scenario, focus points, entry + arguments, success criteria. Only the problem and criteria are described; no Sophia source code answers are included.

### G1 Basic syntax / pure logic (4/4 one-shot)

| ID | Scenario | Focus | Entry / Args | Success Criteria |
| --- | --- | --- | --- | --- |
| G1-01 | Increment integer counter | Single action, Int, arithmetic, pure function | `IncrementCounter(41)` | Returns `42` |
| G1-02 | Mark todo as complete | State (multi-value) + return of a state value | `CompleteTodo(TodoStatus.Pending)` | Returns `TodoStatus.Done` |
| G1-03 | Cart line total | Entity (multi-field), field access, integer multiplication | `LineTotal(CartItem{unit_price=7,quantity=6})` | Returns `42` |
| G1-04 | Free shipping eligibility | Bool logic, comparisons (`>=`) | `QualifiesForFreeShipping(150)` | Returns `true` |

G1 are all “one-shot, zero-repair” (`max_repairs=0`), testing whether a model can produce usable code directly under good scaffolding.

### G2 effects + capabilities (includes real network)

| ID | Scenario | Focus | Entry / Args | Success Criteria |
| --- | --- | --- | --- | --- |
| G2-01 | Audit logging | `Console.Write` effect + capability binding, intent boundaries (`Sanitized<Text>`), `.length` | `LogNotice(Sanitized "hello")` | Returns `5` and console = `["hello"]` |
| G2-02 | Broadcast two-line notice | Sequential multiple `Console.Write` calls; effect declared only once | `Broadcast()` | Returns `2` and console = `["hello","bye"]` |
| G2-03 | Network fetch + intent safety | `Http.Get` effect + capability, intent boundaries (`Raw<Text>` converted to `Sanitized<Text>` via `intent_conversion`), real network | `FetchNonEmpty("https://example.com")` | Returns `true` (fetched trusted text is non-empty) |

- G2-01/G2-02 validate both the return values and the console output (verifying that effects truly execute via the interpreter’s effect host). `Console.Write` only accepts literals / `Sanitized<T>` / `Redacted<T>` (intent boundary), hence “print input text” style cases model input as `Sanitized<Text>`—this is a requirement constraint, not an implementation hint.
- G2-03 is the flagship LLM-native demo: the `Raw<Text>` from `Http.Get` is untrusted and must be explicitly converted via `intent_conversion` to `Sanitized<Text>`; otherwise it is statically rejected. The harness injects a real `CliHost` (`reqwest`) to actually hit `example.com` (IANA’s stable example domain). The assertion checks a stable property—“trusted fetched text is non-empty → return Bool true”—rather than exact length (real response length is unstable), avoiding brittle assertions. The reject half (using unconverted text directly → static rejection) is deterministically nailed down by the unit test `cli/tests/intent_matrix.rs` (see `unit_test.md`).

### G3 Heuristic node processing

| ID | Scenario | Focus | Entry / Args | Success Criteria |
| --- | --- | --- | --- | --- |
| G3-01 | Deduct stock | Through the scheduler `run_goal_loop`: LLM autonomously advances decision→design→implement to reach a candidate | `DeductStock(50, 8)` | Returns `42` |

G3 uses `CaseKind::Scheduler`: the harness does not hardcode the design→implement order, leaving action selection to the LLM. Each decision prompt is rendered at call time from the current active context + progress (`GoalProgress`)—the LLM autonomously advances in multiple steps.

### G4 Complex programs (capability combinations, not difficulty per se)

| ID | Scenario | Focus | Entry / Args | Success Criteria |
| --- | --- | --- | --- | --- |
| G4-01 | Order total (cross-action call) | One action calls another (via Execution Graph call edge) | `OrderTotal(7, 5, 7)` | Returns `42` |
| G4-02 | Withdrawal validation (error algebra) | error declarations + `errors` + `raise`: illegal input raises a domain error (unrecoverable interruption) | `Withdraw(30, 50)` | raises `InsufficientFunds` |
| G4-03 | Restricted value (fallible return via `one of`) | `one of { Int, OutOfRange }`: failure is a return value (recoverable); caller must match both paths | `ClampOrReject(15, 10)` | Returns `OutOfRange{value:15}` (not a raise) |

- G4-01 verifies cross-action calls routed via Execution Graph call edges.
- G4-02 vs G4-03 contrast two error-handling paradigms: G4-02 uses `raise` (unrecoverable/interruption); success criterion `Expect::Raises` checks the raised variant. G4-03 uses `one of` to return a failure member (recoverable/failure is a value); success criterion `Expect::Returns(ErrorValue{...})` checks the returned failure outcome. G4-03 is the core capability uplift vs v0 from F1 (`one of` fallible returns).

### G5 Standard library `File` (real file I/O)

| ID | Scenario | Focus | Entry / Args | Success Criteria |
| --- | --- | --- | --- | --- |
| G5-01 | Write and read back a note | `File.Write` + `File.Read` effects + capabilities, intent boundaries (`File.Read`’s `Raw<Text>` converted via `intent_conversion` to `Sanitized<Text>`; `File.Write` only accepts `Sanitized<Text>`), real temp files | `StoreNote(<temp-path>, Sanitized "hello")` | Returns `5` (length after write-then-read) |

G5-01 is self-contained (`File.Write(path, message)` → `File.Read(path)` → intent conversion → return length, a write→read round trip). The harness injects a real `CliHost`; the path uses a per-process fixed name in `std::env::temp_dir()`, hitting real temp files (not in-memory mock buckets). It examines the `File` library’s local file read/write + intent boundaries (isomorphic to G2-03’s network intent chain, but with an added write boundary: `File.Write` only accepts `Sanitized<Text>`). The `File` library’s syntax/intent boundaries are carried by on-demand assets `assets/stdlib/file.md` and are not part of the resident baseline. See `docs/file_lib.md`.

### G6 Goal tree traversal (decompose)

| ID | Scenario | Focus | Entry / Args | Success Criteria |
| --- | --- | --- | --- | --- |
| G6-01 | Convert two independent readings on a thermostat panel | LLM autonomously decomposes the root objective into two named action subgoals + human-authorization checkpoints + binding inheritance; each subgoal proceeds on its own | `CelsiusToScaled(21)` | Returns `42` |

G6 is driven by the goal-tree traversal layer `run_goal_tree` (`CaseKind::Tree`). Unlike G3 (single-goal spine), it advances a non-linear tree with human authorization checkpoints:

- Human-authorization checkpoints: After decompose lands a `Decomposition` + child `Objective`s (LLM provenance, initially unbound), the traversal layer invokes the injected reviewer callback. The harness uses `AutoAcceptReviewer` (standing in for a human operator) to accept, after which the engine creates a real human `AcceptanceEvent` (not bypassing binding predicates); child goals then inherit binding via `member_of` and enter their own active contexts. The engine never fabricates human authorization—reject paths do not recurse, and no fake withdrawals are made.
- Focus-aware prompts: The harness’s prompt provider picks the objective text for the current `focus` id from the active context so that the design/implement for a child goal sees its own goal (not the root). The root objective uses the case-level acceptance; decomposed child goals do not re-inject the root acceptance, avoiding requiring each child to implement the whole tree. The implement phase also injects the current focus goal as semantic context, preventing design-name drift from being cemented during implementation.
- Success criterion: The harness merges all child-goal candidate files into one program and executes the entry to compare with expectations.

> Implementation status: The engine-side binding path (accept → inherit → child goal enters active context) is covered by traversal unit tests. The real LLM end-to-end run of G6-01 awaits an API-key environment (cleanly skipped without a key).

### R Repair loop (cross-cutting)

| ID | Scenario | Injected defects (problem, not answer) | Success Criteria |
| --- | --- | --- | --- |
| R-01 | Defective candidate for the increment action | C-style `int`, missing braces in `output`, body references an undeclared variable | Repaired within budget → check passes → `IncrementCounter(41)=42` |

R cases explicitly set a positive repair budget, examining “real-diagnostics-driven convergence” (can compose with all groups).

---

## V. Engineering structure

All e2e cases share a single harness (eliminating scaffolding duplication):

```
cli/examples/e2e/
├── main.rs          ← Entry: select group/case (--group/--case/--list), run, and report
├── harness.rs       ← Reusable components: construct LLM backend (+ bounded jitter retries),
│                       drive design→implement→check→repair, real tools/check bridge,
│                       v0 interpreter execution (incl. console verification + real host
│                       injection), anti-leak prompt assembly
└── cases/
    ├── mod.rs               ← Case registry (by group)
    ├── g1_basics.rs         ← G1 + R-01
    ├── g2_effects.rs        ← G2 (incl. G2-03 real network)
    ├── g3_heuristic.rs      ← G3 (scheduler-driven autonomy)
    ├── g4_complex.rs        ← G4 (cross-calls / error algebra / fallible returns)
    ├── g5_file.rs           ← G5 (stdlib File, real temp files)
    └── g6_tree.rs           ← G6 (goal tree traversal)

scripts/run_e2e.sh           ← Sequential batch runner (one process per case; logs saved + summarized)
```

Each case is described by a unified `Case` (problem + entry + expectations + optional defective seed candidate). The harness dispatches by `CaseKind` (`DesignImplement` / `RepairSeed` / `Scheduler` / `Tree`). Adding a new case = add a `Case` in the corresponding group file and register it in `mod.rs`, without touching the harness. If introducing new domain terms, register tokens in the anti-leak assertion in `render.rs`.

### Relationship with CI

e2e by default does not enter `cargo test`. The structural part of its anti-leak discipline (syntax baseline / library assets free of task tokens) is guarded by `sophia-prompt` unit tests—that part does enter CI. e2e itself serves as an on-demand/scheduled verification of the real loop.
