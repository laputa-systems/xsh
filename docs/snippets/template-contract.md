### Templates

`template.render(source: Str, data: Any) -> Result[Str]` renders a small
subset of Go's text/template. It is pure: a template reads `data` and cannot
call XSH functions, read the environment, or reach the host. The implementation
is the embedded `stdlib/template.xsh`; coverage is
`tests/xsh/stdlib/template.xsh`.

Actions are delimited by `{{` and `}}`:

- `{{.}}` is the current value; `{{.a.b}}` reads fields; `{{$}}` is the
  root data and `{{$name.a}}` reads a declared variable.
- `{{if P}}…{{else if P}}…{{else}}…{{end}}`.
- `{{range P}}…{{else}}…{{end}}` rebinds `.` to each item. `{{range $v := P}}`
  and `{{range $i, $v := P}}` also bind the index or key and the item. Lists
  iterate in order; maps and records iterate in sorted key order; null ranges
  as empty; any other value is an error.
- `{{with P}}…{{else}}…{{end}}` rebinds `.` when the value is truthy;
  `{{with $v := P}}` also binds it.
- `{{define "name"}}…{{end}}` (top level only) declares a named template and
  `{{template "name" P}}` renders it with `.` and `$` bound to the value of
  `P`, or null when `P` is omitted. Calls nest at most 64 deep.
- `{{/* comment */}}` renders nothing. `{{- ` trims white space before the
  action and ` -}}` trims it after; the space is required, so `{{-3}}` is the
  number -3.

Operands are fields, variables, `"…"` strings (escapes `\\ \" \n \t \r`),
`` `…` `` raw strings, decimal integers, `true`, `false`, and `null`. A pipeline
passes each stage's value as the last argument of the next, which must be a
function. The functions are fixed: `len`, `upper`, `lower`, `trim`,
`default DEFAULT X` (X when truthy, else DEFAULT), `join SEP LIST`, `quote`
(JSON string of a scalar's text), `json`, `not`, `and A B` / `or A B` (Go
operand-returning semantics), and `eq A B` / `ne A B` (same-type scalars; null
compares with anything and equals only null). Parentheses and `{{$x := …}}`
assignment actions are not supported.

`false`, `null`, `0`, `0.0`, and empty Str, List, Map, and record values are
falsy; every other value is truthy. Only Str, Int, Float, and Bool render
directly; emitting null, a List, or a Map is an error, so use `default`,
`join`, or `json`.

The whole template parses before anything renders, so a syntax error in an
unexecuted branch still fails. Syntax errors (unclosed action or block,
unknown function, wrong argument count, stray `{{end}}`/`{{else}}`, unknown
`{{template}}` name) report kind `template-syntax`; rendering errors (missing
field, field of a non-map, undefined variable, wrong function operand type,
unrenderable value) report `template-render`. A missing field is always an
error, including inside `if`; represent optional data as null. Messages start
with `template:LINE:COLUMN:`, 1-based, with the column counted in characters.
