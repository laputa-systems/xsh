# Curated Showcases

`examples/` contains a small set of executable programs that demonstrate how
multiple XSH modules fit together. It is not a feature-probe directory:
focused behavior, errors, edge cases, and platform coverage belong in
`tests/xsh/stdlib/*.xsh` or the nearest native test module.

`examples/catalog.json` is the executable showcase manifest. It records each
program's arguments, expected status and output, and trace policy. The runtime
gate in `tests/runtime/examples.rs` executes every cataloged program, checks
formatting and linting, and verifies that the manifest covers every
`examples/*.xsh` file.

| Showcase | Composition focus | Canonical documentation | Focused coverage |
|---|---|---|---|
| `json.xsh` | typed JSON read/write, JSON-lines, and schema checks at a persistence boundary | `docs/JSON.md` | `tests/xsh/stdlib/json.xsh` |
| `streams.xsh` | filesystem records through serial and bounded parallel stream stages | `docs/STREAMS.md` | `tests/xsh/stdlib/streams.xsh` |
| `processes.xsh` | process discovery, structured command plans, concurrent waits, and host identity records | `docs/SPEC-OS.md` | `tests/xsh/stdlib/process.xsh`, `tests/xsh/stdlib/system.xsh` |
| `release-package.xsh` | staged archive creation, safe extraction, rooted patching, and byte-preserving compression | `xsht api module:archive module:patch` | `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/diff.xsh`, `tests/xsh/stdlib/patch.xsh` |
| `typed-cli-options.xsh` | typed options, command dispatch, path conversion, and regex-backed input records | `docs/SPEC.md`, `xsht api module:cli module:regex` | `tests/xsh/stdlib/args.xsh` |

Put larger production-like programs in `showcase/`; do not turn a focused
native test into an example merely to demonstrate one API call.
