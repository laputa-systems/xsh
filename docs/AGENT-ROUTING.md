# Agent Routing

Use this file to choose the smallest useful reading set before editing. Keep
`AGENTS.md` short; put situational routing here.

Every owner entry should name the concrete implementation symbol, its file,
and its nearest test or fixture. Paths such as `src/runtime/*` are useful
bounds, but are not sufficient search handles by themselves.

## Baseline

Read these for any implementation task:

- `docs/CHAPTER-01-why-xsh.md`
- the nearest code and tests for the change

Use `docs/ARCHITECTURE.md` for subsystem ownership and `docs/TEST-MAP.md` for
verification commands.

## Task Map

| Task | Read | Primary owners | Tests |
|---|---|---|---|
| Syntax or formatting | `docs/SPEC.md` intro, `Philosophy`, `1. Status`, sections 2-9; `docs/ARCHITECTURE.md` Syntax; `docs/XSHT-FMT.md` for formatter/autofix tooling | `src/syntax/arena.rs`, `src/syntax/node.rs`, `src/syntax/cst.rs`, `src/syntax/lexer.rs`, `src/syntax/parser.rs`, `crates/xsht/src/format.rs` | `tests/syntax.rs`, `tests/fixtures/syntax`, `tests/fixtures/fmt`, `tests/xsh/formatter.xsh` |
| Typechecking or linting | `docs/SPEC.md` relevant section; `docs/SPEC-TYPING.md`; `docs/ARCHITECTURE.md` Semantics | `src/sema/check.rs`, `src/sema/check/*`, `crates/xsht/src/lint.rs`, `src/sema/records.rs` | `tests/sema.rs`, `tests/fixtures/sema` |
| Runtime evaluation | `docs/SPEC.md` relevant section; `docs/ARCHITECTURE.md` Runtime | `src/runtime/eval.rs`, `src/runtime/eval/*`, `src/runtime/value.rs` | `tests/runtime.rs`, `tests/runtime/*`, `tests/fixtures/runtime` |
| Process, cwd, env, signals, cancellation | `docs/SPEC.md` sections 9-12; `docs/SPEC-OS.md`; `docs/ARCHITECTURE.md` Runtime | `src/runtime/run.rs`, `src/runtime/process.rs`, `src/runtime/cwd.rs`, `src/runtime/eval/command.rs`, `src/runtime/eval/stmt.rs` | `tests/runtime/run.rs`, `process.rs`, `os.rs`, `unix.rs`, `linux.rs` |
| Structured streams | `docs/SPEC.md` section 14; `docs/STREAMS.md`; `docs/ARCHITECTURE.md` Runtime | `src/sema/check/stream.rs`, `src/runtime/eval/stream.rs`, `crates/xsh-registry/src/signature/streams.rs` | `tests/xsh/stdlib/streams.xsh`, `tests/runtime/streams.rs`, `examples/streams.xsh` |
| Standard module or method API | `docs/SPEC.md` section 13; `xsht api module:MODULE api:MODULE.FUNCTION method:TYPE.METHOD`; `src/modules/README.md` | `crates/xsh-registry/src/signature/*`, `crates/xsh-registry/src/runtime_op.rs`, matching `src/modules/*.rs`, `src/runtime/eval/modules.rs` | `tests/runtime/modules.rs`, targeted module tests |
| JSON behavior | `docs/SPEC.md` section 16; `docs/JSON.md` | `src/modules/json.rs`, `src/runtime/value.rs`, `src/sema/check.rs` | `tests/xsh/stdlib/json.xsh`, JSON cases in `tests/sema.rs`, `tests/runtime/modules.rs`, `examples/json.xsh` |
| Tracing and tracebacks | `docs/SPEC.md` section 18; `docs/ARCHITECTURE.md` Tracing And Errors | `src/trace.rs`, `src/runtime/eval.rs`, `crates/xsht/src/cli/mod.rs` | `tests/runtime/coverage.rs`, trace tests |
| CLI or tooling | `docs/SPEC.md` section 19; `docs/XSHT.md`; `docs/XSHT-FMT.md` for formatter behavior; `xsht api language:cli` | `crates/xsht/src/cli/mod.rs`, `crates/xsht/src/cli/grep.rs`, `crates/xsht/src/cli/refactor.rs`, `src/runner.rs` | `crates/xsht/tests/cli.rs`, `crates/xsht/tests/grep.rs`, `crates/xsht/tests/api.rs`, `tests/xsh/formatter.xsh` |
| Interactive shell | `docs/SPEC-INTERACTIVE.md`; `docs/ARCHITECTURE.md` Interactive | `crates/xshi/src/interactive/*`, `crates/xsht/src/cli/mod.rs`, `src/runtime/process.rs` | `tests/runtime/interactive.rs` |
| Executable IR or user-visible performance | `docs/FRONTEND.md`; `../FRONTEND-FOLLOWUPS.md`; `docs/BENCHMARKING.md`; `docs/ARCHITECTURE.md` Executable IR Ownership | `src/runtime/eval/indexed.rs`, `indexed/full.rs`, `lower.rs`, `lowered_run/indexed_run.rs`, `lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval.rs` | targeted executable/runtime tests, `scripts/ir-layout.py`, `make bench-fast`, then `make bench` for latency |
| LLVM IR or generic code size | `docs/BENCHMARKING.md` | generic owners identified by `cargo llvm-lines` | `tools/llvm-lines-repeat-offenders.xsh`, targeted behavior tests, `make bench` when applicable |
| Docs, examples, or references | `docs/DOCS-STYLE.md`; `examples/README.md`; `XSHT-API-REGISTRY.md` | `crates/xsh-registry/src/signature/*`, `crates/xsht/src/api.rs`, `crates/xsht/src/xsht/examples.rs`, `examples/catalog.json`, canonical docs | `crates/xsht/tests/api.rs`, `tests/runtime/examples.rs`, API commands in `docs/TEST-MAP.md` |
| Remote amd64 musl work | `../laputa/AGENTS.md` Threadripper Notes; `docs/BENCHMARKING.md` for benchmarks | remote checkout only | native command from the task |

For frontend changes, use the symbol vocabulary and lifecycle table in
`docs/FRONTEND.md`, especially `Lexer::lex_compact`,
`Parser::parse_source_arena_only`, `Checker::check_compact_declarations`,
`FullBuilder::build_compact`, `FullVerifier::verify`, and
`Evaluator::prepare_compact_indexed_only`.

## Spec Sections

For most changes, read the `docs/SPEC.md` introduction, `Philosophy`, and
`1. Status`, then the specific section below:

| Area | Spec section |
|---|---|
| source, spans, diagnostics | 2 |
| lexical rules | 3 |
| programs and statements | 4 |
| types and values | 5 |
| expressions | 6 |
| pure functions and procs | 7 |
| control flow and results | 8 |
| commands | 9 |
| process execution | 10 |
| argv conversion | 11 |
| status | 12 |
| standard modules | 13 |
| structured streams | 14 |
| builder blocks | 15 |
| JSON | 16 |
| resolver and checker | 17 |
| tracing and tracebacks | 18 |
| CLI | 19 |
| native tests and fixtures | 20-21 |

## Final Frontend Gates

Historical frontend migration scripts were removed after the indexed runtime
became the sole production path. Use `docs/TEST-MAP.md` for the
current direct frontend, explicit-frame, and runtime gates. Do not recreate a
shadow executor or a command-line lowering switch to diagnose a gap; retain the
checked source only long enough to render its diagnostic.
