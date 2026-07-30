# XSHT Formatter Design

`xsht fmt` makes ordinary XSH source readable without requiring authors to
hand-tune every line. It is a normalizing formatter, not a byte-preserving
printer: syntax and semantic structure come from the AST, while comments,
source spans, and meaningful layout clues come from the CST.

The formatter owns layout. The language contract remains in `docs/SPEC.md`.
This document records the formatter's design rationale, implementation handles,
verification contract, and intentionally deferred work.

## Design Philosophy

Beautiful output has a few practical properties:

- related syntax stays visually together;
- repeated structures use a consistent shape;
- breaks happen at semantic boundaries;
- nested structures have predictable indentation;
- blank lines separate ideas instead of reacting to line width;
- comments remain attached to the construct they explain;
- output parses, checks, and is idempotent;
- indivisible literals and comments may exceed the width target rather than
  being split into misleading fragments.

The formatter may normalize layout. Original layout is a preference signal, not
an unconditional command. Source intent wins when flat and broken forms are
both readable, but comments, syntax safety, semantic grouping, and width remain
stronger constraints.

The design specifically avoids these failure modes:

- collapsing a deliberately multiline `if` or `match` because its flat form
  happens to fit;
- collapsing a multiline call argument list into a dense line;
- losing authored structure in list and map comprehensions;
- breaking a method chain inside an argument instead of between calls;
- mixing compact and expanded sibling records in one broken collection;
- leaving nested records or checksum lists dense inside an expanded parent;
- adding blank lines merely because a neighboring statement is multiline;
- splitting or otherwise disguising long URLs, paths, predicates, and generated
  literals that are inherently difficult to break.

## Implementation Handles

| Concern | Symbols | Owner and coverage |
|---|---|---|
| formatter entry and output | `Formatter`, `FormatOutput`, `format_files` | `crates/xsht/src/format.rs`, `crates/xsht/src/cli/fmt.rs`; `crates/xsht/tests/cli.rs` |
| source-faithful input | `SyntaxTree`, `SyntaxTree::from_token_table`, `apply_cst_guarded_edits` | `src/syntax/cst.rs`, `crates/xsht/src/edit.rs`; CST and formatter tests |
| layout decisions | `canonical_parens_when_empty`, `expr_precedence`, `needs_top_level_blank` | `src/syntax/node.rs`, `crates/xsht/src/format.rs`; `tests/syntax.rs` |
| document rendering | `Doc`, `DocRenderer`, `prefer_broken` | `crates/xsht/src/format.rs`; formatter layout tests |
| disk-backed corpus | `test_fmt_fixture`, `assert_fmt_fixture` | `tests/xsh/formatter.xsh`, `tests/fixtures/fmt` |

These names are the retrieval handles for formatter work. The visual policies
below are implementation policy, not an alternate syntax specification.

## Source Representations

The AST and CST have different jobs.

The AST is semantic. It supplies expression shape, precedence, declarations,
and checked program structure. `Formatter` uses it to choose safe syntax and
to decide which constructs can be rendered compactly.

The CST is source-faithful. It retains tokens, comments, whitespace, skipped
source gaps, delimiters, interpolation groups, and exact source spans. The
formatter uses it to answer source-fidelity questions:

- whether a construct contains comments;
- which original tokens and trivia belong to a range;
- whether a source-shaped group was intentionally multiline;
- where leading and trailing comments attach;
- whether a `# fmt: skip` statement can be copied byte-for-byte.

The document layer sits between those representations. The AST decides what a
construct means, the CST supplies source intent and comment constraints, and
the document renderer chooses flat or broken layout.

## Document Model

`Doc` and `DocRenderer` in `crates/xsht/src/format.rs` provide the layout layer.
The useful primitives are text, hard and soft lines, indentation, concatenation,
and groups with flat and broken alternatives. A group can set `prefer_broken`
when the original source or the construct policy requires preserving a readable
multiline shape even if its flat form would fit.

Groups make nested constructs decide independently. This prevents one long call
from expanding every small nested call, while allowing an expanded collection
to expand nested records when a compact island would make the parent harder to
read. Indentation follows the enclosing construct rather than accumulated string
state, and the renderer measures the same text that it emits.

## Layout Policies

### Source intent as a tie-breaker

When flat and broken forms are both acceptable, prefer the form the author
already used. This applies especially to control-flow expressions, call
argument lists, comprehensions, records and lists, and multiline method chains.
Do not preserve accidental one-token-per-line formatting, cramped layouts, or
inconsistent sibling shapes merely because the source used them.

### Semantic break points

When a construct must break, preferred boundaries are:

1. between method-chain calls;
2. between call arguments;
3. between record fields;
4. between collection items;
5. before or after comprehension clauses;
6. between pipeline stages;
7. inside nested expressions only when no better boundary exists.

Strings, paths, comments, interpolation text, and other indivisible tokens are
not split merely to satisfy the width target.

### Calls and method chains

An authored multiline argument list remains multiline when its breaks occur
between arguments. Broken argument lists use one argument per line and a
trailing comma. Nested calls make their own decisions.

Long method chains keep the first call attached to its receiver and put later
calls on indented leading-dot lines. The emitted continuation must parse as one
expression rather than separate statements.

### Records, lists, and comprehensions

