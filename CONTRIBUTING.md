# Contributing to SZ-ORM

> Thank you for considering a contribution to SZ-ORM! This document describes the workflow, coding standards, and review process for all contributors.

> 适用版本：SZ-ORM v1.0.0（39 工作空间成员 / 1970+ 测试 / L4 金融级）
> 更新日期：2026-07-21

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Coding Standards](#coding-standards)
- [Testing Requirements](#testing-requirements)
- [The 10-Gate CI Pipeline](#the-10-gate-ci-pipeline)
- [Five-Dimensional Code Review](#five-dimensional-code-review)
- [Commit Message Convention](#commit-message-convention)
- [Pull Request Process](#pull-request-process)
- [Release Process](#release-process)
- [Security Vulnerability Reporting](#security-vulnerability-reporting)
- [License](#license)

---

## Code of Conduct

Be respectful, constructive, and inclusive. We follow the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct).

## Getting Started

### Prerequisites

- **Rust toolchain**: 1.75+ (edition 2021); stable channel recommended
- **Operating system**: Linux / Windows / macOS (CI runs all three)
- **Databases (optional, for integration tests)**: MySQL 8+/9.x, PostgreSQL 14+/18, SQLite 3.35+, Oracle 23ai Free
- **Cloud services (optional, for ignored tests)**: MQTT broker, RabbitMQ, MinIO/S3, OpenAI API key

### Fork & Clone

```bash
# Fork the repo on GitHub, then:
git clone https://github.com/<your-username>/sz-orm.git
cd sz-orm
git remote add upstream https://github.com/ljclz/sz-orm.git
git fetch upstream
```

### Build

```bash
# Full workspace build
cargo build --workspace

# Specific package
cargo build -p sz-orm-core
```

### Run Tests

```bash
# Full workspace (skip ignored tests)
cargo test --workspace

# Core package only
cargo test -p sz-orm-core

# Run ignored integration tests (requires real DBs)
cargo test -p sz-orm-sqlx -- --ignored
```

## Development Workflow

1. **Pick an issue**: Look for `good-first-issue` or `help-wanted` labels, or open a new issue describing what you want to change.
2. **Create a branch**: `git checkout -b feat/<short-description>` (or `fix/` / `docs/` / `refactor/`).
3. **Write code + tests**: Follow the [Testing Requirements](#testing-requirements). TDD is strongly recommended.
4. **Run gates locally**: `./scripts/gate.ps1` (Windows) or `./scripts/gate.sh` (Linux/macOS) — runs fmt, clippy, test, build, doc, audit, deny.
5. **Commit**: Follow the [Commit Message Convention](#commit-message-convention).
6. **Push & open PR**: Fill in the PR template; link the issue.
7. **Address review feedback**: Push additional commits (do not squash until merge).
8. **CI must pass**: All 10 gates must pass on the PR before merge.

## Coding Standards

### Rust Style

- **Format**: `cargo fmt --all` (enforced in CI, gate 1)
- **Lint**: `cargo clippy --workspace --all-targets -- -D warnings` (enforced in CI, gate 3)
- **Edition**: 2021
- **No `panic!`/`unimplemented!`/`todo!` in production code** — return `Result<T, DbError>` instead
- **Zero warnings**: `RUSTFLAGS="-D warnings"` (enforced in CI, gate 2)
- **Async**: Use `tokio` 1.40+ with `async/await`; do not block in async contexts
- **Error handling**: Use `thiserror` for error enums; propagate with `?`; never `unwrap()` in production paths
- **Naming**: Descriptive names; no abbreviations except well-known ones (`i`, `j`, `ctx`, `tx`)
- **Comments**: Chinese comments are acceptable in non-public code; public API docs must be in English or bilingual

### Workspace Conventions

- **Version**: Managed centrally via `workspace.package.version` in root `Cargo.toml`; sub-packages use `version.workspace = true`. **Never** hard-code version numbers in sub-packages.
- **Dependencies**: Use `workspace.dependencies` for shared deps; sub-packages reference with `dependency.workspace = true`.
- **New packages**: Add to `members` array in root `Cargo.toml`; create `packages/sz-orm-<name>/` with `Cargo.toml` + `src/lib.rs`.

### File Organization

- One responsibility per file; files should not exceed ~1000 lines.
- Tests live in `tests/` (integration) or inline `#[cfg(test)]` modules (unit).
- Benchmarks live in `benches/` with `harness = false` for criterion.
- Examples live in `examples/src/bin/`.

### Memory & Performance

- Avoid unnecessary allocations; prefer `Cow<str>`, `&str`, `Arc<T>`.
- Use appropriate data structures (`HashMap` for O(1) lookup, `Vec` for iteration).
- Lazy evaluation: do not compute until needed.
- Hot paths must be benchmarked (add to `benches/core_bench.rs` if core).

## Testing Requirements

### Test Pyramid

| Tier | Type | Location | Coverage |
|------|------|----------|----------|
| T1 | Unit (inline `#[cfg(test)]`) | `src/*.rs` | Each public function |
| T2 | Integration | `tests/*.rs` | End-to-end flows |
| T3 | Property-based | `tests/property.rs` | Invariants with `proptest` |
| T4 | Fuzz | `tests/fuzz.rs` | Edge cases with `proptest` fuzz mode |
| T5 | Stress | `tests/stress.rs` | High concurrency / load |
| T6 | Soak | `tests/soak.rs` | 24h long-running degradation |

### TDD Cycle (RED → GREEN → REFACTOR)

1. **RED**: Write a failing test that captures the desired behavior.
2. **GREEN**: Write the minimum code to pass the test.
3. **REFACTOR**: Clean up while keeping tests green.

### Test Coverage Rules

- Every new public function MUST have at least one test.
- Every bug fix MUST include a regression test that fails before the fix.
- Edge cases (empty input, max values, concurrent access) MUST be tested.
- DML operation sequences (not just individual ops) MUST be tested to catch state pollution.
- Tests must validate **behavior under boundary/extreme conditions**, not just "happy path works".

### Soak Test Acceptance

24h soak test must meet all criteria:
- RSS growth < 50 MB
- fd_count growth < 10
- pool_active terminal value == pool_idle (no leak)
- ops_per_sec decay < 10%
- p99_latency growth < 2x
- error_count == 0

## The 10-Gate CI Pipeline

All PRs must pass these 10 gates before merge:

| # | Gate | Command | Blocking |
|---|------|---------|----------|
| 1 | fmt | `cargo fmt --all -- --check` | ✅ |
| 2 | check (3 OS × 2 Rust) | `cargo check --workspace --all-targets` with `RUSTFLAGS="-D warnings"` | ✅ |
| 3 | clippy | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ |
| 4 | test | `cargo test --workspace` | ✅ |
| 5 | doc | `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"` | ✅ |
| 6 | audit | `cargo audit` (8 ignored advisories with documented reasons) | ✅ |
| 7 | deny | `cargo deny check advisories bans licenses sources` | ✅ |
| 8 | integration | `cargo test --workspace -- --ignored` with real DBs/containers | ✅ |
| 9 | soak-smoke | 10-second soak test (degradation framework sanity check) | ✅ |
| 10 | real-features-compile | `cargo check` with all `real-*` features on Linux | ✅ |

### Local Gate Script

```bash
# Windows
./scripts/gate.ps1

# Linux/macOS
./scripts/gate.sh
```

This runs gates 1-7 locally. Gates 8-10 only run in CI (require Docker / cloud credentials).

## Five-Dimensional Code Review

Every PR must complete an explicit five-dimensional review checklist. Do NOT rely on automated gates alone.

| Dimension | What to check |
|-----------|---------------|
| **1. Correctness (正确性)** | Does the code do what it claims? Are edge cases handled? Are error paths correct? Do tests cover the behavior? |
| **2. Readability (可读性)** | Is the code clear? Are names descriptive? Is the abstraction level appropriate? Are comments helpful (not redundant)? |
| **3. Architecture (架构)** | Does the change fit the existing module boundaries? Does it introduce circular deps? Is the layering correct (core → sqlx → extensions)? |
| **4. Security (安全性)** | Does it handle untrusted input safely? Does it introduce SQL injection risks? Are credentials leaked? Are new dependencies audited? |
| **5. Performance (性能)** | Does it allocate unnecessarily? Does it hold locks too long? Does it have O(n²) where O(n) suffices? Is the hot path still fast? |

Record the review result in the PR description or in `docs/附录H-五维审查清单.md` for major phases.

## Commit Message Convention

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Types

| Type | Use for |
|------|---------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `build` | Build system or dependencies |
| `ci` | CI configuration |
| `chore` | Other maintenance tasks |
| `revert` | Revert a previous commit |

### Scope

Use the package name, e.g., `sz-orm-core`, `sz-orm-sqlx`, `sz-orm-vector`, `docs`, `ci`.

### Subject

- Imperative mood: "add" not "added"; "fix" not "fixed"
- Lowercase, no period at end
- Max 72 characters

### Body

- Explain **why** the change is needed, not just **what** changed
- Wrap at 80 characters
- Use bullet points for multiple changes

### Footer

- `BREAKING CHANGE: <description>` for breaking changes (bump major version)
- `Closes #123` / `Fixes #123` to link issues
- `Co-Authored-By: <name> <email>` for pair contributions

### Example

```
feat(sz-orm-core): add optimistic_lock module

- Add OptimisticLock trait with version field auto-increment
- Add retry_on_conflict() function with exponential backoff
- 12 unit tests + 4 integration tests with concurrent update scenarios
- Closes #142

Co-Authored-By: Jane Doe <jane@example.com>
```

## Pull Request Process

1. **Open PR**: Use the PR template (`.github/pull_request_template.md`).
2. **Fill template**:
   - Summary of changes
   - Related issue (e.g., `Closes #123`)
   - Test plan (what tests were added/modified)
   - Five-dimensional review checklist
   - Breaking changes (if any)
3. **CI runs**: All 10 gates must pass.
4. **Review**: At least one maintainer approval required for merge.
5. **Address feedback**: Push additional commits; do not force-push during review.
6. **Squash & merge**: Maintainer squashes commits on merge (keeps history clean).
7. **Delete branch**: Delete the feature branch after merge.

### PR Size

- **Small PRs** (< 300 lines): Preferred, faster review
- **Medium PRs** (300–1000 lines): Acceptable, may take longer
- **Large PRs** (> 1000 lines): Break into multiple PRs unless explicitly justified

## Release Process

Releases are managed by maintainers. Version numbers follow [Semantic Versioning](https://semver.org/):

- **Major (x.0.0)**: Breaking API changes
- **Minor (1.x.0)**: New features, backward compatible
- **Patch (1.0.x)**: Bug fixes only

### Release Steps

1. Update `workspace.package.version` in root `Cargo.toml` (single source of truth).
2. Update `CHANGELOG.md` with all changes since last release.
3. Update documentation version references (`docs/*.md` headers).
4. Run full gate suite locally: `./scripts/gate.ps1`.
5. Create annotated tag: `git tag -a v1.0.0 -m "v1.0.0 release"`.
6. Push tag: `git push origin v1.0.0`.
7. CI auto-publishes to crates.io on tag push (if configured).
8. Create GitHub Release with changelog notes.

### Version Management Rules

- **Never** hard-code version numbers in sub-package `Cargo.toml` files. Always use `version.workspace = true`.
- **Never** update version in multiple places. The root `Cargo.toml` `[workspace.package]` section is the single source of truth.
- **Always** update `CHANGELOG.md` when bumping version.

## Security Vulnerability Reporting

**Do NOT open a public GitHub issue for security vulnerabilities.**

Instead, email the maintainers at `zhangmingjie@ljclz.vip` with:
- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Suggested fix (if any)

We will acknowledge within 48 hours and aim to release a fix within 7 days for Critical/High severity.

### Security Audit Baseline

- `cargo audit` runs on every push/PR (gate 6)
- `cargo deny check` runs on every push/PR (gate 7)
- 8 RUSTSEC advisories are currently ignored with documented reasons (see `audit.toml`)
- License whitelist: 14 permissive licenses only; copyleft (GPL/AGPL/LGPL) is forbidden
- Dependency sources: only crates.io official registry; no git/path sources

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).

---

## Quick Reference

| Task | Command |
|------|---------|
| Format code | `cargo fmt --all` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Test (fast) | `cargo test --workspace` |
| Test (ignored, needs DB) | `cargo test --workspace -- --ignored` |
| Bench | `cargo bench -p sz-orm-core` |
| Soak (10s smoke) | `cargo test -p sz-orm-core --test soak -- --ignored` |
| Soak (24h) | `SOAK_DURATION=24h cargo test -p sz-orm-core --test soak -- --ignored` |
| Security audit | `cargo audit` (with 8 `--ignore` flags, see CI config) |
| Deny check | `cargo deny check advisories bans licenses sources` |
| Full local gate | `./scripts/gate.ps1` or `./scripts/gate.sh` |
| Generate docs | `cargo doc --workspace --no-deps --open` |

Thank you for contributing to SZ-ORM!
