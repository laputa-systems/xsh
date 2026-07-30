# Test Fixtures

Fixtures are small XSH source files used by Rust integration tests. Add the
smallest fixture that exercises the behavior under test.

| Directory | Used by | Purpose |
|---|---|---|
| `syntax` | `tests/syntax.rs` | parser, formatter, lexer, and syntax oracle inputs |
| `fmt` | `tests/xsh/formatter.xsh::test_fmt_fixture` | annotated disk-backed formatter fixture and golden exercised through `xsht fmt` |
| `sema` | `tests/sema.rs` | checker and lint inputs |
| `runtime` | `tests/runtime/*` | executable scripts for runtime behavior |
| `diagnostics` | diagnostics tests | rendered diagnostic fixtures |
| `modules` | module tests | generated or expected module metadata |

Prefer targeted Rust tests for narrow behavior. Use fixture scripts when the
source shape itself matters or when running through `xsh` gives better coverage.

Cataloged tutorial examples live in `examples/`, not here. Larger complete
programs live in `showcase/` with tests in `showcase/tests/`.
