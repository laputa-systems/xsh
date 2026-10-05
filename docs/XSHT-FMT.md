# XSHT Formatter Design

`xsht fmt` is a normalizing formatter, not a byte-preserving printer. The AST
supplies semantic shape and precedence; the CST supplies comments, exact spans,
and layout clues; a document layer (`Doc`, `DocRenderer` in
`crates/xsht/src/format.rs`) chooses flat or broken layout. The language
contract stays in `docs/SPEC.md`.

| Concern | Symbols | Owner |
|---|---|---|
| entry and output | `Formatter`, `format_files` | `crates/xsht/src/format.rs`, `crates/xsht/src/cli/fmt.rs` |
| source-faithful input | `SyntaxTree`, `apply_cst_guarded_edits` | `src/syntax/cst.rs`, `crates/xsht/src/edit.rs` |
| layout decisions | `canonical_parens_when_empty`, `needs_top_level_blank`, `prefer_broken` | `src/syntax/node.rs`, `crates/xsht/src/format.rs` |
| equivalence check | `verify_formatted_output` | `crates/xsht/src/format_equivalence.rs` |

## Policy

- Source intent breaks ties. When flat and broken forms are both readable, keep
  the author's choice (blocks, control-flow expressions, `with` bindings, call
  arguments, comprehensions, collections, method chains, pipeline stages, and a
  line break before a continuation operator). A collection counts as broken
  when the author broke it between elements, not inside one. A `"..."` string
  keeps its escapes, a quoted command word with interpolations stays one
  quoted string, and a `{name}` pattern field stays shorthand. Do not preserve
  accidental cramped or one-token-per-line layout.
- A block the author wrote on one line with a single statement stays on one
  line when it fits: `{ return 1 }`, `{ |x| x + 1 }`. The branches of an `if`
  are all flat or all broken, and a `match` arm holds a control-flow statement
  unbraced only when it stays on the arm's line. Arm-specific expression
  grouping applies to the arm's own statement; statements inside a declaration
  initializer keep ordinary block syntax.
- Break at semantic boundaries, in this order: between chained calls, call
  arguments, record fields, collection items, comprehension clauses, pipeline
  stages; inside nested expressions only as a last resort.
- Broken argument lists put one argument per line with a trailing comma. Method
  chains keep the first call on the receiver and continue with leading-dot lines
  that still parse as one expression.
- Once a collection breaks, similar siblings share a shape: when any element
  cannot fit flat on its own line, every element expands, and so do the
  collections nested in them. Otherwise each element keeps its own layout, so
  rows of short records stay one per line.
- Pipeline stages use a two-space continuation. Blank lines mark sections,
  declarations, and multi-line control-flow statements, decided from the
  formatted output so the first and second passes agree.
- Comments are layout constraints. Leading comments stay leading, trailing
  comments stay with their statement, a comment that ends a block stays inside
  it, and nested comments block AST-only regeneration. A blank line next to a
  comment is kept as one blank line where the author left it and never added.
  `# fmt: skip` preserves the next statement byte-for-byte.
- Strings, paths, comments, and other indivisible tokens are never split;
  they may exceed `format.line-width` (from the nearest `xsht-config.ini`,
  default 120). Multi-line literal contents are never reindented.
  A path before an interpolation format spec stays quoted so the spec's colon
  does not become part of the path.
- Width is measured in characters; there is no display-column policy and no
  second layout-preference setting until a real source case needs one.

## Invariants

- Output parses without diagnostics and checks without new checker diagnostics.
- Output reparses to the input's syntax tree, ignoring positions.
  `verify_formatted_output` compares canonical walks and returns a
  `format-equivalence` error instead of writing, for both `fmt` and
  `lint --fix`.
- Output has exactly the parentheses `syntax::grouping::needs_parens` requires,
  the same rule `check.redundant-parens` enforces on source, and adjacent
  tokens are joined through `lexer::join_tokens`. `format_proofs` checks the
  rule over every slot and expression form and over generated trees.
- Formatting is idempotent. A conditional branch kept on one line as
  `{ value }` never breaks inside the value; when the value cannot fit, every
  branch breaks, in an assigned value and in an operand as in an initializer.
- Comments are never duplicated or dropped; `fmt: skip` source is preserved.
- Expression continuations never become separate statements.
- A comprehension value is grouped against the qualifier's rendered boundary:
  an inline keyword or a newline in a broken comprehension. Optional pipeline
  callbacks must not gain redundant parentheses when the qualifier moves to
  the next line.
- Lint diagnostics are unchanged by formatting (`docs/XSHT.md`).

## Tests

The curated corpus is one annotated file, `tests/fixtures/fmt/beauty.xsh`, with
one golden, `tests/fixtures/fmt/beauty.expected.xsh`. Add an annotated section
there when a source shape or the CLI rewrite path is part of the behavior;
`tests/xsh/formatter.xsh::test_fmt_fixture` formats a copy, compares the golden,
then runs `xsht check` and `xsht fmt --check`. Narrow unit contracts go in
`tests/syntax.rs` (`cargo test --release --test integration syntax::`). The sibling
`../laputa` corpus, when present, is a broad stress test, not a golden.
