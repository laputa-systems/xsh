# Test Fixtures

Fixtures are source files and data used by native XSH and Rust tests. Add the
smallest fixture that exercises the behavior under test.

| Directory | Used by | Purpose |
|---|---|---|
| `syntax` | `crates/xsht/tests/syntax.rs` | parser, formatter, lexer, and syntax oracle inputs |
| `fmt` | `tests/xsh/formatter.xsh::test_fmt_fixture` | annotated disk-backed formatter fixture and golden exercised through `xsht fmt` |
| `sema` | `tests/sema.rs` | checker and lint inputs |
| `runtime` | `tests/runtime/*`, `crates/xsht/tests/formal_bridge.rs` | runtime behavior and formatter bridge fixture |
| `diagnostics` | diagnostics tests | rendered diagnostic fixtures |
| `modules` | module tests | generated or expected module metadata |
| `lint` | `tests/xsh/lint2-*.xsh` | accepted and rejected lint inputs |
| `stdlib` | `tests/stdlib_port.rs` | fixed OS-release sources |
| `interactive-parity` | no executable driver | historical `ish` terminal transcripts |
| `frontend-indexed` | frontend statistics tests | retained frontend memory corpus |

Use fixture scripts when the source shape itself matters or when running
through `xsh` reaches the behavior under test. See
[Testing](../../docs/TESTING.md#native-tests-first) for test ownership.

Cataloged tutorial examples live in `examples/`, not here. Larger complete
programs live in `showcase/` with tests in `showcase/tests/`.
