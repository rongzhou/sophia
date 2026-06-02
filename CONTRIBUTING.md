# Contributing Guide

Thank you for your interest in Sophia. This document explains the development workflow and code conventions. For building and testing, see [INSTALL.md](INSTALL.md).

## Core Principles

- **Single path**: no multi-path / dual-stack / backward-compatibility burden / functional fallback at any layer. Design changes migrate directly and remove old paths. Placeholders must live in the single code path and clearly return unimplemented errors, rather than fabricating fallback behavior.
- **Honesty**: never fabricate success; hard errors must block honestly (“pending integration” must be labeled honestly).
- **Layering discipline**: `core/*` is zero-I/O and does not depend on `workflow/*`; `tools/*` is deterministic and does not depend on the workflow graph; the compiler never calls LLMs.
- **Comments and docs are unified in Chinese**, and English terms are introduced in the form “Chinese (English term)” on first use.

See `docs/en/engineering_notes.md` for the decision log and the complete statement of these principles.

## Workflow for Every Change

1. **Read before writing**: before changing code, read the relevant code and design docs (`docs/`) to understand the design intent.
2. **Implement**: land the ideal design; do not patch around the issue or compromise.
3. **Add regression tests**: new features / bug fixes should include tests.
4. **Sync docs**: update `docs/en/dev_checklist_v1.md` (current progress SSOT + change log); add an entry to `docs/en/engineering_notes.md` for decisions; update `docs/en/workflow_graph_spec.md` for graph schema changes.
5. **Verify** (must be all green):

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

## Commit Conventions

- Commit messages are in Chinese. The first line should summarize concisely; the body should explain “what changed / why / verification status.”
- Commit only after the logical change is complete and tests pass; avoid many fragmented small commits around the same file.
- Do not commit temporary / debug code or generated artifacts (`target/`, generated files under `sophia-runs/`, `*.sqlite`, etc. are already in `.gitignore`).
- Do not commit secrets: API keys are read only through environment variables such as `SOPHIA_LLM_API_KEY`; they must not be written to disk or committed.

## Testing Conventions

- Unit / integration tests are deterministic and enter the `cargo test` gate.
- Real-LLM end-to-end tests are `example`s and do **not** enter CI; run them manually as needed (see `docs/en/e2e_test.md`).
- Snapshot tests use [`insta`](https://insta.rs/): after adding / changing snapshots, review and accept `.snap.new` with `cargo insta review`.
- Anti-answer-leakage is the first principle for e2e: task answers must not enter shared scaffolding (syntax baseline / system prompt).

## Changing Syntax

Changes to `core/syntax/grammar.js` must regenerate `parser.c` with the aligned Tree-sitter CLI version (see “Changing Syntax” in INSTALL.md). Version alignment among the crate / CLI / ABI is a hard constraint.

## License

By submitting a contribution, you agree to license your contribution under this project’s MIT License, with no additional terms.
