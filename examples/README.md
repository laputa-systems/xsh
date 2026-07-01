# Examples

Examples in this directory must use syntax and behavior documented in
`docs/SPEC.md`. Each `.xsh` file is cataloged in `examples/catalog.json`, covered by
the runtime example corpus in `tests/runtime.rs`, and checked by `xsht fmt
--check`.

- `hello.xsh`: minimal top-level script.
- `args.xsh`: top-level script argument iteration.
- `result.xsh`: `Result` and `?` propagation on a pure function.
- `run.xsh`: explicit external process execution.
- `status.xsh`: status-preserving `run` with nonzero status as data.
- `spawn-wait.xsh`: process fan-out with explicit owned handles and list wait.
- `capture-text.xsh`: UTF-8 stdout capture.
- `capture-bytes.xsh`: byte stdout capture.
- `bytes.xsh`: base64/base32 byte roundtrips, byte slicing/dumps/strings,
  explicit byte-to-text decoding, typed byte comparison, and block copying.
- `text.xsh`: fixed-string text splitting, fields, joining, replacement,
  reversal, and counts.
- `env.xsh`: child environment overlays and scoped `env` blocks.
- `files.xsh`: `fs`, `path`, and `text` module use, including structured
  listings and path methods.
- `streams.xsh`: structured stream filtering, mapping, collection, and bounded
  `par-map`.
- `batch.xsh`: stream batching for count-based chunks and argv-safe command
  splices.
- `table.xsh`: typed sorting and deterministic table rendering for file
  records.
- `adapters.xsh`: explicit text, bytes, and JSON-lines adapters into
  structured pipelines.
- `json.xsh`: ordinary JSON read/write with explicit path, bytes, error, and
  status conversions.
- `cd.xsh`: scoped runtime cwd changes.
- `build-simple.xsh`: build-like file staging plus explicit `run`.
- `control.xsh`: `while`, `break`, `continue`, and minimal `match`.
- `package-records.xsh`: package record schemas, defaults, and rest parameters.
- `stream-surface.xsh`: compact stream surface examples including `defer`, `f`
  strings, duration and octal literals, `run.stream`, newer stream stages, and
  process command builders.
- `hash-tree.xsh`: tree manifest hashing with typed digests and checksum
  verification.
- `typed-cli-options.xsh`: typed option/subcommand parsing and regex records
  for Seed-like script inputs.
- `archive.xsh`: safe tar create/list/extract with gzip auto compression.
- `diff-patch.xsh`: unified diff generation and rooted patch application.
- `processes.xsh`: process, time, system, user, and group module coverage.
- `trace.xsh`: trace summary and raw trace coverage for proc and external process calls.
- `trace-error.xsh`: tracebacks for propagated `Err` values.

Larger standalone tools live as self-tested scripts under `showcase/`.
