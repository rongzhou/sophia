# Changelog

All notable changes to this project will be documented in this file. This project uses two related version notions:

- Engineering version (package.json version): tracks CLI/tooling releases.
- Language boundary version (docs refer to v0.2, v0.3, ...): tracks the committed, testable Sophia-Core feature boundary.

## [0.2.0] - 2026-05-28

- Align package.json/CLI version to 0.2.0; sophia.toml `version` to 0.2.0 and `sophia_version` to 0.2.
- Add CI for typecheck, lint, coverage, format check, build; auto-update docs status.
- Add ESLint with Prettier integration and conservative rules.
- Add coverage script and include in CI.
- Add bench scripts: `bench:report`, `bench:summarize`.
- Add Husky + lint-staged pre-commit hooks.
- Add `.nvmrc` to standardize Node version.
