# Documentation Style

`docs/` is a contract and reference layer for maintainers and coding agents.
It is not a tutorial corpus.

## Write For Change Work

- Put the explanation above the symbol, invariant, or command it describes.
- Name the exact implementation symbols, module paths, and nearest tests. For
  example, describe `Evaluator::collect_stream_values` in
  `src/runtime/eval/stream.rs` and name `tests/xsh/stdlib/streams.xsh` when it
  is the executable contract.
- State non-obvious ownership, platform constraints, ordering guarantees,
  representation boundaries, and intentional omissions. Do not restate syntax
  that `docs/SPEC.md` or `xsht api` already defines.
- Keep one canonical owner per topic. Link to that owner instead of adding a
  second cross-cutting explanation.

## Route Content

| Content | Canonical owner |
|---|---|
| language contract | `docs/SPEC.md` |
| OS-facing behavior | `docs/SPEC-OS.md` |
| stream behavior | `docs/STREAMS.md` |
| JSON boundaries | `docs/JSON.md` |
| API signatures and structured reference | `xsht api` |
| subsystem ownership | `docs/ARCHITECTURE.md` |
| test selection | `docs/TEST-MAP.md` |
| task navigation | `docs/AGENT-ROUTING.md` |

## Examples And Tests

`tests/xsh/stdlib/*.xsh` and nearest focused native tests own exhaustive
behavior coverage and are the default idiomatic XSH corpus. `examples/*.xsh`
contains only cataloged, substantial multi-module showcases.
`examples/README.md` must link each retained showcase to its canonical docs and
focused coverage.

Run the formatter-free docs and showcase gates in `docs/TEST-MAP.md` after
changing generated reference metadata, canonical docs, or retained examples.
