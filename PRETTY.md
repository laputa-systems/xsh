This document records the design direction for making `xsht fmt` produce
beautiful XSH source. It is an implementation note, not a second language
specification. Syntax and source-visible language behavior belong in
`docs/SPEC.md`; formatter ownership and invariants belong in `docs/XSHT.md`.

The formatter should make ordinary XSH code pleasant to read without requiring
authors to hand-tune every line. It should preserve meaningful choices when
there is no strong reason to change them, and it should prefer semantic
boundaries over arbitrary width-based cuts.

## What “beautiful” means

Pretty output should have these properties:

- related syntax stays visually together;
- repeated structures use a consistent shape;
- breaks happen at semantic boundaries;
- nested structures have predictable indentation;
- blank lines separate ideas rather than merely responding to line width;
- comments remain attached to the construct they explain;
- output parses, checks, and is idempotent;
- long literals and comments may exceed the width target without creating
  artificial or misleading splits.

The formatter is allowed to normalize layout. It is not a byte-preserving
printer. Original layout is a preference signal, not an unconditional command.

## Current problems to solve

The current formatter is built from direct string emission plus local width
heuristics. That works for simple expressions, but each new construct adds
another special case. The most visible failure modes are:

- a deliberately multiline `if` or `match` expression being collapsed because
  its flat form fits the line width;
- a multiline call argument list being collapsed into one dense line;
- list and map comprehensions losing their authored structure;
- a long method chain breaking inside an argument while the chain itself stays
  on one line;
- a multiline collection mixing compact and expanded records;
- nested records or checksum lists remaining dense because only the outer
  record was expanded;
- automatic blank lines appearing around every multiline statement;
- long URLs, paths, predicates, and generated strings exceeding the width
  target in ways that are unavoidable but need to remain visually deliberate.

The package corpus is a useful stress test because it contains long metadata
records, generated source lists, nested comprehensions, method chains, and
source-shaped calls.

## Layout principles

### Preserve source intent as a tie-breaker

When both flat and broken forms are acceptable, prefer the form the author
already used. This matters most for:

- control-flow expressions;
- call argument lists;
- comprehensions;
- records and lists;
- multiline method chains.

This preference should not preserve accidental one-token-per-line formatting,
cramped layouts, or inconsistent sibling shapes. A source-shape hint should be
weaker than comments, syntax safety, semantic grouping, and the line-width
target.

### Break at semantic boundaries

Preferred break points, in descending order, are:

1. between method-chain calls;
2. between call arguments;
3. between record fields;
4. between collection items;
5. before or after comprehension clauses;
6. between pipeline stages;
7. inside nested expressions only when no better boundary exists.

The formatter should not split strings, paths, comments, interpolation text, or
other indivisible tokens merely to satisfy the width target.

### Keep sibling structures consistent

If a collection is expanded, structurally similar items should normally use the
same shape. Avoid output like this:

```xsh
[
  {source: "short", kind: "file"},
  {
    source: "long source path",
    kind: "file",
  },
]
```

Prefer either compact records when the collection fits, or expanded records for
all sibling records once the collection has committed to a broken layout.
Nested collections should follow the same rule where expansion improves the
parent's readability.

### Use blank lines sparingly

Blank lines should separate logical sections, declarations, major control-flow
constructs, and explicitly separated source regions. A multiline expression is
not automatically a new section. In particular, avoid turning this:

```xsh
let checksum = source_checksum(source)?
stage_source(
  source,
  checksum,
)?
```

into two unrelated-looking paragraphs merely because the call spans lines.

The formatter should preserve an intentional blank line, but should not add one
solely because either neighboring statement happens to be multiline.

## The document model

The long-term implementation should move from ad-hoc string emission toward a
small document model. The exact Rust types can evolve, but the useful
primitives are:

```text
Doc::Text(text)
Doc::Line
Doc::SoftLine
Doc::Indent(amount, doc)
Doc::Group(flat, broken)
Doc::Concat(parts)
```

Each syntax construct should describe its flat and broken alternatives. The
renderer then decides whether a group fits at the current column. A group that
was originally multiline receives a preference for its broken alternative when
both alternatives fit.

This model should support:

- nested groups with independent fit decisions;
- indentation that follows the enclosing construct rather than string state;
- consistent line measurement;
- explicit continuation indentation for pipelines and method chains;
- comment attachment without reconstructing comments from the AST;
- future configuration without multiplying local conditionals.

