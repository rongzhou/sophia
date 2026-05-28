# Contributing

Thank you for your interest in contributing to Sophia! This document summarizes the working conventions.

- Node version: see `.nvmrc` (Node 26). The repo also enforces `engines.node >= 26`.
- Install: `npm ci`.
- Build: `npm run build` (TypeScript strict mode, ESM NodeNext module).
- Typecheck: `npm run typecheck`.
- Tests: `npm test` (or `npm run test:coverage`).
- Lint/format: `npm run lint`, `npm run format`. Pre-commit hooks run lint-staged for changed files.
- Benchmarks: `npm run bench:report` (requires local Ollama; see README). Use `bench:summarize` to summarize JSONL outputs.

Versioning principles:
- Engineering version (`package.json`): release versions of CLI/tools.
- Language boundary version (docs use v0.2, v0.3, ...): committed, testable Sophia-Core feature boundary. Changes to the language boundary require synchronized updates to docs, diagnostics, and tests, and should not be conflated with engineering patch releases.

Pull Requests:
- Keep changes minimal and well-scoped.
- Add tests for new checker diagnostics or backend behavior.
- Update docs and status when language boundary changes.
- Ensure CI is green: typecheck, lint, tests, format check, and build must pass.
