# Agent Guide

This repository contains the standalone XSH language implementation, tools,
core command scripts, examples, docs, and tests.

## Output

- Do not repeat command output the user can already see.
- Summarize findings, decisions, and verification results.
- When reporting a failure, include the command and why it matters.

## First Five Minutes

Always read:

- `docs/user-tour.md`
- the nearest code and tests for the requested change

Use the documentation-routing policy below, `docs/ARCHITECTURE.md`, and
`docs/TESTING.md` to choose the task-specific contract, owner files, and tests
before editing.

## Implementation Rules

- Keep changes scoped to the requested behavior and prefer existing patterns.
- Preserve useful comments and do not add banner or separator comments.
- Do not add dependencies unless there is a clear need and no local equivalent.
- Update the closest tests, examples, and `docs/` markdown for the behavior you
  changed. Never hand-edit generated documentation (`docs/user-tour.md`,
  `docs/reference/`): edit `docs/templates/` and `docs/snippets/tour/`, then
  run `make docs`; `make check` fails on stale generated docs.
- Prefer an `xsht` native test first for XSH behavior: add or extend a
  `test NAME { ... }` declaration under `tests/**/*.xsh` or `showcase/tests/**/*.xsh` when the
  contract can be expressed through XSH, using `test.run_script`,
  `test.run_xsh`, `test.run_xsht_trace`, temp resources, and mocks as needed.
  Keep Rust integration tests for host or CLI boundaries, exact process or
  byte-level lifecycles, platform or privilege behavior, PTYs, and fixtures or
  servers that native tests cannot own. Before embedding XSH source in Rust,
  confirm that the behavior crosses one of those Rust-owned boundaries.
- If language behavior changes, update `docs/SPEC.md` first or in the same
  change.

## Content Tiers

- Put focused behavior coverage in `tests/xsh/stdlib/*.xsh` or the nearest
  native test module. Syntax, API behavior, edge cases, errors, platform
  behavior, and regressions are tests, not examples.
- Write native tests as the default idiomatic XSH corpus: make ownership,
  effects, cleanup, typed boundaries, and expected errors clear in the test
  itself instead of maintaining a separate idiom guide.
- Put a script in `examples/*.xsh` only when it is a substantial, idiomatic
  multi-module program that is useful to read as a whole. It must not duplicate
  focused native-test coverage.
- Put larger production-like programs in `showcase/`. Existing `showcase/`
  content is outside ordinary example maintenance.

## Documentation Routing

- Put language contracts (including OS, streams, JSON, and interactive
  behavior) in `docs/SPEC.md`, API details in the registry behind `xsht api`,
  architecture and invariants in `docs/ARCHITECTURE.md`, test gates in
  `docs/TESTING.md`, tooling in `docs/XSHT.md`, and the system-report contract
  in `core/SYSTEM-REPORT.md`.
- Do not add prose that restates obvious syntax or API signatures. Prefer exact
  symbols, module paths, and test names; document non-obvious constraints and
  rationale; update the canonical owner instead of creating another guide.

## Verification

Choose the narrowest useful command first, then run the full relevant gate from
`docs/TESTING.md`. Run native XSH test suites on release binaries: build them
with `cargo build --release -p xsh --bins -p xsht --bin xsht`, then run
`target/release/xsht test ...`. They run several times faster than debug (the
stdlib suite takes 6 s instead of 34 s), and `xsht` runs the sibling `xsh` from
its own directory. Use debug builds for Rust
`cargo test` gates and quick compile checks.
Build the exact binary or package needed for the task instead of using bare
`cargo build --release`: the `xsh`, `xshi`, and `xsht` packages own the
user-facing binaries, while the root package also owns the `xsh-test-*`
helper binaries and the `xsh-frontend-stats` profiling tool. Do not use the
`dist` profile for agent work; it is reserved for CI release packaging.

Bound machine load: run one full native suite at a time, run ad-hoc `xsh`
probes with a wall-clock limit, and never leave processes running after a lane
finishes.

All Linux support goes through the `Dockerfile.test` environment. Linux builds,
tests, and verification run in the image that file defines (`xsh-test`), driven
by `cargo dev test linux` or `cargo dev test linux --ci`; the
target is `aarch64-unknown-linux-musl` with the flags in
`dev/targets.xsh::docker_test_env`. Do not substitute another Linux toolchain,
image, or libc: the container pins the compiler, the musl CRT objects, and the
`__isoc23_*` symbol aliases that this tree links against, so a build outside it
is not evidence about Linux support.

Do not run formatters or autofixers — `cargo dev lint --fix`, `cargo fmt`,
`cargo clippy --fix`, `xsht fmt`, or `xsht lint --fix`. `cargo dev lint --fix`
runs `clippy --fix --all-features` and `cargo fmt --all`, which rewrite files
unrelated to your change and create large, noisy churn. Formatting and linting
are the user's responsibility; leave them to the user. To verify your own work,
build and test instead.