The AST should continue to provide semantic shape and precedence. The CST
should continue to provide comments, source spans, and source-shape hints. The
document model is the layout layer between those two representations.

## Construct-specific policies

### Calls

Keep a multiline argument list multiline when its breaks occur between
arguments, even if the flattened call fits within the width. Expand arguments
one per line with a trailing comma. A single multiline literal or record may
remain compact when its delimiters and contents are already readable.

Nested calls should make their own layout decisions. A long call should not
force every small nested call to expand.

### Method chains

For a chain that does not fit, keep the first call attached to its receiver and
put later calls on indented leading-dot lines:

```xsh
let names = source.display()
  .replace("/", "_")
  .replace("-", "_")
```

The parser must accept this continuation form. The formatter must never emit a
broken chain that reparses as separate statements.

### Records and lists

Use compact records and lists when they fit and are not source-shaped as
multiline. Once a collection is broken, prefer consistent sibling formatting.
Expand nested records or lists when leaving them compact would create a dense
island inside an already expanded parent.

Long literal values remain indivisible. A record containing an unbreakable URL
may still have a line longer than the configured width.

### Comprehensions

Use a predictable clause layout:

```xsh
let rows = [
  {
    name: item.name,
    value: item.value,
  }
  for item in items
  if item.enabled
]
```

The expression, `for` clause, and optional `if` clause should have stable
indentation. A multiline comprehension must close on its own delimiter line
when the syntax permits it. Trailing `?` expressions and pipeline iterables
must remain parseable across those line breaks.

### Pipelines

Use the existing two-space continuation convention. A pipeline stage should
remain visually associated with its input expression, and nested stage blocks
should indent relative to the stage rather than the entire source line.

Pipeline layout should compose with comprehensions, calls, and method chains
without producing a second unrelated indentation scheme.

### `if` and `match` expressions

Preserve authored multiline shape when the expression is already readable in
that form. When breaking automatically, use the same branch layout as the
corresponding statement form. Match arms should have one arm per line and
trailing commas in multiline expression matches.

### Comments

Comments are layout constraints, not ordinary text. A comment-bearing construct
should not be flattened if doing so would move a comment, change its attachment,
or make the result ambiguous. Leading, trailing, nested, and `fmt: skip`
comments each need dedicated regression cases.

## Beauty corpus

Maintain a curated formatter corpus separate from the large package tree. It
should include compact, authored-multiline, cramped, width-boundary, and
comment-bearing examples for:

- calls with positional, named, and splice arguments;
- records containing nested records and lists;
- homogeneous and heterogeneous collections;
- list and map comprehensions with filters, `?`, and pipelines;
- short and long method chains;
- `if` and `match` expressions in bindings, calls, and records;
- formatted strings, paths, comments, and `fmt: skip`;
- top-level statements and nested blocks;
- generated-code-like long literals and predicates.

For each example, test at least:

1. formatting the original source;
2. parsing and checking the result;
3. formatting the result again;
4. comparing the second result with the first;
5. reviewing the expected output as a human-readable golden.

The package repository remains valuable as a broad integration corpus, but it
should not be the only source of formatter expectations. Small fixtures make a
layout regression easy to understand.

## Verification invariants

Every formatter change should preserve these invariants:

- formatted output has no parser diagnostics;
- checked output has no new checker diagnostics;
- formatting is idempotent;
- comments are neither duplicated nor silently dropped;
- `fmt: skip` source is byte-preserved;
- line width is respected where tokens are breakable;
- unbreakable literals and comments are allowed to exceed the target;
- formatter output cannot turn an expression continuation into separate
  statements;
- source-shaped multiline constructs do not collapse without a deliberate
  policy reason.

The narrow formatter gate is `cargo test --test syntax`. The package corpus can
be checked with `xsht fmt --check` after formatting it with the development
binary.

## Roadmap

1. Finish source-shape preservation for calls, comprehensions, records, and
   control-flow expressions.
2. Replace automatic blank lines around multiline statements with a logical
   section policy.
3. Make sibling collection items choose a consistent flat or broken shape.
4. Expand nested records and collections when their parent is already broken.
5. Introduce the document model and migrate constructs incrementally.
6. Grow the beauty corpus as each construct moves to document-based layout.
7. Revisit width configuration and layout preferences only after the structural
   model is stable.

The goal is not maximum normalization. The goal is source that reads as if a
careful human made the important layout decisions, while routine formatting
remains automatic and safe.
