# Sophia

Sophia is an LLM-native programming language and workflow for unattended LLM software development.

The project explores a premise: large-scale code pretraining is a powerful and efficient shortcut for teaching AI systems how to program, but it is not necessarily the only path or the long-term best substrate. Sophia asks whether part of programming competence can instead be externalized into a language, deterministic checks, graph-shaped context, and heuristic node workflows designed specifically for LLMs.

Current workflow:

```text
goal -> LLM decision -> .pseudo -> LLM implementation -> deterministic gates -> LLM repair/decision -> materialize -> build -> run
```

The current implementation is a v0.2 TypeScript CLI prototype. End-to-end experiments do not fake success paths: LLMs generate structured pseudocode, checkable Sophia candidates, and graph decisions; scaffold, diagnostics, and gates constrain and verify the work without replacing those LLM responsibilities.

## Documents

- [Status](docs/en/status.md): implemented capabilities, validation status, and known limits.
- [Language Design](docs/en/sophia_language_design.md): Sophia-Core semantics and the v0.2 boundary.
- [Heuristic Workflow](docs/en/heuristic_workflow.md): `.pseudo`, development graph, LLM decisions, repair, and materialize gates.
- [Roadmap](docs/en/roadmap.md): current effective roadmap and research direction.
- [Diagnostic Codes](docs/en/diagnostic_codes.md): parser / checker / build / run diagnostic code reference.
- [Benchmark Commands](docs/en/benchmark_runs.md): reproducible benchmark command examples.
- [Technical Report v0.2](docs/en/sophia_arxiv_technical_report_v0_2.md): current English technical report.

Chinese documents are under [docs/cn](docs/cn). Historical implementation notes, old roadmaps, experiment logs, and obsolete paper drafts are archived under `docs/archive/` and are not tracked by Git.

## Environment

Locally validated toolchain:

```text
Node.js >= 26
npm >= 11
Ollama installed locally
```

Install dependencies:

```bash
npm install
```

## Core Commands

```bash
npm run typecheck
npm test
npm run build
```

Run the CLI from source:

```bash
npm run dev -- --help
```

Run the built CLI:

```bash
node dist/cli/main.js --help
```

Initialize a workspace:

```bash
node dist/cli/main.js init
```

## Common CLI Flow

Initialize and use the development graph:

```bash
node dist/cli/main.js graph init
node dist/cli/main.js graph start "Compute, print, and return the first ten rabbit numbers."
node dist/cli/main.js graph design N0001 --model qwen3.6:latest
node dist/cli/main.js graph implement-loop N0002 --model qwen3.6:latest --max-repairs 2
node dist/cli/main.js graph check N0005
node dist/cli/main.js graph audit N0005
node dist/cli/main.js graph diff N0005
node dist/cli/main.js graph verify N0005
node dist/cli/main.js graph select N0005
node dist/cli/main.js graph materialize N0005
```

Check, index, generate context, build, and run materialized Sophia source:

```bash
node dist/cli/main.js check
node dist/cli/main.js index
node dist/cli/main.js context --action SumFirstFive
node dist/cli/main.js build
node dist/cli/main.js smoke
node dist/cli/main.js run SumFirstFive
node dist/cli/main.js run DoubleInput --input '{"count":7}'
```

Inspect pseudocode and repair artifacts:

```bash
node dist/cli/main.js graph pseudo-check fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-outline fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-scaffold fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js repair-context N0006
node dist/cli/main.js graph report
```

If Ollama is not running, LLM-dependent commands fail explicitly and create failed RawLlmNode artifacts instead of pretending a CodeNode succeeded.

## Benchmarks

Run a leakage-resistant benchmark verifier:

```bash
node dist/cli/main.js experiment list --suite benchmarks/category_a
node dist/cli/main.js experiment verify --task benchmarks/category_a/account_pipeline/task.json
node dist/cli/main.js experiment run --task benchmarks/category_a/account_pipeline/task.json --model qwen3.6:latest --mode full --max-design-revisions 2 --max-repairs 2 --ollama-timeout-ms 900000 --out sophia-runs/results/account-pipeline-full.jsonl
```

`experiment run` intentionally accepts one `--task`. Hidden verifier cases live in `task.json` and are not shown to the model prompt. See [docs/en/benchmark_runs.md](docs/en/benchmark_runs.md) for serial suite commands.