Compact records and lists remain compact when they fit and are not source-shaped
as multiline. Once a collection is broken, structurally similar siblings use a
consistent shape. Nested records and collections expand when leaving them
compact would create a dense island inside an already broken parent.

Multiline comprehensions use stable continuation lines for the expression, the
`for` clause, and the optional `if` clause. Their closing delimiter gets its own
line when the syntax permits it. Trailing `?` expressions and pipeline iterables
remain part of the same expression across those breaks.

### Pipelines and control flow

Pipeline stages use the existing two-space continuation convention. Nested stage
blocks indent relative to the stage, and the same continuation style composes
with calls, comprehensions, and method chains.

Authored multiline `if` and `match` expressions keep their readable branch
shape. Automatically broken expressions use the corresponding statement layout;
multiline expression matches put one arm per line with trailing commas.

### Blank lines

Blank lines express logical sections, declarations, major control-flow
constructs, or an authored blank line. A multiline call, collection, pipeline,
or control-flow expression does not create a blank line merely because it uses
more than one output line. `needs_top_level_blank` owns this top-level section
policy.

### Comments and `fmt: skip`

Comments are layout constraints, not ordinary text. Leading comments remain
leading comments, same-line trailing comments stay with their complete
statement, and nested comments prevent AST-only regeneration unless the
formatter can deliberately reattach them.

`# fmt: skip` applies to the next statement and preserves that statement's raw
source. The directive itself remains in the formatted output.

## Configuration

`format.line-width` from the nearest `xsht-config.ini` controls the layout target;
the default is 120 columns. The target is not a hard maximum. Unbreakable
strings, paths, comments, and `fmt: skip` regions may exceed it.

The nearest configuration behavior is covered by
`crates/xsht/tests/cli.rs::fmt_uses_nearest_xsht_config_line_width`. Formatter
policy does not add a second layout-preference configuration surface until the
structural model needs one.

## Beauty Corpus

The curated disk-backed corpus is one annotated source file at
`tests/fixtures/fmt/beauty.xsh` with one checked-in golden at
`tests/fixtures/fmt/beauty.expected.xsh`. Its annotated sections cover:

- positional and nested call arguments, including a multiline call;
- method chains;
- source-shaped lists and comprehensions;
- sibling and nested collection expansion;
- `if` and `match` expressions;
- leading, trailing, nested, and `fmt: skip` comments;
- formatted strings, long URLs, paths, and generated-code-like literals.

`tests/xsh/formatter.xsh::test_fmt_fixture` copies the source to a temporary
file, runs `xsht fmt`, compares the golden output, runs `xsht check`, and runs
`xsht fmt --check` to verify idempotency through the CLI. Keep the fixture
monolithic and add clearly annotated sections so a layout regression remains
easy to understand without maintaining a directory of tiny files.

When available, the package corpus at `../packages` remains a useful broad
stress test for long metadata records, generated source lists, nested
comprehensions, method chains, and source-shaped calls. It is an integration
corpus, not a substitute for small annotated sections with one intentional
golden.

Use `tests/syntax.rs` for a narrow formatter unit contract and add an annotated
section to `tests/fixtures/fmt/beauty.xsh` when the source shape or CLI rewrite
path is part of the behavior.

## Verification Invariants

Formatter changes preserve these invariants:

- formatted output has no parser diagnostics;
- checked output has no new checker diagnostics;
- formatting is idempotent;
- comments are neither duplicated nor silently dropped;
- `fmt: skip` source is byte-preserved;
- breakable tokens respect the configured width target;
- unbreakable literals and comments may exceed that target;
- expression continuations cannot become separate statements;
- source-shaped multiline constructs do not collapse without a policy reason.

The narrow Rust gate is `cargo test --test integration syntax::`. The CLI corpus
gate is
`target/debug/xsht test --exact tests/xsh/formatter.xsh::test_fmt_fixture`
after `cargo build --bin xsht`; the broader native-test command is `xsht test`.

## Deferred Work

The current formatter is complete for the design above. These are the follow-ups
worth doing, in priority order, when formatter work resumes:

1. **Finish the fmt fixture.** Add annotated sections for named and splice
   arguments, pipeline stage blocks, multiline `?` expressions, `if` and `match`
   nested in calls or records, and comments inside multiline collections and
   calls. Keep the single source/golden pair routed through
   `tests/xsh/formatter.xsh::test_fmt_fixture`.
2. **Complete the document-model migration.** `Doc`, `DocRenderer`, and group
   selection now cover the most sensitive call-argument layout, but much of
   `crates/xsht/src/format.rs` still emits strings directly. Migrate one
   construct family at a time when changing its layout policy; do not rewrite
   the formatter solely for architectural purity.
3. **Strengthen comment and trivia fidelity.** Add CST-backed regression cases
   for comments between arguments or collection items, comments adjacent to
   delimiters, multiple authored blank lines, and `fmt: skip` next to trailing
   comments. Preserve the conservative fallback when a construct cannot be
   regenerated without risking comment movement.
4. **Make width measurement display-aware.** The current width accounting uses
   character counts. If XSH source begins depending on tabs, wide Unicode, or
   combining characters in layout-sensitive code, switch the renderer to an
   explicit display-column policy and add boundary fixtures.

Do not add a large configuration surface, byte-preserving mode, or speculative
line-breaking rules yet. Those would make formatter behavior harder to reason
about without solving a demonstrated layout problem.
