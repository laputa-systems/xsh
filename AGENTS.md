# Agent Guide

This repository contains the standalone XSH language implementation, tools,
core command scripts, examples, docs, and tests.

## Output

- Do not repeat command output the user can already see.
- Summarize findings, decisions, and verification results.
- When reporting a failure, include the command and why it matters.

## First Five Minutes

Always read:

- `docs/CHAPTER-01-why-xsh.md`
- the nearest code and tests for the requested change

Use the documentation-routing policy below, `docs/ARCHITECTURE.md`, and
`docs/TEST-MAP.md` to choose the task-specific contract, owner files, and tests
before editing.

## Implementation Rules

- Keep changes scoped to the requested behavior and prefer existing patterns.
- Preserve useful comments and do not add banner or separator comments.
- Do not add dependencies unless there is a clear need and no local equivalent.
- Update the closest tests, examples, and `docs/` markdown for the behavior you
  changed. Do not rebuild or edit generated documentation unless the user
  explicitly asks; it creates large generated churn.
- Prefer an `xsht` native test first for XSH behavior: add or extend a
  `proc test_*` under `tests/**/*.xsh` or `showcase/tests/**/*.xsh` when the
  contract can be expressed through XSH, using `test.run_script`,
  `test.run_xsh`, `test.run_xsht_trace`, temp resources, and mocks as needed.
  Keep Rust integration tests for host or CLI boundaries, exact process or
  byte-level lifecycles, platform or privilege behavior, PTYs, and fixtures or
  servers that native tests cannot own. Before embedding XSH source in Rust,
  confirm that the behavior crosses one of those Rust-owned boundaries.
- If language behavior changes, update `docs/SPEC.md` first or in the same
  change.
- `LANG.md` contains only open language proposals and unresolved tickets.
  Implemented behavior belongs in `docs/SPEC.md` and its canonical companion
  documentation.

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

- Put language contracts in `docs/SPEC.md`, OS behavior in `docs/SPEC-OS.md`,
  streams in `docs/STREAMS.md`, JSON boundaries in `docs/JSON.md`, API details
  in `xsht api`, architecture in
  `docs/ARCHITECTURE.md`, and testing in `docs/TEST-MAP.md`.
- Do not add prose that restates obvious syntax or API signatures. Prefer exact
  symbols, module paths, and test names; document non-obvious constraints and
  rationale; update the canonical owner instead of creating another guide.

## Verification

Choose the narrowest useful command first, then run the full relevant gate from
`docs/TEST-MAP.md`. Use debug builds for ordinary development and verification.
Build the exact binary or package needed for the task instead of using bare
`cargo build --release`: the root package declares the user-facing `xsh`,
`xshi`, and `xsht` binaries, plus seven `xsh-test-*` helper binaries and the
`xsh-frontend-stats` profiling tool. A root release build applies thin LTO to
all of them, so prefer commands such as `cargo build --bin xsh`,
`cargo build --bin xsht`, or a targeted package/test command.
Use release builds only when working on profiling or benchmarking. Do not use
the `dist` profile for agent work;
it is reserved for CI release packaging.

Do not run formatters or autofixers — `make lint`, `cargo fmt`, `cargo clippy
--fix`, `xsht fmt`, or `xsht lint --fix`. `make lint` runs `clippy --fix
--all-features` and `cargo fmt --all`, which rewrite files unrelated to your
change and create large, noisy churn. Formatting and linting are the user's
responsibility; leave them to the user. To verify your own work, build and test
instead.
