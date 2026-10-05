# Grammar

The complete syntax of XSH, rendered from the one grammar definition in
`src/syntax/grammar.rs` (`xsht grammar` prints the same productions). The
parser reads its operator, keyword, quoted-literal, stream-stage, and
run-form tables from that definition, and every test run checks the
productions against the parser in both directions: sentences generated from
them parse without diagnostics, and every checked-in source file and
fuzz-generated program is recognized by them.

## Token stream

The productions read the lexer's tokens after three adjustments:

- Comments are dropped.
- A line break followed (after any blank or comment lines) by a
  [continuation token](#line-continuation) is removed, joining the two lines.
- A `\` that follows whitespace and ends its line is whitespace, together
  with its line break. It is accepted only between the parts of a command
  (SPEC 2.5); the productions never see it.
- Every other run of line breaks is one `NEWLINE`.

A newline or `;` ends a statement. A compound statement (one that ends with
a block, such as `if`, `for`, or `proc`) needs no separator after it.

## Notation

| Form | Meaning |
|---|---|
| `"let"`, `"("` | the keyword, contextual word, or punctuation spelled so |
| `~"("` | the token is written directly after the previous one, with no space, line break, or comment between them |
| `A B` | `A` followed by `B` |
| `A \| B` | `A` or `B` |
| `A?`, `A*`, `A+` | optional, zero or more, one or more |
| `list(A)` | `A ("," A)* ","?`, possibly empty, with line breaks allowed around each `A` and `,` |
| `list1(A)` | like `list(A)`, with at least one `A` |
| `line(A)` | `A` written on one line |
| `!(A \| B)` | the following tokens begin with neither `A` nor `B` |
| `&(A \| B)` | the following tokens begin with `A` or `B`, or the input ends |

## Terminals

| Terminal | Meaning |
|---|---|
{{- range .grammar.terminals}}
| `{{.name}}` | {{.description}} |
{{- end}}

Keywords:

```text
{{.grammar.keywords}}
```

Every other quoted word is contextual: it is an ordinary identifier outside
the position shown.

Quoted literal prefixes:

| Prefix | Terminal | Raw (no escapes) |
|---|---|---|
{{- range .grammar.literals}}
| `{{.prefix}}` | `{{.terminal}}` | {{.raw}} |
{{- end}}

Duration suffixes: {{.grammar.duration_suffixes}}.

## Operator precedence

From tightest to loosest. Postfix forms (`.name`, `?.name`, `[i]`, `?[i]`,
calls, `?`) bind tighter than every operator; prefix `!` and `-` bind at
level {{.grammar.prefix_precedence}}; the conversion `value as TYPE` binds at
level {{.grammar.conversion_precedence}} and chains to the left; `is` shares
the equality level; `|>` is looser than every operator.

| Level | Operators | Associativity | Family |
|---|---|---|---|
{{- range .grammar.operators}}
| {{.precedence}} | {{.operators}} | {{.associativity}} | {{.families}} |
{{- end}}

Ordering operators chain (`0 <= i < n`); an ordering never mixes with an
equality, membership, or pattern test without grouping
(`parse.mixed-comparison`), which the productions spell out. The checker's
grouping rules (`check.mixed-logical`, `check.ambiguous-grouping`,
`check.redundant-parens`) apply on top of this grammar.

## Line continuation

An expression continues onto the next line when that line begins with one of
these tokens, none of which can begin a statement (the item expression
`.name` is always postfix at the start of a line):

{{.grammar.continuation}}

A line that starts with `-` (negation) or `/` (an absolute path) starts a new
statement; to split around them, end the first line with the operator.

## Expressions before a block

A condition, `for` iterable, `match` subject, `with` value, or `ctx` message
is followed by a block, so the `condition_` rules read it: there a brace after
a qualified pattern test (`x is E.V`) is the test's payload only when it looks
like one (`{name: P}`, `{..}`), and any other brace opens the block. Inside a
bracket, brace, or parenthesis the general rules apply again.
{{range .grammar.sections}}
## {{.title}}

```ebnf
{{.ebnf}}
```
{{end}}
## Stream stages

A `|>` stage that begins with one of these names is a stream stage; any
other stage is an expression.

| Stage | Takes a block | Takes an inline expression |
|---|---|---|
{{- range .grammar.stages}}
| `{{.name}}` | {{.block}} | {{.inline}} |
{{- end}}

## Run forms

{{range .grammar.run_forms}}`{{.}}` {{end}}

Run options, each at most once: {{.grammar.run_options}}.

## Conditions outside the productions

- A value pipeline stage that is a call uses exactly one `_` placeholder as a
  whole argument, or none (`parse.pipeline-hole`).
- Inside a `match` statement arm or a `with` binding, a `,` ends the
  statement, so a bare command word there cannot contain `,`.
