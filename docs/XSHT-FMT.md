# XSHT Formatter Design

`xsht fmt` normalizes source rather than preserving every byte. The AST supplies
semantic shape and precedence; the CST supplies spans, comments, and layout
cues. `crates/xsht/src/format.rs` owns layout. The language contract is in
`SPEC.md`.

## Policy

- Preserve meaningful source choices when flat and broken forms both fit:
  blocks, control-flow expressions, `with` bindings, calls, comprehensions,
  collections, method chains, pipeline stages, and breaks before continuation
  operators. Do not keep accidental cramped or one-token-per-line layouts.
- A one-statement block stays on one line when it fits. The branches of an
  `if` share a flat or broken shape; a `match` arm keeps a control-flow
  statement unbraced only while it fits on that arm's line.
- Break at semantic boundaries before breaking inside nested expressions:
  chained calls, arguments, record fields, collection items, comprehension
  clauses, and pipeline stages. Broken calls put one argument per line with a
  trailing comma; method chains continue on leading-dot lines. A collection
  that breaks between elements expands its siblings consistently, including
  nested collections; a break inside one element does not make the collection
  broken.
- Keep a fitting command on one line. Otherwise, break at its arguments,
  options, environment assignments, redirections, and pipeline segments, with
  each option kept with its value. Continuations use ` \` and one indent
  level; command blocks use two. Pipeline stages use a two-space continuation.
- Blank lines separate sections, declarations, and multi-line control flow.
  Comments constrain layout: leading and trailing comments stay attached,
  block-ending comments stay inside the block, and comments prevent AST-only
  regeneration where their position would be lost. Keep an existing blank line
  beside a comment, but do not add one. `# fmt: skip` preserves the next
  statement byte-for-byte.
- Strings, paths, comments, and other indivisible tokens are not split and may
  exceed `[format] line-width` (default 120 characters). Multiline literal
  contents are not reindented. Width is measured in characters, not display
  columns. Preserve string escapes, quoted command words with interpolation,
  and shorthand pattern fields.

## Invariants

- Output parses without diagnostics, checks without new checker diagnostics,
  and reparses to the same syntax tree ignoring positions. `fmt` and `lint
  --fix` verify this before writing; a mismatch is a `format-equivalence`
  error.
- Parentheses match the grouping required by the parser. Formatting ignores
  only `check.redundant-parens`; every other checker diagnostic blocks a write.
- Formatting is idempotent. A fitting conditional value stays on its line; if
  it must break, all branches break. Expression continuations never become
  separate statements. Comprehension values stay grouped at their rendered
  qualifier boundary, and moving an optional pipeline qualifier does not add
  redundant callback parentheses.
- Formatting neither duplicates nor drops comments, and preserves `fmt: skip`.
  It leaves lint diagnostics unchanged; see [XSHT tooling](XSHT.md#lint).
- A typed command argument loses parentheses only where its bare form parses
  as the same expression. Parentheses around a name, number, or ordinary string
  remain because those forms are command words.
- `xsht fmt` writes only after every selected file formats successfully.
  Cancellation during a file writes none of the selected files.

## Tests

Add visible formatting cases to the annotated golden pair in
`tests/fixtures/fmt/`. The disk-backed test is
`tests/xsh/formatter.xsh::test_fmt_fixture`; narrow host contracts live in
`crates/xsht/tests/syntax.rs` and `crates/xsht/tests/formal_bridge.rs`. See
[Testing](TESTING.md#native-tests-first) for focused commands. The sibling
`../laputa` corpus is an optional stress test, not a golden.
