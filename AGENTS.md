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
- `docs/CHAPTER-15-why-not-xsh.md`
- `docs/IDIOMS.md`
- `docs/AGENT-ROUTING.md`
- the nearest code and tests for the requested change

Use `docs/AGENT-ROUTING.md` to choose the task-specific spec, architecture,
owner files, and tests before editing.

## Implementation Rules

- Keep changes scoped to the requested behavior and prefer existing patterns.
- Preserve useful comments and do not add banner or separator comments.
- Do not add dependencies unless there is a clear need and no local equivalent.
- Update the closest tests, examples, and `docs/` markdown for the behavior you
  changed. Do not rebuild or edit `docs-html/` unless the user explicitly asks;
  it creates large generated churn.
- If language behavior changes, update `docs/SPEC.md` first or in the same
  change.
- `LANG.md` contains only open language proposals. Implemented behavior belongs
  in `docs/SPEC.md` and `docs-src/CHAPTER-*.md.in`.

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
