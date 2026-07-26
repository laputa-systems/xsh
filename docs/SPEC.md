# XSH Specification

This is the living implementation contract for XSH.
`docs/XSH-GUIDE.md` indexes generated tutorial material, `docs/STDLIB.md` is the
generated standard-library API manual, and `docs/REFERENCE.md` is the generated
non-stdlib language and tooling reference. When those documents disagree,
`docs/SPEC.md` is authoritative for core language behavior; update this file
before changing language behavior.
`docs/SPEC-TYPING.md` is the detailed contract for typechecking, including
assignability, `Any`, strict dynamic checking, schema check boundaries, and
flow-sensitive narrowing.
`docs/SPEC-OS.md` is the detailed contract for OS-facing runtime behavior,
including signal handlers, evaluator checkpoints, process-group cancellation,
and signal hook shutdown paths.

The executable examples in `examples/` and standalone programs in `showcase/`
are part of this contract. Each `.xsh` example must be cataloged, and each
standalone showcase script must be covered by a native test in `showcase/tests/`.

`LANG.md` tracks open language design proposals. **When a proposal is
implemented:** remove its entry from `LANG.md` entirely — implemented behaviour
belongs here in `docs/SPEC.md` and in the relevant `docs-src/CHAPTER-*.md.in`
template, not in `LANG.md`. Follow the full checklist in `LANG.md §Process`
and reference the entry in the commit message.

## Section Map

| Area | Section |
|---|---|
| source, spans, diagnostics | 2 |
| lexical rules | 3 |
| programs and statements | 4 |
| types and values | 5 |
| expressions | 6 |
| pure functions and procs | 7 |
| control flow and results | 8 |
| commands | 9 |
| process execution | 10 |
| argv conversion | 11 |
| status | 12 |
| standard modules | 13 |
| structured streams | 14 |
| builder blocks | 15 |
| JSON | 16 |
| resolver and checker | 17 |
| tracing and tracebacks | 18 |
| CLI | 19 |
| native tests and fixtures | 20-21 |

## Philosophy

XSH addresses a specific gap: scripts that have outgrown bash but do not
warrant a full application runtime. The design resolves a tension — shell
ergonomics are unmatched for process orchestration, but shell semantics
collapse under complexity. XSH keeps the ergonomics and replaces the
semantics. The language prefers predictable source-visible behavior over hidden
cleverness: if a feature needs a scheduler, implicit rewrite, ambient policy, or
implementation-specific exception to explain it, it is probably outside XSH's
tier.

**Types without ceremony.** Every value has a type: paths, integers, records,
durations, digests. Inference carries context forward; types appear at
module boundaries and function signatures, not in every binding. The rule is
that the type system prevents mistakes, not that it demands declarations.

**Explicit at boundaries.** The dangerous part of shell is invisible
behavior: word splitting, glob expansion, silent type coercions, commands
that succeed when they should fail. XSH makes every boundary visible. `@xs`
splices argv. `${expr}` interpolates a value. `?` propagates an error. `run`
names the process boundary. Code shows where xsh-land ends and subprocess-land
begins.

**Runtime graphs, source trees.** XSH source stays tree-shaped on purpose:
ordinary files, blocks, calls, and pipelines that remain readable and
greppable. Running code is graph-shaped. Process invocations, cwd and env
scopes, stream stages, parallel jobs, file resources, and propagated errors
create runtime relationships that are not identical to the AST. Tracing exposes
that runtime graph as structured evidence while source spans anchor events back
to the code that created them.

**Predictable execution model.** The language is specified for direct evaluation
of checked source structure, not for a hidden bytecode VM or application
scheduler. An implementation may optimize, but those optimizations must preserve
observable order, explicit boundaries, and traceable failures. They must not
expose futures, green threads, callbacks, implicit event loops, or VM-specific
behavior as part of the language contract. XSH's concurrency units are host
processes and structured stream stages because those are the useful units for
systems glue.

**Methods are the primary API surface.** Operations on values are methods:
`xs.len()`, `m.set(k, v)`, `s.trim()`, `p.read_text()`. The receiver comes
first because that is how the operation is read: "the list's length", "set
this key on the map". Module functions exist only where there is no natural
receiver — factories like `map.empty()` and constructors like
`regex.compile()`. The `lint.prefer-method` rule enforces this and autofixes
violations.

**Pipelines, not text plumbing.** `|>` is a typed operator. Each stage knows
what flows through it. The pipeline result is a `List[T]`, collected
automatically. There is no word splitting, no IFS, no globbing, no xargs.
The pipeline describes a transformation; the types describe what is being
transformed.

**Results, not exceptions.** Fallibility is part of the signature. `?`
propagates, `??` recovers, `match` handles. Nothing can throw without the
caller seeing `?` in the code. The error kind is a plain string, not a class
hierarchy. Code that looks clean either handles errors or explicitly
propagates them.

**Greppable by default.** XSH code must be searchable with ordinary text
tools. This means no sigils that change meaning based on context, no implicit
coercions that hide the underlying operation, no overloaded syntax. Every
call to `f(x)` is findable with `grep 'f('`. Every method call `.m(` is
findable without understanding the type. The AST-aware `xsht grep` tool
extends plain grep to handle whitespace and expression boundaries correctly,
but plain grep must also be sufficient for most searches.

**Testable at every layer.** The language has mocks, temp files, and
assertions built in. Tests are typed procs; the test module is a standard
library, not a framework import. Coverage is structural — measured over the
API surface — rather than only over lines. Scripts that are not tested are
not complete.

**Spec-first.** The spec is the contract. Implementation serves the spec;
the spec is not documentation of the implementation. When behavior changes,
the spec is updated first, then the implementation, then the tests and
examples. A feature without a spec entry does not exist.

## Interactive Use

XSH has a separate interactive command surface for the first minutes of use:
`xshi` accepts the small commands people naturally try while exploring a
directory, checking paths, or sketching a one-line transformation. This surface
is deliberately a compatibility layer, not a second shell language, and it is
not part of ordinary `.xsh` script execution.

`docs/SPEC-INTERACTIVE.md` is authoritative for the full `xshi` contract,
including startup, session state, shell-subset parsing, prompt rendering,
history, completion, autosuggestions, denv, and tests. This section records only
the core language boundary that matters to normal XSH.

The design goal is fast orientation without importing shell semantics into
scripts. XSH-classified interactive commands use XSH command-argument
evaluation: quoted text stays one argument, interpolation is explicit, typed
command arguments keep their value conversions, and list splices must be written
with `@`. Core utilities are ordinary `.xsh` scripts on `PATH`; they are not
resolved through a native compatibility registry. In normal `.xsh` files a bare
utility name is an unresolved proc command unless the script defines it.

Input that is not classified as XSH may be handled by the `xshi` shell-subset
frontend. That frontend is still interactive-only. It may provide shell-style
word conveniences such as variables, command substitution, and glob expansion,
but those conveniences do not become normal `.xsh` syntax and do not apply to
`xsh` or `xsht`.

Native xshi session builtins return integer status values. `0` means success,
`1` means an ordinary command failure, and `2` means usage or unsupported input.
The runtime also records the status for `$?`.

The REPL has a small amount of session state. `exit` leaves the REPL using the
last status, `exit N` leaves with that status, and `cd PATH` changes the host
working directory so subsequent entered lines see the new directory. This
session-level `cd` does not change the scoped `cd { ... }` command used in
scripts.

Core utility names such as `ls`, `cat`, `rg`, and `tree` are ordinary PATH
commands in `xshi`. Scripts should use canonical XSH forms such as `print`,
`Path.read_text()?`, `fs.*`, `time.*`, `process.which`, structured stream
operations, explicit `run`, scoped `cd`, scoped `env`, and ordinary control
flow.

## 1. Status

The implemented v1 surface includes:

- UTF-8 source files with byte-offset spans and rendered line/column positions.
- Comments beginning with `#`.
- Newline and semicolon statement terminators.
- `use`, `export`, `let`, `var`, `proc`, `pure`, `type`, `return`, `defer`,
  `if`, `else`, `while`, `for`, `break`, `continue`, and `match`.
- Required parameter lists, required return annotations for `pure`, default
  `Result[Unit]` returns for annotation-free `proc`, typed defaults for simple
  defaulted parameters, plus default and rest parameters.
- Expression-style pure and proc calls, plus fully qualified standard-module
  command calls for effectful APIs returning `Result[Unit]`.
- `Ok(value)`, `Err(value)`, `Result`, postfix `?`,
  right-associative `??` fallback, `Result.context(...)`, and implicit
  `Ok(value)` wrapping for `Result[T]` tail values.
- User-defined tag union types with exhaustiveness checking: `type Level = Info | Warn | Error(Str)`.
  Exhaustive `match` emits `check.non-exhaustive-match` for uncovered variants.
  `lint.stringly-typed-match` flags ≥ 3 string-literal match arms.
- Narrow function and task tail values: a final expression or command statement
  can produce the declared block value.
- Final top-level `Int` or `UInt` values become the script process exit status.
- `abort(status: Int, force: Bool = false)` exits immediately; deferred cleanup
  runs unless `force` is true.
- Explicit external execution through `run`, byte pipelines, redirections,
  scoped environment and cwd, timeouts, captures, process streams, status
  values, `spawn`/`wait` process handles, cancellation, and `$?`.
- Entry-script signal hooks for bounded shutdown handling at evaluator
  checkpoints.
- Structured streams with bounded parallel stages, adapters, and the accepted
  finite operator set.
- API-owned builder blocks for `process.command`.
- Core commands `print`, `eprint`, scoped `cd`, and scoped `env`.
- Native interactive compatibility commands in `xshi`, described in
  `Interactive Use`.
- Native XSH tests under the current directory's `tests/**/*.xsh` and
  `showcase/tests/**/*.xsh`, the `test` standard module, opt-in example
  integration tests, and API coverage reports through `xsht test --cov`.
- ELF file inspection through `elf.inspect(path)`, including non-error
  `type: "not-elf"` results for ordinary non-ELF files and dynamic dependency
  metadata for ELF files.
- Command interpolation through `${expr}` plus `$name` and `$record.field`
  shorthand.
- `Null`, `Bool`, `Int`, `UInt`, `Float`, `Duration`, `Str`, `Bytes`, `Digest`, `Regex`, `Path`,
  `Any`, `List`, `Map`, `Stream`, `Record`, `Module`, `Result`, `Status`, `Error`,
  `ProcessError`, `ProcessHandle`, `Command`, `Pure`, `Proc`, and `Unit`.
- Standard modules are available without `use`; module, method, record,
  builtin type-name, and builtin error API metadata is defined by the internal
  language registry and generated module, method, and record API signatures
  live in `docs/STDLIB.md`.
- Text and JSON-lines traces, method trace events, tracebacks, `xsh`, `xshi`,
  and `xsht`.

The following remain outside v1 unless this spec later promotes them:
shell-string process execution, first-class command block literals, slice
syntax, block-valued named arguments, public tagged JSON, doc comments,
multi-job interactive job control, script-level job-control syntax, service
supervision, command-compatible Seed applet shims, and
package-manager-specific grammar.

## 2. Source, Spans, And Diagnostics

Source files are UTF-8. Invalid UTF-8 is reported as a source-loading
diagnostic.

Spans are byte-offset ranges into a source file. Diagnostics render one-based
line and column positions. Columns are measured in Unicode scalar values for
display, but byte offsets are authoritative for tooling.

Line endings are normalized for lexing: `\n` and `\r\n` both terminate a line.
Rendered diagnostics should preserve the original source line text.

Diagnostics from source loading, lexing, parsing, checking, linting,
formatting, and evaluation must include a source span when one exists. Human
rendering and machine-readable diagnostics must be derived from the same
diagnostic value.

## 3. Lexical Rules

Whitespace separates tokens outside strings and command words. Comments begin
with `#` outside strings and run to the end of the line. A comment may appear on
its own line, or after a complete statement where a newline, semicolon, `}`, or
end of file would be valid. End-of-line statement comments are trivia attached
to the preceding statement; they are not part of the semantic AST.

Reserved keywords:

```text
and break continue defer else false for if in let match not null or proc pure
retry return run spawn stream true type use var wait while yield
```

`not` is reserved only as part of the binary `not in` operator. Unary negation
is `!`.

Contextual command words include `env`, `export`, and builder-owned entries
such as `source`, `task`, `command`, and `exec` in accepting builder blocks.

Contextual type and constructor names:

```text
Bool Bytes Command Digest Duration Err Error Int List Map Null Ok Path Proc
ProcessHandle Pure Record Regex Result ProcessError Status Str Stream UInt Unit
```

Identifiers used in expressions match:

```text
[A-Za-z_][A-Za-z0-9_]*
```

The standard module names are the module entries in the internal
language-facing registry that generates `docs/STDLIB.md`. Those names are
reserved in simple binding and alias positions so module namespaces cannot be
shadowed or aliased by ordinary declarations. Record destructuring may bind
fields with those names because standard record schemas commonly contain fields
such as `path`.
`args` is a special case: it is also the predeclared script argument value, so
ordinary bindings named `args` are allowed for compatibility; qualified
`cli.parse(...)` still resolves to the standard module.

Command and proc identifiers additionally allow `-` after the first character:

```text
[A-Za-z_][A-Za-z0-9_-]*
```

String literals use double quotes and support `\\`, `\"`, `\$`, `\n`, `\r`,
`\t`, `\0`, `\xNN`, and `\u{HEX}`. Triple-quoted string literals use
`"""..."""` and support the same escapes while allowing newlines. Raw string
literals use `r"..."` or `r"""..."""`; their contents are literal text and
escapes are not decoded. A `Str` literal must decode to valid UTF-8 and cannot
contain NUL when converted to a path, environment value, or argv item.
`lint.redundant-newline-triple-string` flags the exact single-newline triple
string form and autofixes it to the equivalent escaped string literal `"\n"`.

Expression string literals do not interpolate. `${expr}` interpolation is
recognized only in command words and quoted command word parts. `$name` and
`$record.field` are accepted shorthand there for simple binding or field-access
chains. Arbitrary expressions still require `${expr}`.

Bytes literals are `b"..."` and support the same byte escapes except
`\u{HEX}`. They produce `Bytes`.

Path literals are `p"..."` and support the same escapes as string literals.
They produce `Path` and do not interpolate.

Formatted path literals are `fp"..."` or `fp"""..."""` and support `${expr}`
interpolation with display conversion. They produce `Path`.

Obvious path literals may be written without `p` when they begin with `/`,
`./`, or `../` and contain no whitespace or delimiters. These produce `Path`
values:

```xsh
let root = ./target/build
let cc = /usr/bin/cc
let parent = ../src/main.c
```

Glob literals are `g"..."` and support the same escapes as string literals.
They expand against the evaluator cwd and produce `List[Path]`. Globbing is an
effect and is rejected in pure functions. Wildcards are explicit only; ordinary
command words never glob.

Integer literals are decimal or octal. Octal literals use `0o` followed by
octal digits, such as `0o755`. A leading `-` is parsed as unary minus rather
than as part of the literal.

`UInt` is the non-negative integer type.

Float literals require a decimal point followed by at least one digit or an
exponent: `1.0`, `0.25`, `10e-3`, and `1.5e6`. They produce `Float` values.
The runtime representation is IEEE 754 binary64.

Duration literals are decimal integers followed immediately by `ms`, `s`, `m`,
or `h`. They produce `Duration` values.

Display strings use `f"..."` or `f"""..."""` and support `${expr}`
interpolation with display conversion. Interpolation scans balanced braces,
brackets, parentheses, and string literals inside the expression, so nested
record literals, `if` expressions, `match` expressions, and inner strings do
not terminate the interpolation early. The same interpolation scanner is used by
formatted path literals. `\${` writes a literal interpolation marker. Ordinary
expression string literals and raw string literals still do not interpolate.

## 4. Programs And Statements

Top-level statements execute in order. If a `proc main` is defined at the top
level and has not been explicitly called by the final top-level statement, it
is automatically invoked with `args` after all other top-level statements
complete. `lint.redundant-main-call` flags and autofixes an explicit
`main(@args)` call when implicit invocation applies.

Script arguments after `--` are available through the predeclared immutable
binding `args: List[Str]`. The current interpreter also accepts `ARGV` as a
compatibility alias; new examples and docs should use `args`.

```xsh
for arg in args {
  print ${arg}
}
```

Statement forms:

```ebnf
program      = statement* EOF ;

statement    = use_stmt
             | export_stmt
             | let_stmt
             | var_stmt
             | assign_stmt
             | proc_def
             | pure_def
             | stream_def
             | type_def
             | return_stmt
             | yield_stmt
             | if_stmt
             | while_stmt
             | for_stmt
             | with_stmt
             | break_stmt
             | continue_stmt
             | match_stmt
             | defer_stmt
             | command_stmt
             | expr_stmt
             ;

terminator   = NEWLINE | ";" ;
block        = "{" block_params? statement* "}" ;
block_params = "|" IDENT ("," IDENT)* "|" ;
```

Declarations:

```ebnf
use_stmt     = "use" module_path ("as" IDENT)? terminator ;
module_path  = module_segment ("." module_segment)* ;
module_segment = IDENT | PROC_IDENT ;
export_stmt  = "export" (let_stmt | proc_def | pure_def | stream_def | type_def) ;

let_stmt     = "let" binding_target type_ann? "=" expr_or_run terminator ;
var_stmt     = "var" binding_target type_ann? "=" expr_or_run terminator ;
binding_target = IDENT | record_binding_target ;
record_binding_target = "{"
                 (destructure_field ("," destructure_field)* ","?)? "}" ;
destructure_field = IDENT | ".." ;
assign_stmt  = assign_target assign_op expr_or_run terminator ;
assign_target = IDENT ("." IDENT | "[" expr "]")* ;
assign_op    = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;
type_def     = "type" IDENT "=" type_body terminator ;
type_body    = type_expr | record_schema | module_contract ;
record_schema = "{" schema_field ("," schema_field)* ","? "}" ;
schema_field = IDENT ":" type_expr ;
module_contract = "module" "{" module_contract_entry* "}" ;
module_contract_entry = "export" "optional"? module_contract_kind terminator? ","? ;
module_contract_kind = ("let")? IDENT ":" type_expr
             | "proc" IDENT param_list effect_ann? "->" type_expr
             | "pure" IDENT param_list "->" type_expr ;
type_ann     = ":" type_expr ;
defer_stmt   = "defer" expr_or_run terminator ;
yield_stmt   = "yield" expr_or_run terminator ;
```

Module path segments accept hyphenated identifiers (proc-ident form) in addition
to ordinary identifiers. When the final module-path segment contains a hyphen, an
explicit `as` alias is required because the hyphenated form is not a valid
binding name in expression context. The checker rejects hyphenated-final-segment
`use` declarations without `as` with `check.hyphenated-module-alias`.

`let` bindings are immutable. `var` bindings are mutable. Assigning to a `let`
binding or an undefined name is a checker error. Assignment targets may name a
mutable local binding directly, a field below a mutable local record, or a
string-keyed entry below a mutable local map. Field and indexed assignment
update the local value stored in the root binding; they do not introduce shared
record or map identity. Compound assignment requires a mutable target and
follows the corresponding binary operator type rules for the target value.
Record destructuring targets bind named fields from a record value; `..` marks
ignored remaining fields. Destructured `let` and `for` bindings are immutable,
destructured `var` bindings are mutable. `export let` accepts simple names
only.

Control flow:

```ebnf
while_stmt   = "while" expr block ;
for_stmt     = "for" binding_target "in" expr block ;
break_stmt   = "break" terminator ;
continue_stmt = "continue" terminator ;
match_stmt   = "match" expr "{" match_arm* "}" ;
match_arm    = pattern guard? "=>" (statement | block) ","? ;
guard        = "if" expr ;
pattern      = "_" | IDENT | type_pattern | literal | constructor_pattern
             | record_pattern | pattern "|" pattern ;
type_pattern = ("_" | IDENT) "is" type ;
constructor_pattern = IDENT "(" pattern? ")" ;
record_pattern = "{" record_pattern_field ("," record_pattern_field)* ","? "}" ;
record_pattern_field = IDENT (":" pattern)? | ".." ;
```

`break` and `continue` affect the nearest `while` or `for`. They are checker
errors inside structured stream stage blocks.

`defer` registers a block-scoped cleanup expression or run command. Cleanups
run in last-in-first-out order when control leaves the block through success,
`Err` propagation, runtime failure, `return`, loop control, or cancellation.
Cleanup failures are reported without hiding a primary failure.

Standard modules are built-in namespaces and cannot be aliased. User modules are
imported from sibling `.xsh` files relative to the importing source file, then
from each directory in `XSH_MODULE_PATH` when the file-relative path does not
exist. `XSH_MODULE_PATH` uses the host platform's path-list separator. User
modules may export `let`, `proc`, `pure`, `stream`, `type`, and `error`
declarations.
Non-exported module bindings, types, and error families remain local to the
imported module. `use helper` imports exported values, types, and error
families as bare names and also binds `helper` as a module namespace.
`use helper as h` exposes exported values through `h` and exported types in type
positions as `h.TypeName`; types and error families are compile-time names and
are not fields in the runtime alias record. Imported modules may contain
top-level `use`, `let`, `proc`, `pure`, `stream`, `type`, and `error`
declarations, but not top-level mutation, commands, or control flow.

Surface-only conveniences are lowered before checking and evaluation. The core
lowering includes path literals and p-strings, typed environment fields to an
environment lookup node, value pipeline calls to ordinary calls with the
pipeline input inserted, and builder syntax to module-owned builder calls.
Formatting preserves the readable surface form.

## 5. Types And Values

Type expressions:

```ebnf
type_expr    = IDENT
             | IDENT "." IDENT
             | "List" "[" type_expr "]"
             | "Map" "[" type_expr "]"
             | "Stream" "[" type_expr "]"
             | "Module" "[" type_expr "]"
             | "Result" "[" type_expr "]"
             | "Result" "[" type_expr "," type_expr "]"
             | type_expr "?"
             ;
```

`Result[T]` means `Result[T, Error]`. `T?` is an optional type: the value is
either `null` or a value of type `T`. Postfix `?` on a type expression is
distinct from postfix `?` on a value expression (which propagates `Err`).

User-defined aliases bind a type name to another type expression. Record
schemas bind a type name to required fields with fixed types. Module contracts
bind a type name to a checked runtime module export shape. Tag unions bind a
type name to a set of named variants, each with zero or more payload fields.

**Module contracts** use the `type T = module { ... }` form. Each entry names
an exported runtime binding. Value exports use `export let name: Type` or the
short form `export name: Type`. Exported proc and pure entries include their
call signatures. `optional` permits the export to be absent after a dynamic
module check.

```xsh
type BuildPlugin = module {
  export let name: Str
  export optional let description: Str
  export proc build(root: Path) [fs, process, error] -> Result[Unit]
  export pure label(name: Str) -> Str
}
```

**Tag unions** use the `type T = A | B | C(Type, ...)` form:

```xsh
type Level = Info | Warn | Error | Debug
type Result2 = Ok(Str) | Err(Int, Str)
```

Each variant is a constructor. Zero-field variants are bare names; non-zero
variants are called as functions: `Info`, `Stopped("disk full")`. Tag union
values are matched with constructor patterns. When the matched value is a
`Tag(T)` type and no arm is a wildcard or binding, the checker emits
`check.non-exhaustive-match` for uncovered variants.

Runtime values are distinct by type:

- `Null` (the absent value, produced by `null` literals and by `?.` when base is null)
- `Bool`
- `Int`
- `Float`
- `Duration`
- `Str`
- `Bytes`
- `Digest`
- `Regex`
- `Path`
- `List`
- `Map`
- `Record`
- `Module`
- `Result`
- `Status`
- `Error`
- `ProcessError`
- `ProcessHandle`
- `Command`
- `Stream`
- `Pure`
- `Proc`
- `Unit`
- `Tag` (user-defined tag union variant with name and zero or more fields)

`Optional[T]` (written `T?` in type position) is not a distinct runtime value
kind — it is a type annotation that permits either `Null` or a `T` value. The
`null` keyword produces `Null`. A `T?` parameter or binding accepts both.

`Stream[T]` is a one-pass structured stream value. Direct `for` loops and
structured pipelines consume it lazily. `.collect() -> List[T]` drains a stream
and materializes its remaining items when random access, length, or list APIs
are required.

`Str` is valid UTF-8 text. `Bytes` is arbitrary byte data. `Path` stores native
Unix path bytes and cannot contain NUL; it can represent paths that are not
valid UTF-8. `Map[T]` is a deterministic string-keyed collection whose values
all have type `T`.

`Float` is an IEEE 754 binary64 scalar for measured quantities such as rates,
percentages, load averages, and JSON metrics. Float equality is exact over the
runtime value's binary representation. Display conversion renders finite values
as decimal text and renders non-finite values as `NaN`, `Infinity`, or
`-Infinity`. Sort keys use the IEEE total order, so `NaN` values have stable
ordering. Public JSON encoding rejects non-finite `Float` values.

`Duration` is a millisecond-resolution runtime value produced by duration
literals and accepted by timeout policy. `Digest` is a module-owned typed hash
digest with `algorithm: Str`, `bytes: Bytes`, `hex() -> Str`, and
`base64() -> Str`. `Regex` is a module-owned compiled regular expression with
`pattern: Str`, `.matches(text: Str) -> Bool`,
`.find(text: Str) -> List[Record]`, `.captures(text: Str) -> List[Str]`, and
`.replace(text: Str, replacement: Str) -> Str`. `Command` is a module-owned
typed process plan produced by `process.command` or `process.command_argv`; it
serializes as argv arrays through owning modules and is not a general command
block literal. `ProcessHandle` is a runtime-owned child-process handle
produced by `spawn`. It is cloneable as a value, but aliases share one live
handle id and the first `wait` or `cancel` consumes the child.

`Error` is the common structured runtime error value used by `Result[T]`.
Declared error families are nominal subtypes of `Error`; each variant has a
fixed payload and may implement one or more nominal facets:

```xsh
error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied
```

Constructors are qualified by family, for example
`FsError.NotFound(file: target)`. Error values expose `.message`. Exact variant
payload fields are available after exact variant matching, and facets are
matched with `is Facet`. Family and variant labels may be rendered in
diagnostics, but source programs must not branch on string error kinds.

`ProcessError` is the structured process-execution error family returned by
process forms. It includes variants for not found, permission denied, nonzero
exit, signal termination, timeout, cancellation, capture-limit failures, and
invalid or already-consumed process handles.
Process failures can be matched by exact variant or shared facet:

```xsh
match process.run(command) {
  Err(ProcessError.Timeout { message }) => print ${message}
  Err(is PermissionDenied) => print "permission denied"
  Err(error) => return Err(error)
  Ok(status) => print ${status.ok}
}
```

Standard constructors:

- `p"literal"` and `fp"${value}"` produce `Path` values from UTF-8 source text.
- `Path(str) -> Path` remains a direct cast from text, but p-strings are the
  preferred spelling.
- `Path.parse_bytes(bytes) -> Result[Path]`.

Path values expose file-reading methods through the standard method surface:

- `.read_text() -> Result[Str]`.
- `.read_bytes() -> Result[Bytes]`.
- `.lines() -> Result[Stream[Str]]`, opening the file and yielding UTF-8 lines
  lazily.
- `.bytes_lines() -> Result[Stream[Bytes]]`, opening the file and yielding raw
  byte lines lazily with no UTF-8 decoding.

Prefer `p"literal"` for trusted UTF-8 source paths and `fp"${root}/child"` when
combining displayable values into a path. Accepted path promotion is limited to
source string literals at statically known path boundaries, such as typed module
arguments, proc arguments, typed bindings, and redirection targets. Runtime
`Str` values still require explicit checked conversion.

## 6. Expressions

Expression grammar:

```ebnf
expr_or_run  = expr | run_form result_op? ;

expr         = result_fallback ;
result_fallback = logic_or ("??" result_fallback)? ;
logic_or     = logic_and ("or" logic_and)* ;
logic_and    = equality ("and" equality)* ;
equality     = comparison (("==" | "!=") comparison)* ;
comparison   = term (("<" | "<=" | ">" | ">=") term)* ;
term         = factor (("+" | "-") factor)* ;
factor       = unary (("*" | "/" | "%") unary)* ;
unary        = ("!" | "-") unary | postfix ;
postfix      = primary postfix_op* ;
postfix_op   = "." IDENT | "." "require" "(" type_expr ")" | "?." IDENT
             | "[" expr "]" | call_args | "?" ;
call_args    = "(" arg_list? ")" ;
arg_list     = arg ("," arg)* ","? ;
arg          = expr | named_arg ;
named_arg    = IDENT ":" expr ;
primary      = literal | IDENT | list_lit | record_lit | map_comp | if_expr | match_expr
             | retry_expr | run_form | spawn_form | wait_form | "(" expr ")" ;
spawn_form   = "spawn" (run_form | expr) ;
wait_form    = "wait" expr ;
if_expr      = "if" expr "{" expr "}" ("else" "if" expr "{" expr "}")*
               "else" "{" expr "}" ;
match_expr   = "match" expr "{" match_expr_arm* "}" ;
match_expr_arm = pattern guard? "=>" expr ","? ;
retry_expr   = "retry" "[" (expr ("," expr)* ","?)? "]" block ;
```

Literals:

```ebnf
literal      = "null" | "true" | "false" | INT | FLOAT | DURATION
             | STRING | FMT_STRING | BYTES | PATH | PATH_FMT | GLOB ;
list_lit     = "[" list_body "]" ;
list_body    = (expr ("," expr)* ","?)?
             | expr "for" binding_target "in" expr ("if" expr)? ;
record_lit   = "{" (record_field ("," record_field)* ","?)? "}" ;
record_field = IDENT ":" expr | STRING ":" expr | IDENT ;
map_comp     = "{" field_path ":" expr "for" binding_target "in" expr
               ("if" expr)? "}" ;
field_path   = IDENT ("." IDENT)* ;
```

List comprehensions use `[expr for target in iterable]`, with an optional
`if condition` guard after the iterable. Map comprehensions use
`{item.key: value for item in iterable}` and follow the same iterable, binding,
and guard rules. The iterable must be a `List[T]`, `Stream[T]`,
`Result[List[T], E]`, or `Result[Stream[T], E]`; result iterables are unwrapped
like `?` before iteration. Comprehension guards must be `Bool` or `Status`.
Map comprehension keys must be `Str`. When two items produce the same key, the
later value replaces the earlier value.

Empty `{}` remains an empty record unless it appears in a context that expects
`Map[T]`; in a map-typed context, `{}` is sugar for an empty map.

Operators:

- `or`, `and`, and `!` operate on `Bool`. `!` also accepts `Status`, using the
  inverse of `status.ok`. `and` and `or` short-circuit: the right side of
  `false and expr` and `true or expr` is not evaluated.
- `??` operates on `Result`; it evaluates to the `Ok` value when the left side
  is `Ok`, otherwise it evaluates and returns the fallback expression.
- `==` and `!=` compare values of the same runtime type.
- `<`, `<=`, `>`, and `>=` operate on `Int` and `Str`.
- `+`, `-`, `*`, `/`, and `%` operate on `Int`.
- Path composition is written with formatted path literals, such as
  `fp"${root}/child"`. The `/` operator is numeric division only.
- `in` and `not in` test membership for `List`, substring containment for
  `Str`, byte containment for `Bytes`, display-text substring containment for
  `Path`, and exact entry membership for `env.PATH`.
- `.` accesses record fields and standard methods.
- A newline immediately before a `.` postfix operator continues the same
  expression, so long method chains may use one method per line.
- `.require(Type)` validates the receiver against a type expression and returns
  `Result[Type]`. The type argument is syntax, not a runtime identifier.
- `?.` is a null-safe field access: if the base is `null`, the expression
  evaluates to `null` without accessing the field; otherwise it accesses the
  field normally. The result type is `Optional[FieldType]`.
- `[]` indexes lists by integer and records by string key.
- Pure function calls use expression syntax.
- Postfix `?` propagates `Err` from a `Result`.
- `retry [delays...] { ... }` re-executes a fallible block and returns a
  `Result`.
- `spawn` and `wait` are process expressions. A trailing `?` applies to the
  `Result` produced by the whole `spawn` or `wait` expression, so
  `spawn run true ?` means "spawn the command, then propagate spawn failure."
- `range(n: Int) -> Stream[Int]` and `range(start: Int, n: Int) -> Stream[Int]`
  are builtin call expressions that produce integer sequences. They are usable
  directly in `for` loops and as pipeline sources.
- Tag union constructors are call expressions: zero-field variants use bare
  identifiers (`Info`); non-zero-field variants use call syntax (`Stopped("x")`).
  The type of a constructor expression is `Tag(TypeName)`.

There is no implicit string-number conversion.

There is also no implicit widening between integer and float values. Integer
literals remain `Int` or `UInt`; float literals remain `Float`. Arithmetic
between two `Float` values produces `Float`; arithmetic between two `Int`
values produces `Int`; mixed numeric arithmetic is rejected unless the integer
side is explicitly converted with `.float()`. Comparisons follow the same rule:
`Float` may be compared with `Float`, `Int` with `Int`, and mixed numeric
comparisons require explicit conversion. `%` is integer-only.

### Retry Blocks

Retry blocks are orchestration control flow for transient operations:

```xsh
let body = retry [1s, 2s, 4s] {
  fetch_remote_index()?
}?
```

The delay list is evaluated once, left to right, before the first attempt. Each
delay expression must produce `Duration`. The block is evaluated once, then
again after each delay while attempts fail. An empty delay list performs exactly
one attempt. A zero-duration delay is valid and retries immediately.

The retry expression returns `Result[T]`, or `Result[T, E]` when the attempt
body produces a more specific error type. On success, `Ok(value)` contains the
successful block value. If every attempt fails, the retry expression returns
`Err(final_error)` from the last failed attempt.

Inside the attempt block, postfix `?` is attempt-local: it turns the current
attempt into a failed attempt instead of propagating from the enclosing proc.
This attempt-local `?` does not itself require the enclosing proc's `error`
effect. A `return` statement inside a retry block keeps its ordinary meaning and
returns from the enclosing proc. `break` and `continue` keep their ordinary loop
targets.

Each attempt has an ordinary block scope. `defer` actions registered during an
attempt run before the next attempt begins and before a successful retry
returns.

Effects are the union of the delay expressions and the attempt body. A
non-empty delay list additionally requires the `time` effect because the runtime
sleeps between failed attempts.

Each attempt emits a structured `retry.attempt` trace event with the source
span, attempt number, maximum attempts, next delay when another attempt will be
made, and the failed error kind/message when the attempt failed.

## 7. Pure Functions And Procs

Definitions:

```ebnf
proc_def     = "proc" PROC_IDENT "(" param_list? ")" effect_list? "->" type_expr block ;
pure_def     = "pure" IDENT "(" param_list? ")" "->" type_expr block ;
stream_def   = "stream" IDENT "(" param_list? ")" effect_list? "->" "Stream" "[" type_expr "]" block ;
param_list   = param ("," param)* ","? ;
param        = IDENT (":" type_expr ("=" expr)? | "=" expr) ;
effect_list  = "[" (IDENT ("," IDENT)*)? "]" ;
return_stmt  = "return" expr_or_run? terminator ;
```

Parameters without an explicit type require a default expression whose type is
syntactically clear. Supported inferred defaults include `Bool`, `Int`,
`Duration`, `Str`, `Bytes`, and `Path` literals. Parameters
without defaults and rest parameters require an explicit type.

Pure functions are called with expression syntax:

```xsh
let obj = object_path(src)
```

Procs are called with expression syntax, including procs returning
`Result[Unit]`:

```xsh
compile(p"main.c", p"main.o")?
```

In statement position, unsuccessful `Result[Unit]` proc calls propagate by
default.

Expression-call arguments may splice a list into positional arguments with
`@expr`, for example `main(@args)?`.

Procs returning a value may be called in expressions, and the call remains
effectful:

```xsh
let objects = compile_objects(srcs)?
```

Procs are effectful and cannot be called from `pure` functions, even in
expression position. Command-style proc calls are rejected; command syntax is
reserved for core commands, `run` forms, and command-callable standard-module
APIs.
Unresolved command names are checker errors and never fall through to `PATH`.
First-class proc values expose `call(...) -> Result[Any]` for dynamic
export contracts; arguments are checked at runtime against the proc signature.
First-class pure values expose `call(...) -> Any`.

Pure functions are effect-free by contract. A pure function can call other pure
functions and standard module APIs whose signatures are marked pure. It cannot
execute external processes, call procs or core commands, read or write ambient
process state, or call effectful host APIs.

Pure functions may use local scratch mutation for deterministic computation.
`var` bindings declared inside a pure function body may be assigned within that
same pure function, including from nested blocks. Assignment from a pure function
to parameters, `let` bindings, top-level or imported bindings, captured outer
bindings, or any future reference-like value is rejected. Pure functions may
assign fields or string-keyed map entries below their own local `var` bindings;
this remains local scratch mutation and does not create shared mutable records
or maps. This local mutation does not change the effect contract: calls from pure
functions remain limited to pure functions, pure methods, pure standard module
APIs, and effect-free operators.

Stream producers are named lazy functions declared with `stream`. Calling a
producer returns `Stream[T]`; its body starts evaluating only when the stream is
consumed by a direct `for` loop or structured pipeline. Each `yield value`
emits one `T` item to the consumer. A producer signature must explicitly return
`Stream[T]`, and each yielded value must match `T`; yielding a stream value is
rejected so nested streams are introduced through explicit stages such as
`flat-map`.

Stream producers use proc-like effect annotations because a producer may open
files, run commands, or propagate `Result` failures while it is being consumed.
`return` without a value stops the producer. `return expr` is rejected inside a
producer, because items leave the producer through `yield`. `defer` cleanups run
when the producer exhausts, fails, or the downstream consumer stops early.

### Effect Annotations

A proc may carry an optional `[effect, ...]` annotation between its parameter
list and its return type. This annotation declares which categories of side
effects the body is allowed to produce.

```xsh
proc read_config() [fs, error] -> Result[Config] { ... }
proc build() [fs, process, error] -> Result[Status] { ... }
proc get_time() [time] -> Int { ... }
```

A proc with no `[]` clause remains **unrestricted** — identical to existing
`proc` behavior. Annotations are opt-in; existing code requires no changes.

**Effect set.**

| Effect | Covers |
|--------|--------|
| `fs` | `fs.*`, `archive.*`, `diff.*`, `patch.*`, `user.*`, `group.*`, `module.*` |
| `io` | `io.*`, plus superset of `{fs, net, process, env}` |
| `net` | `net.*`, `dns.*` |
| `process` | `run` forms, `spawn`, `wait`, `ProcessHandle.cancel`, `process.*` (effectful overloads), `unix.*`, `linux.*`, `applet.*` |
| `env` | `env.*`, `cd`, `system.*` |
| `time` | `time.*`, delayed `retry` blocks |
| `error` | can propagate `Err` via `?` outside retry attempt blocks |
Use `io` for direct stdin/stdout operations. It also covers `{fs, net, process,
env}` for scripts that intentionally treat host I/O as one boundary; prefer
specific effects when a proc does not need stdin/stdout.

**Enforcement rules.**
- A restricted proc (`Some([E])`) may only call procs whose declared effects are
  a subset of `E`, plus `pure` functions.
- Calling an unrestricted proc from a restricted proc is a checker error.
- Direct calls to standard-module functions (e.g. `fs.read_text`) and standard
  methods (e.g. `path.read_text()`) are checked against `E`; the `io` effect
  covers `fs`, `net`, `process`, and `env` but not `time` or `error`.
- `run` forms, `spawn`, `wait`, and `ProcessHandle.cancel` require the
  `process` effect.
- The `?` propagation operator requires the `error` effect, except inside retry
  attempt blocks where it fails the current attempt instead of propagating from
  the enclosing proc.
- Unrestricted procs (no annotation) may call anything — no restriction.
- Diagnostic code: `check.effect-violation`.

**Inference.** `xsht lint` emits `lint.unannotated-effects` for procs that
have no annotation but whose bodies contain inferable effects, and
`lint.missing-effects` for restricted procs whose annotation omits inferable
body effects. `--fix` inserts or replaces the annotation automatically. The
linter infers from direct module calls, typed standard methods, restricted proc
calls, `run` forms, `spawn`, `wait`, delayed `retry`, and `?` outside retry
attempt blocks.

Function bodies use a narrow tail-value rule. If the final statement in a
`proc` or `pure` body is an expression statement, that expression produces the
function result. If the final statement is a command statement, the command's
statement result produces the function result. A final expression-style proc
call that returns `Result[Unit]` propagates failure and produces `Unit`. A
final plain `run ...` in statement position asserts success and produces
`Unit`.
`lint.redundant-tail-return-binding` flags a final `let` or `var` binding that
is immediately returned, and autofixes it to the initializer as the final tail
expression when doing so would not remove intervening comments. Annotated
bindings are autofixed only when checked types show the initializer already has
the annotated type, so the edit does not remove a binding-level conversion.
This also covers typed empty-list temporaries such as
`let empty: List[T] = []; return empty` when the function tail type provides
the needed context. `lint.redundant-ok-tail` flags final `return Ok(value)` in
`Result[T]` functions and autofixes to the plain tail value when checked types
show the value already has type `T`.

Non-tail expression statements inside value-producing function bodies must have
type `Unit` or `Result[Unit]`; `Result[Unit]` statements propagate failure by
default. Otherwise bind the value, return it explicitly, or make it the final
statement. `if`, `match`, `while`, and `for` statements do not produce function
tail values in v1. Use explicit `return` in branches where branch control flow
determines the result.

`return` without a value returns `Unit`. `Result[Unit]` procs, pure functions,
builder tasks, and effect blocks may fall off the end or tail-produce `Unit`;
the runtime converts that to `Ok()`. A function returning `Result[T]` may
tail-produce or return either a `Result[T]` value or a plain non-`Result` `T`
value; plain `T` is wrapped as `Ok(value)`. `Ok(value)` and `Err(error)` remain
valid when the result shape should be visible at the call site.

Signatures support default parameters, rest parameters, type aliases, richer
module signatures, overloads, and known record shapes. Overloads are selected
from argument names and argument types; return type alone does not
disambiguate.

## 8. Control Flow And Results

Implemented control flow:

```ebnf
if_stmt      = "if" expr block ("else" "if" expr block)* ("else" block)? ;
while_stmt   = "while" expr block ;
for_stmt     = "for" binding_target "in" expr block ;
break_stmt   = "break" terminator ;
continue_stmt = "continue" terminator ;
with_stmt    = "with" with_binding ("," with_binding)* ","? block
               "else" ("|" IDENT "|")? block ;
with_binding = IDENT "=" expr ;
guard_stmt   = "guard" "let" binding_target (":" type_expr)? "=" expr_or_run
               "else" ("|" IDENT "|")? block ;
loop_stmt    = "loop" block ;
break_stmt   = "break" expr? terminator
             | "break" ("when" | "unless") expr terminator ;
continue_stmt = "continue" terminator
              | "continue" ("when" | "unless") expr terminator ;
return_stmt  = "return" expr? terminator
             | "return" ("when" | "unless") expr terminator ;
match_stmt   = "match" expr "{" match_arm* "}" ;
```

Conditions evaluate to `Bool` or `Status`. A `Status` condition is true when
`status.ok` is true. `while` repeats until its condition is false.

Type patterns test a dynamic matched value and narrow the binding inside the
arm:

```xsh
match json.decode(input)? {
  i is Int => print ${i.float()}
  f is Float => print ${f}
  _ is Null => print "null"
}
```

The checker accepts type patterns only for dynamic matched values such as `Any`
or empty `Record`. Use `.require(Type)?` when the program expects a known
schema; use type patterns when the program intentionally handles unknown JSON or
other dynamic shapes.

`for` iterates over `List[T]`, `Stream[T]`, `Result[List[T]]`, or
`Result[Stream[T]]`. `Result` wrappers are auto-unwrapped; an `Err` propagates
as a runtime error. The loop target is bound immutably for each iteration
unless copied into a `var`. Structured pipeline expressions are valid as the
iterator; they evaluate to `List[T]` and iterate without materialising an
intermediate variable. The binding target may be a record destructuring pattern:

```xsh
for {path, size} in files {
  print f"${path} ${size}"
}
```

`match` tries arms in order; if no arm matches, evaluation reports a
`match-no-arm` runtime error. When the matched value is a tag union type,
`check.non-exhaustive-match` warns when variants are not fully covered and no
wildcard or binding arm is present.

`with` binds multiple sequential fallible values with a shared `else` handler.
Each binding evaluates its expression; if the result is `Ok(value)` or a
non-error direct value, that value is bound in the `with` body. If any binding
produces `Err(error)` or a propagated `?` error, the `else` block executes
instead. The optional `|param|` receives the error value.

```xsh
with
  config = read_config()?,
  db = connect(config)?,
  result = query(db, sql)?
{
  process(result)
} else |e| {
  print f"setup failed: ${e.message}"
}
```

### Block parameter conventions

XSH has two syntactic positions for block parameters. They serve different
purposes and must not be confused:

**Inside the block (`{ |x| ... }`)** — used by stream stages (`map`, `where`,
`sort-by`, `tee`, `group-by`, `any`, `all`, `count`, etc.) and by lambda-style
expressions passed as arguments. The `|param|` appears immediately after the
opening `{`:

```xsh
let doubled = numbers |> map { |n| n * 2 }
let long    = words   |> where { |w| w.len() > 5 }
```

The implicit item shorthand `.` is also available in stream blocks as an alias
for the first parameter. `map { . * 2 }` is equivalent to `map { |n| n * 2 }`.

**Before the block (`|param| { ... }`)** — used by the `else` clause of `with`
and `guard let`. The `|param|` appears between the `else` keyword and the
opening `{`:

```xsh
with result = fallible_op() {
  use_result(result)
} else |e| {
  print f"failed: ${e.message}"
}

guard let n = parse(input) else |e| {
  return Err(e)
}
```

The before-block form exists because the bound name needs to be visible in
the else clause but is not part of the block's general scope. The inside-block
form exists because stream stages use `parse_block()` which reads params as the
first thing inside the `{`. **Never mix the two**: `else { |e| ... }` will
silently treat `|e|` as a pipeline expression inside the block, not as a
parameter binding.

`Result` values are:

```text
Ok(value)
Err(error)
```

`Ok()` is equivalent to `Ok(Unit)`.

Postfix `?` can be applied only to a `Result`. If the value is `Ok(value)`,
`?` evaluates to `value`. If the value is `Err(error)`, `?` returns
`Err(error)` from the nearest enclosing `Result`-returning proc, pure function,
builder task, or effect block.

In ordinary expression context, `?` may be written immediately after the
expression (`expr?`) or separated before an expression separator or statement
terminator (`expr ?`). In command-argument context, a separated `?` after an
argument belongs to the command statement or run form, not to the preceding
typed argument. Use `expr?` or `(expr?)` when the command argument itself must
contain a propagated expression.

At top level, `?` on `Err(error)` terminates script evaluation with a runtime
failure diagnostic and the runtime-failure CLI exit code. A final top-level
`Int` or `UInt` value exits with that status code. Status values must be in the
range `0..=255`.

`abort(status: Int, force: Bool = false)` exits the script with `status`.
Deferred cleanup runs while unwinding by default. `abort(status, force: true)`
skips deferred cleanup.

`left ?? fallback` operates on `Result` and `Optional` values. For `Result`:
`Ok(value)` evaluates to `value`; `Err(error)` evaluates the fallback. For
`Optional`: a non-null value evaluates to itself; `null` evaluates the fallback.
`??` is right-associative. `or` remains Bool-only; use `??` for Result and
Optional fallback.

Ignoring a value-producing `Result` is a checker error. A `Result[Unit]`
statement propagates failure by default. Assign to `_` only when an ignored
value-producing result is intentional:

```xsh
let _ = fs.remove(path)?
```

`Result.context(kind: Str, message: Str = "", ...)` returns the original
`Ok` unchanged. For `Err`, it appends diagnostic context to the error value so
runtime diagnostics and traces can name the failing package, rule, stage, path,
or operation. Context values must be displayable.

## 9. Commands

Command forms:

```ebnf
command_stmt = command result_op? terminator ;
result_op    = "?" ;

command      = module_command | core_command | run_form ;
module_command = STANDARD_MODULE "." IDENT command_arg* ;

core_command = ("print" | "eprint") command_arg*
             | "cd" command_arg block
             | "env" env_assignment* block
             | "env" env_expr_assignments block
             ;
env_assignment = IDENT "=" command_arg ;
env_expr_assignments = "{" env_expr_assignment* "}" ;
env_expr_assignment = IDENT "=" expr terminator ;
```

Core command names are reserved in command position and cannot be shadowed by
user procs.

Command arguments:

```ebnf
command_arg  = word | splice | typed_arg ;
word         = word_part+ ;
word_part    = bare_word | STRING | interpolation | dollar_shorthand ;
bare_word    = bare_char+ ;
interpolation = "${" expr "}" ;
dollar_shorthand = "$" IDENT ("." IDENT)* ;
splice       = "@" (IDENT | "(" expr ")" | glob_literal) ;
typed_arg    = "(" expr ")" | FMT_STRING | PATH_STRING | PATH_FMT_STRING
             | command_expr_chain ;
command_expr_chain = contiguous expression chain containing a call or index ;
```

Adjacent word parts with no intervening whitespace form one argument. Words do
not perform globbing, tilde expansion, variable expansion, brace expansion, or
word splitting.

For readability, command arguments may use an unwrapped typed expression chain
when the chain is unambiguous in command-argument position, such as
`basename_value(name, suffix)`, `input.display()`, or `rows[0]`. Plain field
access like `record.field` remains a word unless written as `$record.field`,
`${record.field}`, or `(record.field)`. Whitespace ends the command argument;
a separated following `?`, `(`, `[`, or `.` is not consumed as part of the
typed expression chain.

Use `$name` or `$record.field` for simple command interpolation. Use `${expr}`
for arbitrary expressions. `f"..."` and `fp"..."` literals are accepted
directly as typed command arguments without `${}` or `()` wrapping;
`lint.redundant-fmt-wrapper` flags the wrapped forms and autofixes them.
Interpolation is evaluated as one command argument when it is the whole word;
when a standalone interpolation evaluates to `List[T]` and `T` can be an argv
item, the list is spliced into argv. Interpolation inside a compound word uses
display conversion. `Path` values display as their path text without a
`.display()` call.

An `expr_stmt` beginning with an identifier followed by `|>` (across
whitespace or newlines) is parsed as a structured pipeline expression
statement, not a command invocation. This allows `pipeline |> table.print(...)`
as a bare statement without `let _ = ...`.

```xsh
let target = p"target/debug/tool"
let cache_dir = p".cache"
run $target "--cache=${cache_dir}" ?
```

Command arguments are not general expressions. The expression escape hatch is
`(expr)`.

Fully qualified standard-module commands are accepted only for effectful
standard-module APIs returning `Result[Unit]`, such as:

```xsh
fs.mkdir build
fs.remove dist --missing-ok
json.write out (metadata)
```

These command statements propagate unsuccessful `Result[Unit]` values by
default.

Value-producing module APIs remain expression-only. Module command boolean
flags are available only for defaulted `Bool` parameters; kebab-case flag names
map to snake_case parameter names, so `--missing-ok` means
`missing_ok: true`. Non-`Bool` named arguments use expression-call syntax.

For scoped environment overlays, `env NAME=value { ... }` uses command-word
conversion. `env { NAME = expr } { ... }` evaluates expressions and converts
each result to one environment value with the same argv-item conversion used by
external commands.

Command-style proc calls are not accepted. Use expression-call syntax so
argument boundaries and types remain explicit.

`print` writes its arguments separated by one space and appends a newline to
stdout. `eprint` does the same on stderr. `print --flush` and `eprint --flush`
write to the process's inherited stdout or stderr immediately instead of the
captured script-output buffer; `--flush` is recognized only as the first
argument. Both return `Unit`. They accept human-facing scalar output: `Str`,
`Int`, `Bool`, and `Path`. `Path` interpolation and printing use display
conversion and must not canonicalize, resolve, or otherwise change the path.

Script stdout and stderr are byte streams. Text-producing APIs append UTF-8
bytes; `io.write_stdout_bytes` appends bytes exactly and does not check
UTF-8.

`cd path { ... }` changes the evaluator cwd context while the block runs. It
returns `Result[Unit, Error]` and must restore the previous cwd after success,
after `Err`, and after runtime failure.

## 10. Process Execution

External programs always require `run`. A bare `make -j4` is not a process
execution form and does not search `PATH`.

Run forms:

```ebnf
run_form     = "run" run_target command_arg*
             | "run.status" run_target command_arg*
             | "run.text" run_target command_arg*
             | "run.bytes" run_target command_arg*
             | "run.capture" capture_mode run_target command_arg*
             | "run.stream" capture_mode run_target command_arg*
             | "run.builtin" run_target command_arg*
             | "run.builtin.status" run_target command_arg*
             | "run.builtin.text" run_target command_arg*
             | "run.builtin.bytes" run_target command_arg*
             | "run.builtin.capture" capture_mode run_target command_arg*
             | "run.builtin.stream" capture_mode run_target command_arg*
             | run_head "(" command_arg+ redirection* ")"
             ;
run_head     = "run" | "run.status" | "run.text" | "run.bytes"
             | "run.capture" capture_mode | "run.stream" capture_mode
             | "run.builtin" | "run.builtin.status" | "run.builtin.text"
             | "run.builtin.bytes" | "run.builtin.capture" capture_mode
             | "run.builtin.stream" capture_mode ;
capture_mode = "--text" | "--bytes" ;
run_target   = word | typed_arg ;
```

All run forms accept `--timeout=<Duration>` and `--cpumax=<Int>` immediately
after the run form and before environment overlays:

```xsh
run --timeout=30s --cpumax=80 make check
```

`--cpumax=N` requests a CPU quota of `N` percent of one CPU for process-backed
execution. `80` means 80% of one core; values above `100` are valid. On Linux,
XSH enforces this with cgroups v2 `cpu.max` using a 100000 microsecond period.
If `XSH_CGROUP_ROOT` is set, scopes are created under that root; otherwise XSH
uses the current delegated cgroup subtree. Linux reports a `ProcessError` when
cgroups v2 enforcement is requested but unavailable or not writable. macOS
accepts and ignores `--cpumax`; other non-Linux platforms report unsupported
platform when a CPU quota is requested.

All run forms also accept a grouped invocation body after run options and
environment overlays. To keep `run (expr)` available for typed command
arguments, a grouped body starts with `(` followed by a newline or comment.
Each argument inside the parentheses is an ordinary command argument, newlines
are allowed between arguments, and trailing `?` applies to the whole run form:

```xsh
run (
  make
  "ARCH=arm64"
  "-j2"
  "Image"
) ?
```

Target resolution:

- A bare target with no slash is resolved through `PATH` by `run` only.
- A target containing `/` is treated as an explicit relative or absolute path.
- A `Path` target uses native path bytes.
- A `Str` target is encoded as UTF-8 and must not contain NUL.
- Not found, permission denied, not executable, `ENOEXEC`, NUL in target,
  spawn failure, and I/O failure are distinct `ProcessError` variants or
  facets.
- `run.builtin*` forms are legacy spellings that execute the target like the
  corresponding `run*` form. They do not use a compatibility-builtin registry or
  shim.

Process results:

- Plain `run` and byte pipelines in statement position assert success by
  default, update `$?`, and propagate `ProcessError` for nonzero exits, signal
  termination, setup failures, and failed pipeline segments.
- Plain `run` in value position evaluates to `Status` and updates `$?`.
- `run.status` is the explicit status-preserving form. It evaluates to
  `Status`, updates `$?`, and does not propagate unsuccessful completion
  unless followed by trailing `?`.
- `run.text` returns `Result[Str, ProcessError]`.
- `run.bytes` returns `Result[Bytes, ProcessError]`.
- `run.capture --text` returns
  `Result[{status: Status, stdout: Str, stderr: Str}, ProcessError]`.
- `run.capture --bytes` returns
  `Result[{status: Status, stdout: Bytes, stderr: Bytes}, ProcessError]`.
- `run.stream --text` returns `Stream[Str]` after explicit UTF-8 decoding.
- `run.stream --bytes` returns `Stream[Bytes]`.

Capture behavior:

- `run.text`, `run.bytes`, `run.stream --text`, and `run.stream --bytes`
  capture stdout and inherit stderr and stdin.
- `run.capture --text` and `run.capture --bytes` capture stdout and stderr and
  inherit stdin. Nonzero child exit returns `Ok(record)` with `status`; setup,
  timeout, cancellation, capture-limit, and UTF-8 decode failures return `Err`.
- Captured stdout and stderr are exact; no trailing newline is removed.
- `--text` requires valid UTF-8.
- `--bytes` performs no decoding.
- The default capture limit is 16 MiB per captured stream.
- If the limit is exceeded, the child is terminated and a `ProcessError`
  capture-limit variant is returned.
- The implementation must read captured streams without pipe deadlock.

Spawn and wait forms:

```xsh
let handle = spawn run make test ?
let status = wait handle?
```

- `spawn run ...` starts exactly one external child immediately and returns
  `Result[ProcessHandle, ProcessError]`.
- `spawn command_expr` evaluates `command_expr` to `Command`, starts the typed
  command plan, and returns the same handle result.
- `wait handle` waits for a live handle and returns
  `Result[Status, ProcessError]`.
- `wait [h1, h2, ...]` waits all distinct live handles in input order and
  returns `Result[List[Status], ProcessError]`.
- `handle.cancel(signal: Str = "TERM", kill_after: Duration = 2s)` sends the
  signal to the child process group, escalates to SIGKILL after `kill_after`
  when needed, waits for reaping, consumes the live handle, and returns
  `Result[Unit, ProcessError]`.

`spawn run` accepts the normal single-command argv, interpolation, typed
arguments, argv splices, environment overlays, `--timeout`, `--cpumax`, cwd,
and redirection behavior used by `run`, and inherits stdio by default. V1
rejects byte pipelines, `run.text`, `run.bytes`, `run.capture`, `run.stream`,
and any form that cannot map to exactly one child process. There is still no
shell-string process execution form.

`spawn command_expr` uses the command plan's target, argv, cwd, env overlay,
timeout, `cpu_max`, `detach`, `new_session`, and `ignore_hup` fields. This is
distinct from `process.spawn(command)`, which remains a lower-level detached
helper returning a record and waiting in the background.

`ProcessHandle` exposes read-only metadata fields `pid: Int`, `command: Str`,
`argv: List[Str]`, and `detached: Bool`. Field reads do not perform host I/O
and remain valid after the live child has been waited or canceled. The runtime
child is single-consumption: aliases share one handle id, and later aliases
return `Err(ProcessError.Unknown)` after the first successful `wait` or
`cancel`.

`spawn` does not update `$?`. A successful `wait handle` updates `$?` to the
returned status. A successful `wait [handles]` updates `$?` to the last status
in the returned list when the list is non-empty. Wait errors do not update `$?`
through the status path. Ordinary nonzero exits and signal terminations are
status data; setup failures, timeout, cancellation, wait I/O failure, and
invalid handles are `ProcessError`.

Timeouts are measured from spawn time, not from wait time. If a timeout has
already expired when `wait` starts, the child is terminated promptly and `wait`
returns a timeout process error.

List wait evaluates the list first, validates items as process handles, waits
each distinct live valid handle in input order, and continues draining those
handles after an earlier duplicate, invalid, non-handle, timeout, or wait
error. If any error occurred, the first `ProcessError` is returned after the
drain and no partial status list is exposed. A duplicate handle is waited once,
then the duplicate occurrence is treated as an invalid-handle error.

Live handles are owned by lexical scope ids, not by Rust value identity. Values
containing handles transfer ownership outward when returned, tail-produced,
broken from loop expressions, or assigned into an outer binding. At scope exit,
owned non-detached handles are canceled and reaped before `defer` cleanup runs;
owned detached handles are released to a background waiter instead of being
killed. If SIGINT or SIGTERM reaches the evaluator while ordinary XSH code is
running, checkpoint cancellation cleans up live non-detached handles outside
the signal handler and propagates a canceled process error. This feature is
process fan-out, not an async runtime: there are no futures, callbacks,
channels, scheduler, `await`, or wait-any primitive in v1.

In statement position, plain `run` records status in `$?` and propagates
`ProcessError` for unsuccessful completed statuses or setup failures. In value
position, plain `run` evaluates to `Status`. Use `run.status` when status data
should be inspected without default propagation. A trailing `?` remains
available as an explicit success assertion for status-preserving process
forms. Process error kinds include exec failures, `nonzero-exit`, `signal`,
`pipeline-failure`, `timeout`, and `canceled`.

For byte pipelines, `--cpumax` is valid only on the first segment. When present,
one shared cgroup scope is created for the whole pipeline and every child
process in the pipeline is assigned to that scope.
Diagnostics for unsuccessful external commands include the cwd and a
shell-escaped argv rendering by default. Environment overlays are not included
in the diagnostic text.

Accepted byte pipeline and redirection syntax:

```xsh
run tar cf - src | run gzip -9 > ${tarball}
run make > ${log} 2> ${errlog}
run sort < ${input} > ${output}
run tool >& 2
```

Redirection targets are typed path-like values or non-negative file descriptor
numbers for fd duplication. `2>` and `2>>` redirect stderr for write and append.
Process traces must represent argv, env overlays, cwd, pipeline segments,
spawn/wait handle ids, and redirections structurally, never as reconstructed
shell strings.

Accepted environment syntax:

```xsh
run CC=cc CFLAGS="-O2 -pipe" ./configure --prefix=/usr

env CC=cc CFLAGS="-O2 -pipe" {
  run make -j${cpu.count()}
}
```

`env.Str.NAME` returns `Result[Str]`, `env.Path.NAME` returns `Result[Path]`,
and `env.PathList.NAME` returns `Result[List[Path]]`. `env.PATH` is a scoped
mutable path-list view backed by the current runtime environment overlay.
String lookup errors when the value is missing or is not valid UTF-8.
Non-UTF-8 environment values can still be inherited by child processes on Unix;
string lookup does not decode them silently.

Cancellation is process-group based on Unix. A `run` command has its own
process group, and a pipeline has one cancellation root shared by every
segment. When XSH receives SIGINT or SIGTERM while process work is running, it
forwards that same signal to the child process group, waits for a
runtime-defined grace period, then sends SIGKILL to remaining children. The
result is a canceled `ProcessError` variant. XSH cannot clean up descendants
that intentionally move into another process group.

Signal hooks provide a bounded shutdown path for entry scripts:

```xsh
on SIGINT --pre-cancel=150ms [fs, process, error] {
  p"/tmp/build.interrupted".write("interrupted\n")?
  abort(130)
}
```

The grammar shape is:

```text
signal_hook_stmt = "on" signal_name hook_option* effect_list block
hook_option      = "--pre-cancel=" duration_literal
effect_list      = "[" (effect ("," effect)*)? "]"
```

`on` is contextual; ordinary bindings such as `let on = 1` remain valid. A
hook is recognized only in hook statement shape. Effects are required; use `[]`
when the hook has no effects. `--pre-cancel` is optional and defaults to
`150ms`.

Hook signal names are identifiers written with or without one leading `SIG`
prefix. Names are normalized by ASCII uppercasing and stripping that prefix.
Accepted v1 names are `HUP`, `INT`, `QUIT`, `TERM`, `USR1`, `USR2`, `ALRM`,
`XCPU`, and `XFSZ`, subject to platform availability. Numeric declarations,
unknown signals, `KILL`, `STOP`, job-control or event-like signals (`CHLD`,
`CONT`, `TSTP`, `TTIN`, `TTOU`), and `PIPE` are rejected.

Hooks are entry-script-only in v1. Imported or dynamically loaded modules that
contain hooks fail checking. Hooks are not exported, are not module API, and
may appear only at the entry script top level. Duplicate hooks for the same
normalized signal are checker errors, so `TERM` and `SIGTERM` conflict.

A hook is registered when its top-level statement is evaluated. It can refer to
root procs and pures, plus top-level values already evaluated before the hook
declaration. It cannot refer to later top-level values. Hook-local bindings use
ordinary lexical scope. Hook bodies must produce `Unit`, `Status`, or
`Result[Unit]`; `?` inside a hook requires the `error` effect.

OS signal handlers only record signal state. XSH code never runs inside the OS
handler. The main evaluator services pending signals at checkpoints: between
statements, loop boundaries, defer boundaries, process waits, pipeline waits,
parallel stream scheduling/collection, and chunked `time.sleep` waits.

The first handled signal chooses the shutdown path and may run one matching
hook. Repeated handled signals during that path request escalation: no hook
re-entry, prompt SIGKILL for owned process groups, and remaining cleanup may be
skipped after the current safe point.

When active child process groups exist, the hook's `--pre-cancel` budget is the
time it may delay forwarding the primary signal. If the hook completes before
forwarding, XSH forwards after hook-local defers. If the hook reaches a
checkpointed blocking wait or the budget expires, XSH forwards the primary
signal to non-hook-owned active process groups and lets the hook continue until
completion or escalation. Process work started by the hook ignores the primary
signal but is killed on escalation.

If a hook calls `abort(status)`, that status is committed while owned child
process groups are still canceled. `abort(status, force: true)` also skips
defers. Without an abort, `INT` and `TERM` hooks default to XSH's runtime
cancellation status `3`; non-`INT`/`TERM` hooks default to `128 + signal`.
Hook failure is recorded in diagnostics/traceback and produces runtime failure
status `3`.

## 11. Argv Conversion

Every external argv item is a byte sequence that cannot contain NUL.

Allowed argv conversions:

- `Str` to UTF-8 bytes.
- `Path` to native Unix path bytes.
- `Int` to decimal ASCII.
- `Bool` to `true` or `false`.

Rejected argv conversions:

- `Null`.
- `Bytes`, unless an explicit encoding API is used first.
- `List` without `@`.
- `Record`.
- `Result`.
- `Status`.
- `Error`.
- `ProcessError`.
- `Pure`.
- `Proc`.
- `Unit`.

Each word produces one argv item unless it is a standalone interpolation or
dollar shorthand whose expression evaluates to `List[T]`; in that case each
list item becomes one argv item. Interpolation inside a compound word, such as
`-j${jobs}` or `"prefix=$value"`, contributes to that same argv item and cannot
splice lists. Explicit `@name` and `@(expr)` splices remain available when the
source should visibly mark list expansion.

## 12. Status

`Status` records completed process state. It must distinguish ordinary exit
from signal termination and expose total inspection APIs. `Status` is
runtime-only and cannot be constructed with a record literal; obtain values from
`run`, `run.status`, `process.run`, or other process APIs.

Required fields:

- `success: Bool`.
- `kind: Str`, either `"exit"` or `"signal"`.

Required methods:

- `status.exited() -> Bool`.
- `status.signaled() -> Bool`.
- `status.exited_with(code: Int) -> Bool`.
- `status.exit_code() -> Result[Int]`.
- `status.signal_number() -> Result[Int]`.

`status.ok` is the short success predicate.

## 13. Standard Modules

`applet`:

The `applet` module is the internal host surface used by shipped core applet
scripts. It is not a general stable user API. Auth applet policy, option
parsing, prompts, passwd/shadow parsing, shadow updates, lock/unlock/delete
decisions, nologin messages, and getty handoff rules live in XSH scripts under
`core/` and `core/lib/`. The host functions below own only password hashing and
verification, the current effective uid and executable path needed by applet
scripts, native `mdev`, and privileged session mechanics such as groups,
uid/gid changes, cwd/env setup, and process status.

- `applet.hash_password(password: Str, algorithm: Str) -> Result[Str]`.
- `applet.verify_password(password: Str, hash: Str) -> Bool`.
- `applet.current_euid() -> Int`.
- `applet.current_exe() -> Result[Path]`.
- `applet.login_session(user: Record, preserve_env: Bool, host: Str) -> Result[Int]`.
- `applet.su_session(user: Record, login: Bool, preserve_env: Bool, shell: Str, command: Str, extra_args: List[Str]) -> Result[Int]`.
- `applet.sulogin_session(user: Record) -> Result[Int]`.
- `applet.mdev(argv: List[Str]) -> Result[Int]`.

`archive`:

- `archive.tar_list(path: Path, compression: Str = "auto",
  members: List[Path] = []) -> Result[Stream[Record]]`. The stream opens the
  archive when created and yields selected entries in archive order as they
  are consumed. Use `.collect()` when a materialized list is needed.
- `archive.tar_extract(path: Path, dest: Path, strip_components: Int = 0,
  compression: Str = "auto", overwrite: Bool = false,
  members: List[Path] = []) -> Result[Unit]`.
- `archive.tar_create(path: Path, root: Path, entries: List[Path],
  compression: Str = "auto", overwrite: Bool = false) -> Result[Unit]`.
- `archive.cpio_list(path: Path) -> Result[Stream[Record]]`; consume it directly or call `.collect()` for a list.
- `archive.cpio_extract(path: Path, dest: Path, overwrite: Bool = false) -> Result[Unit]`.
- `archive.cpio_create(path: Path, root: Path, entries: List[Path],
  overwrite: Bool = false) -> Result[Unit]`.
- `archive.zip_list(path: Path) -> Result[Stream[Record]]`; consume it directly or call `.collect()` for a list.
- `archive.zip_extract(path: Path, dest: Path, overwrite: Bool = false) -> Result[Unit]`.
- `archive.compress(source: Path, dest: Path, format: Str = "auto",
  level: Int = 6, overwrite: Bool = false) -> Result[Unit]`.
- `archive.decompress(source: Path, dest: Path, format: Str = "auto",
  overwrite: Bool = false) -> Result[Unit]`.
- `archive.decompress_bytes(source: Path, format: Str = "auto") -> Result[Bytes]`.

Archive compression modes are `"auto"`, `"gz"`, `"gzip"`, `"bz2"`, `"bzip2"`,
`"xz"`, and `"lzma"`.
`"auto"` detects gzip, bzip2, and xz by input magic when reading, falls back to
file extensions including `.lzma`, and chooses gzip, bzip2, xz, lzma, or plain
tar from the output filename when creating. Tar and cpio creation accept only
relative entry paths under `root`; `p"."`
archives root contents without adding a leading root directory entry. Cpio uses
the portable `newc` format. Zip support lists and extracts existing archives,
including Stored, Deflate, bzip2, LZMA, zstd, xz, and deflate64 entries where
the ZIP reader supports them.

Extraction rejects absolute paths, parent traversal, existing destination files
unless `overwrite` is true, symlink destinations, symlink ancestors, and symlink
or hardlink targets that escape the destination tree.

Archive entry records have `path: Path`, `kind: Str`, `size: Int`, `mode: Int`,
`modified: Int`, and `link_name: Str`.

`cli`:

- `cli.parse(argv: List[Str], schema: Record, command: Str = current script) -> Result[Record]`.
- `cli.parse_full(argv: List[Str], schema: Record, env: Record = {},
  command: Str = current script) ->
  Result[Record]`.
- `cli.usage(schema: Record, command: Str = "command") -> Str`.
- `cli.commands(argv: List[Str], commands: Record) -> Result[Record]`.
- `cli.commands(argv: List[Str], rootless_default: Str, commands: Record,
  fallback_command: Record = {}) -> Result[Record]`.
- `cli.tokens(argv: List[Str], value_flags: List[Str] = []) ->
  Result[List[Record]]`.

`cli.parse` is the typed replacement for `getopt`: it accepts long options such
as `--root value` and `--jobs=4`, short options such as `-v`, and short clusters
such as `-vj4`. It maps dashes in long option names to underscores in record
fields. When the schema argument is a literal record, the checker infers a
result record shape: required, positional, and defaulted scalar fields are
concrete, non-required scalar fields are optional, repeated fields are
`List[T]`, and flags are `Bool`. Schema descriptors may be a type string such as
`"Str"` or a record with `kind`, `form`, `default`, `required`, `repeated`,
`flag`, `long`, `short`, `choices`, `conflicts`, `requires`,
`required_group`, `env`, `hidden`, `deprecated`, `help`, `optional_value`,
`optional_default`, `min`, `max`, `positive`, `nonzero`, `exists`, `file`, and
`dir` fields. The `form` field is a compact usage spelling such as
`"-j --jobs N"`, `"--root DIR"`, `"--color[=WHEN]"`, or `"...FILE"`; option
spellings become long and short aliases, non-option forms before any option
mark positionals, `[=...]` marks an optional option value, and `...` marks
repeated positionals. If `kind` is absent, the value type is inferred from
`default`; absent defaults use `Str`, and repeated fields without a default use
`List[Str]`. Supported option value types are `Str`, `Int`, `UInt`, `Bool`,
`Path`, `Duration`, and `List[...]` through repeated options. `UInt` parses
non-negative decimal integers and returns an `Int` value. Runtime results
include `null` for absent optional scalar fields. Errors include the failing
argv index in the diagnostic message when the failure comes from an input item.

`cli.parse` and `cli.parse_full` reserve `-h` and `--help` implicitly. Schemas
do not declare a help option or include a help field in their result record.
The optional `command` label controls the rendered usage prefix; scripts
normally omit it, while subcommands can pass labels such as `"pm world-plan"`.
When help is requested before `--`, parsing returns a `cli-help` error whose
message is the rendered usage. If that result is propagated with `?` at script
top level, XSH prints the usage to stdout and exits successfully. Other parse
failures keep the `args-parse` error kind, include the rendered usage after the
specific parse message, and exit with usage status `2` without a traceback when
propagated at script top level.

Option values may be constrained with `choices`, integer bounds (`min`, `max`),
integer/duration checks (`positive`, `nonzero`), and path checks
(`exists`, `file`, `dir`). Use `kind: "Path"` with `file: true` or `dir: true`
for existing-file or existing-directory checks. `conflicts` and `requires` name
other option fields or option spellings; `required_group` requires at least one
member of the named group. Hidden options parse normally but are omitted by
`cli.usage`; deprecated options parse and emit warnings through
`cli.parse_full`. `env` names an explicit environment record key used by
`cli.parse_full`, with precedence `argv > env > default > absent`; `cli.parse`
behaves as if the environment record were empty.

`cli.parse_full` returns `{values: Record, sources: Record, warnings:
List[Str]}`. `values` is the same record returned by `cli.parse`; `sources`
maps fields to `"argv"`, `"env"`, `"default"`, or `"absent"`; `warnings`
contains deprecation messages. `cli.usage` renders a plain usage string from
the schema, includes the implicit `-h, --help` option, and skips hidden options.

`cli.commands` parses subcommand-style CLIs. The `commands` record maps command
names to descriptors with `positionals: List[Str]`, optional `types: Record`
using the same scalar type strings as `cli.parse`, optional `rest: Str`,
optional `min_rest: Int`, optional `aliases: List[Str]`, optional `form: Str`,
and optional `options: Record`. Command names with `-` in argv match descriptor
fields with `_`. The result always includes canonical `command: Str` and entered
`action: Str`; named positionals, rest arguments, and parsed command options are
added as fields. If `rootless_default` is non-empty and argv does not begin with
a known command or fallback command, that descriptor is used without consuming a
command token. `fallback_command` can parse extension-style commands; with
`command_like: true`, only relative slash-free, non-dot-prefixed tokens are
accepted as fallback commands.

`cli.tokens` is the lightweight BusyBox/getopt helper. It returns records with
`kind: Str`, `name: Str`, and `value: Str`. `kind` is `"short"`, `"long"`, or
`"operand"`. Short clusters such as `-abc` become three short tokens unless a
flag name appears in `value_flags`; then the remainder of the cluster or the
next argv item is used as that token's value. Long options accept `--name=value`
and, when `name` appears in `value_flags`, `--name value`. A bare `--` stops
flag parsing and emits the remaining items as operands. Tokens that look like
negative numbers, such as `-1`, are operands rather than short-option clusters.

`diff`:

- `diff.unified(original: Path, modified: Path, context: Int = 3) ->
  Result[Record]`.

`diff.unified` reads UTF-8 text files and returns `{files: Int, hunks: Int,
text: Str}` where `text` is a unified diff. The generated headers use each
path's file name.

`dns`:

- `dns.lookup(name: Str, record: Str = "A", server: Str = "",
  timeout: Duration = 5s) -> Result[List[Record]]`.
- `dns.resolve_host(name: Str, family: Str = "any") -> Result[List[Record]]`.
- `dns.reverse(addr: Str) -> Result[List[Str]]`.
- `dns.nameservers() -> Result[List[Str]]`.

`dns.lookup` supports `A` and `AAAA` records and returns records with `name:
Str`, `record: Str`, `value: Str`, and `ttl: Int`. With the default empty
`server`, it uses the host resolver. With a non-empty `server`, it sends an
explicit UDP DNS query to that server; bare IP/host values use port 53, and
`host:port`/`[ipv6]:port` values use the supplied port. Explicit server lookup
does not currently follow CNAME chains or fall back to TCP for truncated
responses. `dns.resolve_host` returns records with `name: Str`, `family: Str`,
and `addr: Str`; `family` is `"any"`, `"ipv4"`, or `"ipv6"`. DNS failures use
structured error kinds for invalid names, unsupported records, server failures,
timeouts, missing records, malformed responses, truncated responses, and reverse
lookup failures.

`patch`:

- `patch.apply(root: Path, text: Str, strip_components: Int = 0,
  overwrite: Bool = false) -> Result[Record]`.

`patch.apply` applies unified or git-style text patches under `root` and returns
`{files: Int, hunks: Int}`. It rejects absolute paths, parent traversal,
symlink roots, symlink ancestors, symlink file targets, unsupported binary
patches, and create/copy/rename overwrites unless `overwrite` is true. Modified
files are written through a temporary file in the destination directory.

`fs`:

- `fs.walk(path: Path, gitignore: Bool = true, stat: Bool = true, hidden: Bool = false) -> Result[Stream[Record]]`.
  The walk is **parallel and unordered** (entries arrive in traversal completion
  order, not sorted) using one worker per CPU. Use `|> sort-by .path` when a
  deterministic order matters. `stat: false` skips the per-entry `stat` (zeroes
  the size/mode/time fields) for a cheaper traversal. Hidden entries are skipped
  by default; pass `hidden: true` to include dot-prefixed files and directories.
- `fs.files(path: Path, gitignore: Bool = true, stat: Bool = true, exts: List[Str] = [], hidden: Bool = false) -> Result[Stream[Record]]` —
  equivalent to `fs.walk |> where .kind == "file"`. Preferred over the full
  walk when only files are needed. When `exts` is non-empty, only files whose
  no-dot, case-sensitive `ext` field is in the list are emitted; directories are
  still traversed. The filter is applied before file records are built, so it
  avoids per-file `stat` work for non-matching files when `stat: true`. Include
  `""` to emit extensionless files.
- `fs.dirs(path: Path, gitignore: Bool = true, stat: Bool = true, hidden: Bool = false) -> Result[Stream[Record]]` —
  equivalent to `fs.walk |> where .kind == "dir"`.
- `fs.ls(path: Path, stat: Bool = true, ordered: Bool = true) -> Result[Stream[Record]]`.
- `fs.children(path: Path, stat: Bool = true, ordered: Bool = true) -> Result[Stream[Record]]`.
- `fs.metadata(path: Path) -> Result[Record]`.
- `fs.filesystem_stats(path: Path) -> Result[Record]`.
- `fs.mounts() -> Result[Stream[Record]]`; call `.collect()` when a reusable list is needed.
- `fs.mount_for(path: Path) -> Result[Record]`.
- `fs.cwd() -> Result[Path]`.
- `fs.read_text(path: Path) -> Result[Str]`, requiring valid UTF-8.
- `fs.write(path: Path, data: Bytes) -> Result[Unit]`.
- `fs.write(path: Path, data: Str) -> Result[Unit]`.
- `fs.write_atomic(path: Path, data: Bytes) -> Result[Unit]`.
- `fs.write_atomic(path: Path, data: Str) -> Result[Unit]`.
- `fs.exists(path: Path) -> Result[Bool]`.
- `fs.executable(path: Path) -> Result[Bool]`.
- `fs.executable(mode: Int) -> Bool`.
- `fs.world_writable(mode: Int) -> Bool`.
- `fs.sticky(mode: Int) -> Bool`.
- `fs.setuid(mode: Int) -> Bool`.
- `fs.setgid(mode: Int) -> Bool`.
- `fs.owner_executable(mode: Int) -> Bool`.
- `fs.group_executable(mode: Int) -> Bool`.
- `fs.other_executable(mode: Int) -> Bool`.
- `fs.open_root(path: Path) -> Result[FsRoot]`.
- `fs.close_root(root: FsRoot) -> Result[Unit]`.
- `fs.root_path(root: FsRoot) -> Result[Path]`.
- `fs.root(root: FsRoot, path: Path) -> Result[FsRoot]`.
- `fs.root_read(root: FsRoot, path: Path) -> Result[Bytes]`.
- `fs.root_read_text(root: FsRoot, path: Path) -> Result[Str]`.
- `fs.root_write(root: FsRoot, path: Path, data: Bytes) -> Result[Unit]`.
- `fs.root_write(root: FsRoot, path: Path, data: Str) -> Result[Unit]`.
- `fs.root_write_atomic(root: FsRoot, path: Path, data: Bytes) -> Result[Unit]`.
- `fs.root_write_atomic(root: FsRoot, path: Path, data: Str) -> Result[Unit]`.
- `fs.root_metadata(root: FsRoot, path: Path) -> Result[FsEntry]`.
- `fs.root_exists(root: FsRoot, path: Path) -> Result[Bool]`.
- `fs.root_mkdir(root: FsRoot, path: Path, mode: Int = 0o777, parents: Bool = false) -> Result[Unit]`.
- `fs.root_remove(root: FsRoot, path: Path, dir: Bool = false) -> Result[Unit]`.
- `fs.root_readlink(root: FsRoot, path: Path) -> Result[Path]`.
- `fs.root_symlink(root: FsRoot, target: Path, path: Path, parents: Bool = true,
  overwrite: Bool = false) -> Result[Unit]`.
- `fs.root_chmod(root: FsRoot, path: Path, mode: Int) -> Result[Unit]`.
- `fs.root_install_file(source_root: FsRoot, source: Path, dest_root: FsRoot,
  dest: Path, mode: Int, parents: Bool = true,
  overwrite: Bool = false) -> Result[Unit]`.
- `fs.copy(source: Path, dest: Path, overwrite: Bool = false) -> Result[Unit]`.
- `fs.copy_tree(source: Path, dest: Path, parents: Bool = false,
  overwrite: Bool = false, follow_symlinks: Bool = false) -> Result[Record]`.
- `fs.rename(source: Path, dest: Path, overwrite: Bool = false) -> Result[Unit]`.
- `fs.mkdir(path: Path, parents: Bool = true) -> Result[Unit]`.
- `fs.remove(path: Path, missing_ok: Bool = false) -> Result[Unit]`.
- `fs.remove_manifest(root: Path, manifest: List[Path],
  missing_ok: Bool = false, prune_dirs: Bool = true) -> Result[Record]`.
- `fs.install(source: Path, dest: Path, mode: Int, parents: Bool = false, overwrite: Bool = false) -> Result[Unit]`.
- `fs.install_as(source: Path, dest: Path, mode: Int, owner: User,
  group: Group, parents: Bool = false, overwrite: Bool = false) -> Result[Unit]`.
- `fs.chmod(path: Path, mode: Int) -> Result[Unit]`.
- `fs.chown(path: Path, owner: User,
  follow_symlinks: Bool = false) -> Result[Unit]`.
- `fs.chgrp(path: Path, group: Group,
  follow_symlinks: Bool = false) -> Result[Unit]`.
- `fs.mkfifo(path: Path, mode: Int) -> Result[Unit]`.
- `fs.fsync(path: Path) -> Result[Unit]`.
- `fs.sync() -> Result[Unit]`.
- `fs.symlink(target: Path, path: Path) -> Result[Unit]`.
- `fs.lock(path: Path, shared: Bool = false,
  nonblocking: Bool = false) -> Result[Record]`.
- `fs.unlock(lock: Record) -> Result[Unit]`.
- `fs.tempfile() -> Result[{root: FsRoot, path: Path}]`.
- `fs.tempdir() -> Result[FsRoot]`.
- `fs.project_root(kind: Str, qualifier: Str, organization: Str,
  application: Str) -> Result[FsRoot]`.
- `fs.user_root(kind: Str) -> Result[FsRoot]`.

Filesystem entry records have `path: Path`, `name: Str`, `kind: Str`,
`ext: Str`, `size: Int`, `blocks_512: Int`, `mode: Int`, `uid: Int`,
`gid: Int`, `modified: Int`, `accessed: Int`, `executable: Bool`,
`world_writable: Bool`, `sticky: Bool`, `setuid: Bool`, `setgid: Bool`,
`owner_executable: Bool`, `group_executable: Bool`, and
`other_executable: Bool`. `fs.walk` skips hidden entries and `.git` directories
and honors `.gitignore` files by default; pass `hidden: true` to include hidden
entries and `gitignore: false` to disable ignore-file rules. `ext` is the file
extension without a leading dot. When `stat: false`, stat-derived numeric and
permission fields are zero or false; `path`, `name`, `kind`, and `ext` remain
populated.
Filesystem stats records have `blocks_1k: Int`, `used_1k: Int`,
`available_1k: Int`, and `capacity_percent: Int`.
Filesystem mount records have `filesystem: Str`, `mounted_on: Path`,
`fstype: Str`, `blocks_1k: Int`, `used_1k: Int`, `available_1k: Int`,
`capacity_percent: Int`, `files: Int`, `files_used: Int`, `files_free: Int`,
`files_capacity_percent: Int`, and `readonly: Bool`. `fs.mounts` returns the
host's currently mounted filesystems. `fs.mount_for(path)` resolves `path` when
possible and returns the longest matching mounted filesystem. Linux reads
`/proc/self/mountinfo` and `statvfs(2)`; macOS reads mount entries through
native mount APIs and filesystem counters through `statvfs(2)`.

`fs.copy_tree` refuses an existing destination unless `overwrite` is explicit,
preserves symlinks by default, and requires `follow_symlinks` when the caller
wants traversal through links. `fs.remove_manifest` accepts only relative
manifest paths without `..`; it removes listed files, symlinks, or empty
directories under `root`, then prunes empty parent directories when requested.
`fs.chown`, `fs.chgrp`, and `fs.install_as` take records from the `user` and
`group` modules instead of string names. `fs.lock` returns a lock record held by
the current XSH process until `fs.unlock` or process exit.
`fs.open_root`, `fs.tempdir`, `fs.project_root`, and `fs.user_root` return an
opaque `FsRoot` record `{id: Int}` backed by an open directory handle owned by
the evaluator. `fs.tempfile` returns `{root: FsRoot, path: Path}` where `path`
is relative to the returned root. `fs.root_path` is an explicit escape hatch for
APIs or subprocesses that still require host paths; it returns `Err` when the
root is closed or the platform cannot expose the path. `fs.root_*` operations
resolve relative paths from the handle rather than by joining strings; they
reject absolute paths, `..`, and symlink traversal. `fs.root_readlink` and
`fs.root_symlink` operate on symlink target text without traversing it. This
makes the rooted APIs the preferred surface when a trusted root directory is
combined with untrusted relative names.

`path`:

- `path.absolute(path: Path) -> Result[Path]`.

`path.absolute` joins relative paths to the current XSH cwd and lexically
normalizes `.` and `..` components without requiring the resulting path to
exist. Absolute inputs are normalized in place. Use `.resolve()` when the path
must exist and symlinks should be resolved by the host filesystem.

Path values also expose `.parent`, `.name`, `.ext`, `.display()`,
`.normalize()`, `.resolve()`, `.exists()`, `.executable()`, `.du()`,
`.metadata()`, `.read_bytes()`, `.read_text()`, `.lines()`, `.bytes_lines()`,
`.write(data: Bytes)`, `.write(data: Str)`,
`.write_atomic(data: Bytes)`, `.write_atomic(data: Str)`,
`.copy(dest: Path, overwrite: Bool = false)`,
`.rename(dest: Path, overwrite: Bool = false)`,
`.mkdir(parents: Bool = true)`, `.remove(missing_ok: Bool = false)`,
`.remove_dir()`, `.touch(create: Bool = true)`, `.truncate(size: Int)`,
`.chmod(mode: Int)`, `.hardlink(path: Path)`, `.unlink()`, `.readlink()`,
`.strip_prefix(prefix: Path) -> Result[Path]`,
`.relative_to(base: Path) -> Path` — returns `strip_prefix(base) ?? self`,
never fails; preferred over `strip_prefix` when a fallback to the original
path is acceptable, and
`.with_ext(ext: Str) -> Path`.

`env`:

- `env(name: Str) -> Result[Str]`.
- `env.get(name: Str) -> Result[Str]`.
- `env.get_or(name: Str, fallback: Str = "") -> Result[Str]`.
- `env.bool(name: Str, fallback: Bool = false) -> Result[Bool]`.
- `env.path(name: Str, fallback: Path = p"") -> Result[Path]`.
- `env.int(name: Str, fallback: Int = 0) -> Result[Int]`.
- `env.path_list(name: Str) -> Result[List[Path]]`.
- `env.path_entries(name: Str) -> Result[List[Record]]`.

`env.path_entries` preserves empty PATH-like entries and returns records
`{index: Int, raw: Str, path: Path, empty: Bool}`.

Static typed environment access uses field syntax:

- `env.Str.NAME -> Result[Str]`.
- `env.Path.NAME -> Result[Path]`.
- `env.PathList.NAME -> Result[List[Path]]`.

`env.PATH` is a scoped mutable path-list view with
`prepend(path: Path) -> Result[Unit]`, `append(path: Path) -> Result[Unit]`,
and `pop() -> Result[Path]`. Membership with `in` and `not in` is supported.

`module`:

- `module.load(path: Path) -> Result[Module]`.

Runtime-loaded modules follow the same top-level restrictions as imported user
modules: explicit exports, no top-level commands, no top-level mutation, no
top-level control flow, and imports resolved relative to the loaded file.
Exported procs and pures are callable exports on the returned module value.

Dynamic module values can be refined with `.require(ModuleContract)?`, where
`ModuleContract` is a `type Name = module { ... }` contract. After refinement,
required exports have the contract's field types and proc or pure exports may
be called directly:

```xsh
type BuildPlugin = module {
  export let name: Str
  export optional let description: Str
  export proc build(root: Path) [fs, process, error] -> Result[Unit]
}

let plugin = module.load(plugin_path)?.require(BuildPlugin)?
print ${plugin.name}
plugin.build(root)?
```

Module values are immutable export records. They support `.has(field: Str)`,
`.get(field: Str) -> Result[Any]`, `.keys()`, field access for known exports,
and string indexing. Use `.get()` or `.has()` before accessing optional exports
when absence is expected. Exported types are checker-visible through static
imports, but they are not runtime module fields.

`record`:

- `record.require(record: Record, required: Record, optional: Record = {},
  source: Path = p"") -> Result[Record]`.

`record.require` validates dynamic records, especially values returned by JSON
decoding or schema-erased plumbing. It uses the same contract string format
described below; failures return structured record contract errors. Contract
records map field names to type
strings such as `"Str"`, `"Bool"`, `"Path"`, `"Proc"`, `"List[Str]"`,
`"List[Path]"`, or proc signatures such as `"Proc(Path) -> Result[Unit]"`.
`"Any"` is the dynamic contract type. Missing required fields and present
fields with wrong dynamic types return messages that include the field name,
expected type, actual dynamic type when a value is present, and the optional
source path.

Record values also expose `.has(field: Str)`, `.get(field: Str) ->
Result[Any]`, and `.keys()`. `.get()` returns a structured missing-field error
when the field is absent.

`net`:

- `net.pool(name: Str = "default", max_idle_per_host: Int = 8,
  idle_timeout: Duration = 90s) -> Result[Record]`.
- `net.close_pool(name: Str = "default") -> Result[Unit]`.
- `net.close_all_pools() -> Result[Unit]`.
- `net.request(request: Record) -> Result[Record]`.
- `net.download(request: Record) -> Result[Record]`.
- `net.upload(request: Record) -> Result[Record]`.

`net` supports HTTP and HTTPS only. It uses runtime-managed Hyper clients with
Rustls and the `aws-lc-rs` crypto provider, keyed by caller-visible pool name
and TLS configuration. Hostnames are resolved by the XSH DNS helper layer, and
TCP sockets are opened through `cap-net-ext` using a `cap-std` network pool. TLS
verification is enabled by default with platform verification, honors
`SSL_CERT_FILE`, accepts an explicit `ca_certificate: Path`, and allows
`tls_verify: false` only through an explicit request field.

`net.request` accepts a request record with `method: Str`, `url: Str`,
`headers: List[Record]`, optional `body: Bytes`, `body_text: Str`, or
`body_file: Path`, `pool: Str`, `timeout: Duration`, `connect_timeout:
Duration`, `redirects: Int`, `tls_verify: Bool`, `ca_certificate: Path`,
`fail_status: Bool`, and `max_body_bytes: Int`. Methods are `GET`, `HEAD`,
`POST`, `PUT`, `PATCH`, and `DELETE`.

`net.request` returns `status: Int`, `reason: Str`, `bytes: Int`, `headers:
List[Record]`, `url: Str`, and `body: Bytes`. `net.download` and `net.upload`
return the same metadata without `body`; downloads write through a temporary
file and rename atomically by default. Unsupported schemes, invalid URLs,
unsupported methods, TLS failures, DNS failures, redirects, timeouts, status
failures when `fail_status` is true, and response-size limits use structured
error kinds.

List values expose collection operations as methods:

- `.len() -> Int`.
- `.push(item: T) -> List[T]`.
- `.extend(more: List[T]) -> List[T]`.
- `.contains(item: T) -> Bool`.
- `.get(index: Int) -> Result[T]`.
- `.get(index: Int, fallback: T) -> T`.
- `.join(separator: Str = "") -> Str` (only when `T` is `Str`).

`map`:

- `map.empty() -> Map[T]`; empty maps usually need an expected type from a
  binding annotation or later typed API boundary. In those map-typed contexts,
  `{}` is equivalent to `map.empty()`.

Map values expose all routine map operations as methods:

- `.len() -> Int`.
- `.has(key: Str) -> Bool`.
- `.get(key: Str) -> Result[T]`.
- `.get(key: Str, default: T) -> T`.
- `.set(key: Str, value: T) -> Map[T]`.
- `.push(key: Str, value: T) -> Map[List[T]]` when the receiver is
  `Map[List[T]]`; missing keys are created with a singleton list.
- `.remove(key: Str) -> Map[T]`.
- `.keys() -> List[Str]`, in deterministic key order.
- `.values() -> List[T]`, in deterministic key order.

`set`:

String-key sets are represented as `Map[Bool]` and constructed through the
`set` module:

- `set.empty() -> Map[Bool]`.
- `set.from(items: List[Str]) -> Map[Bool]`.
- `set.has(set: Map[Bool], item: Str) -> Bool`.
- `set.add(set: Map[Bool], item: Str) -> Map[Bool]`.
- `set.remove(set: Map[Bool], item: Str) -> Map[Bool]`.

`text`:

`Str` values expose all text operations as methods. The structured stream
adapter `text.lines()` is available in pipelines.

- `.trim() -> Str`.
- `.starts_with(prefix: Str) -> Bool`.
- `.ends_with(suffix: Str) -> Bool`.
- `.contains(needle: Str) -> Bool`.
- `.lines() -> Stream[Str]`.
- `.words() -> List[Str]`, split with Unicode whitespace semantics.
- `.split(separator: Str) -> List[Str]`; an empty separator splits into Unicode
  scalar values.
- `.fields(delimiter: Str = "") -> List[Str]`; an empty delimiter uses Unicode
  whitespace and a non-empty delimiter drops empty fields.
- `.replace(from: Str, to: Str) -> Str`.
- `.wrap(width: Int) -> List[Str]`, greedily wrapping on Unicode whitespace and
  breaking long words by Unicode scalar count.
- `.translate(from: Str, to: Str) -> Str`, replacing each scalar in `from`
  with the scalar at the same position in `to`; extra `from` scalars are deleted.
- `.lower() -> Str` / `.upper() -> Str`, Unicode case folding.
- `.delete(chars: Str) -> Str`, deleting listed Unicode scalars.
- `.squeeze(chars: Str = "") -> Str`, collapsing consecutive repeated scalars
  from `chars`; an empty `chars` squeezes all repeated scalars.
- `.reverse() -> Str`, by Unicode scalar value.
- `.count_lines() -> Int`.
- `.count_words() -> Int`.
- `.count_chars() -> Int`, by Unicode scalar value.
- `.count_bytes() -> Int`.
- `.byte_len() -> Int`, equivalent to `.count_bytes()`.
- `.byte_at(index: Int, default: Int = -1) -> Int`, returning the byte value at
  byte index `index` or `default` when out of range.
- `.byte_slice(offset: Int, length: Int = rest) -> Str`, slicing by byte offset
  and length.
- `.find(needle: Str, start: Int = 0) -> Int`, returning the byte index of
  `needle` at or after byte index `start`, or `-1` when missing.
- `.parse_int() -> Result[Int]`, accepting decimal, `0x` hexadecimal, `0o`
  octal, `0b` binary, `_` separators, and an optional leading sign.

Byte-indexed `Str` methods are intended for ASCII-oriented scanners. They count
UTF-8 bytes, not Unicode scalar values. `.byte_slice()` rejects negative offsets
or lengths, offsets past the end of the text, and slices that do not align to
UTF-8 boundaries.

`Str` values also expose `.base64_decode() -> Result[Bytes]` and
`.base32_decode() -> Result[Bytes]`.

`Int`:

- `.float() -> Float`.

`Float`:

- `.floor() -> Result[Int]`.
- `.ceil() -> Result[Int]`.
- `.round() -> Result[Int]`.
- `.format(precision: Int = 6) -> Str`.

Float-to-`Int` conversions reject `NaN`, infinities, and values outside the
`Int` range. `format` requires a precision between `0` and `100`.

`regex`:

- `regex.compile(pattern: Str) -> Result[Regex]`.
- `Regex.matches(text: Str) -> Bool`.
- `Regex.find(text: Str) -> List[Record]`.
- `Regex.captures(text: Str) -> List[Str]`.
- `Regex.replace(text: Str, replacement: Str) -> Str`.

Regex APIs use Rust `regex-lite` syntax. The common Rust regex surface is
supported, including captures, alternation, repetition, inline flags, byte
offsets, and replacement, while Unicode property classes such as `\p{...}` and
`\P{...}` are outside the v1 surface. Compile errors return structured regex
compile errors from `regex.compile(...)`. A `Regex` value has already validated
its pattern, so its methods return plain values instead of `Result`. `captures`
returns an empty
list when there is no match; otherwise index 0 is the full match and subsequent
items are capture groups in order, with unmatched optional groups represented
as empty strings. Match records expose byte offsets as `start` and `end` plus
the matched `text`. Fixed-string operations remain on `Str` methods or
ordinary string membership; callers choose regex behavior explicitly at the API
boundary.

`bytes`:

- `bytes.human(size: Int) -> Str`, formatting a byte count with compact binary
  units.
- `bytes.copy(source: Path, dest: Path, block_size: Int = 512,
  count: Int = rest, skip: Int = 0, seek: Int = 0,
  overwrite: Bool = false) -> Result[Record]`, copying whole or partial blocks
  from `source` to `dest` and returning `bytes` and `blocks` counts.

`bytes.copy` rejects non-file sources, refuses to overwrite existing
destinations by default, and rejects symlink destinations.

`Bytes` values also expose `.len()`, `.slice(offset: Int, length: Int = rest)`,
`.dump(format: Str = "canonical")`, `.strings(min_len: Int = 4)`, `.base64()`,
`.base32()`, `.utf8()`, `.chunks(size: Int)`, `.compare(other: Bytes)`,
`.md5()`, `.sha1()`, `.sha256()`, and `.sha512()`.

`Bytes` also exposes byte-oriented text scanning that mirrors the matching
`Str` methods but takes and returns `Bytes`, for processing file content
without first requiring valid UTF-8:

- `.lines() -> Stream[Bytes]` splits on `\n` and drops a trailing `\r`, like
  `Str.lines()`.
- `.count_lines() -> Int` counts those lines without allocating them.
- `.trim() -> Bytes` removes leading and trailing whitespace, matching
  `Str.trim()`'s Unicode `White_Space` semantics on valid UTF-8.
- `.starts_with(prefix: Bytes) -> Bool`, `.ends_with(suffix: Bytes) -> Bool`,
  and `.contains(needle: Bytes) -> Bool` are byte searches.
- `.lower() -> Bytes` lowercases ASCII bytes only, leaving other bytes intact.
- `.byte_at(index: Int, default: Int = -1) -> Int` returns the byte value at
  `index`, or `default` when out of range.

`.slice()` rejects negative offsets and lengths and offsets past the end of the
input. `.dump()` output is deterministic text for rendering or manifest data,
not a parser boundary. `.base64_decode()` and `.base32_decode()` live on `Str`
because decoding starts from encoded text.

`.compare()` returns `equal: Bool`, `byte: Int`, `line: Int`, `left: Int`,
and `right: Int`. `byte` and `line` are one-based at the first difference.
For equal inputs, `byte` and `line` are `0` and byte values are `-1`. At EOF,
the missing side is `-1`; otherwise `left` and `right` are byte values from
`0` through `255`.

`io`:

- `io.stdin_bytes() -> Result[Bytes]`.
- `io.stdin_text() -> Result[Str]`, requiring valid UTF-8.
- `io.stdin_line() -> Result[Str]`, reading one UTF-8 line without the trailing
  newline.
- `io.write_stdout(text: Str) -> Result[Unit]`.
- `io.write_stdout_bytes(data: Bytes) -> Result[Unit]`.

`io` functions read from the script process's stdin and append directly to its
stdout without adding a newline. In the current runtime, script stdout is
UTF-8-backed, so `write_stdout_bytes` rejects bytes that are not valid UTF-8.
Use it to preserve "no automatic newline and no display conversion" semantics;
fully arbitrary binary script stdout remains a future runtime-output model
extension.

`hash`:

- `hash.md5(data: Bytes) -> Digest`.
- `hash.md5(path: Path) -> Result[Digest]`.
- `hash.sha1(data: Bytes) -> Digest`.
- `hash.sha1(path: Path) -> Result[Digest]`.
- `hash.sha256(data: Bytes) -> Digest`.
- `hash.sha256(path: Path) -> Result[Digest]`.
- `hash.sha512(data: Bytes) -> Digest`.
- `hash.sha512(path: Path) -> Result[Digest]`.
- `hash.verify_file(path: Path, sha256: Str) -> Result[Unit]`; the named
  checksum may also be `md5`, `sha1`, or `sha512`.
- `hash.parse_check_line(line: Str) -> Result[Record]`, accepting GNU-style
  `<hex>  <path>` and `<hex> *<path>` checksum lines and returning `hex`,
  `path`, and `binary` fields.

`mime`:

- `mime.lookup_ext(ext: Str) -> {mime: Str, exts: List[Str]}?`.
- `mime.lookup_path(path: Path) -> {mime: Str, exts: List[Str]}?`.
- `mime.parse(value: Str) -> Result[Record]`, returning
  `{type: Str, params: Map[Str]}`.

Extensions are accepted with or without a leading dot and are normalized to
lowercase. Lookup uses a small built-in table, then reads `/etc/mime.types` if
available; entries from that file override built-ins by extension, and later
host entries override earlier ones. Missing or unreadable `/etc/mime.types` is
ignored. Host lines whose media type is malformed or that contain no extensions
are ignored.

`mime.lookup_path` checks compound extensions before shorter suffixes, so
`package.tar.gz` can match `tar.gz` before `gz`. A missing lookup returns
`Null`. `mime.parse` lowercases the `type/subtype` value and parameter names,
keeps parameter values as strings, accepts token values and quoted strings, and
returns `Err(mime-parse)` for malformed media types.

`ini`:

- `ini.decode(text: Str) -> Result[Record]`.
- `ini.read(path: Path) -> Result[Record]`.
- `ini.encode(value: Record) -> Result[Str]`.
- `ini.write(path: Path, value: Record, overwrite: Bool = true)
  -> Result[Unit]`.

The accepted INI dialect is a conservative Python `ConfigParser`-style data
subset, not universal INI compatibility. `#` and `;` begin whole-line comments
after leading whitespace. Key/value entries use `=` or `:` delimiters. Section
headers are `[section]`. Indented non-empty lines continue the previous value,
joined with `\n`. Inline comments, interpolation, valueless keys, and typed
value inference are not supported.

Global keys before the first section become top-level string fields. Sections
become top-level record fields. Global keys and section names share one
top-level namespace; collisions are rejected. Option names are normalized to
lowercase, and duplicate options are rejected case-insensitively. Section names
are preserved exactly and duplicate sections are rejected.

`ini.encode` accepts records whose top-level fields are either string globals
or section records containing only string values. Output is deterministic:
global keys are sorted first, then sections sorted by name, then keys sorted
within each section. Multiline string values are emitted as continuation lines.
`ini.write(..., overwrite: false)` returns `Err(ini-write)` if the destination
already exists.

`shlex`:

- `shlex.quote(value: Str) -> Str`.
- `shlex.join(argv: List[Str]) -> Str`.

`shlex.quote` renders exactly one POSIX-like shell word that evaluates back to
the original string. Empty strings render as `''`; safe words made only of
ASCII letters, digits, `_@%+=:,./-` are left unquoted; other words use single
quotes with embedded single quotes escaped. `shlex.join` quotes each argv item
independently and joins them with one space. This API is for rendering
diagnostics, snippets, and explicit interop text. It does not enable shell
execution, splitting, expansion, globbing, or command substitution; use typed
argv and `Command` values for process execution.

`json`:

- `json.decode(s: Str) -> Result[Any]`.
- `json.encode(value: JSON-compatible, pretty: Bool = false) -> Result[Str]`.
- `json.encode_lines(values: List[JSON-compatible]) -> Result[Str]`.
- `json.get(value: Any, path: List[Any]) -> Result[Any]`.
- `json.get(value: Any, path: List[Any], fallback: Any) -> Any`.
- `json.read(path: Path) -> Result[Any]`.
- `json.remove(value: Any, path: List[Any]) -> Result[Any]`.
- `json.set(value: Any, path: List[Any], replacement: JSON-compatible) -> Result[Any]`.
- `json.write(path: Path, value: JSON-compatible, pretty: Bool = false) -> Result[Unit]`.
- `json.write_lines(path: Path, values: List[JSON-compatible]) -> Result[Unit]`.

The structured adapter stages `text.lines()`, `bytes.chunks(size)`,
`json.lines()`, and `json.stream()` are valid only as the first structured
pipeline stage. Text and JSON adapters accept `Str`; invalid UTF-8 must be
rejected at the explicit text decoding boundary, such as `run.text`.
`bytes.chunks` performs no decoding.

`linux`:

The `linux` module is a narrow privileged surface for Linux kernel, procfs, and
early boot operations. Its checker-visible signatures exist so XSH init scripts
can keep policy in script code while depending only on syscall-level host
primitives. Runtime calls are gated by default. Set `XSH_LINUX_DRY_RUN=1` to
run the dry-run implementation used by the baseinit proof; it validates
arguments, writes random-seed output for `read_device`, and appends JSON-lines
operation records when `XSH_LINUX_DRY_RUN_LOG` is set. On Linux, set
`XSH_LINUX_REAL=1` to run the privileged syscall implementation. Non-Linux
hosts reject real mode with a structured unsupported error.
Dry-run `linux.file_attrs` defaults to the immutable and append-only flags,
and `linux.file_version` defaults to `0`. They accept
`XSH_LINUX_FILE_ATTRS_FLAGS` and `XSH_LINUX_FILE_VERSION` decimal overrides.

- `linux.mount(source: Str, target: Path, fstype: Str = "",
  options: List[Str] = []) -> Result[Unit]`.
- `linux.mount_all() -> Result[Unit]`.
- `linux.umount_all(types: List[Str] = []) -> Result[Unit]`.
- `linux.swapon_all() -> Result[Unit]`.
- `linux.swapoff_all() -> Result[Unit]`.
- `linux.root_device() -> Result[Str]`.
- `linux.link_up(interface: Str) -> Result[Unit]`.
- `linux.set_ipv4_address(interface: Str, address: Str, netmask: Str) -> Result[Unit]`.
- `linux.add_default_ipv4_route(gateway: Str, interface: Str = "") -> Result[Unit]`.
- `linux.interfaces() -> Result[Stream[Record]]`, returning
  `{name, flags, mtu, mac, addresses}` records from `/sys/class/net` and
  `getifaddrs(3)`.
- `linux.routes() -> Result[Stream[Record]]`, returning
  `{family, dst, prefix_len, gateway, dev, metric, flags}` records from
  `/proc/net/route` and `/proc/net/ipv6_route`.
- `linux.meminfo() -> Result[Record]`, returning
  `{total, free, available, buffers, cached, swap_total, swap_free}` byte
  counts from `/proc/meminfo`.
- `linux.modules() -> Result[Stream[Record]]`, returning `{name, size, used_by}`
  records from `/proc/modules`.
- `linux.dmesg() -> Result[Stream[Str]]`, returning kernel log messages read from
  `/dev/kmsg` or the kernel log buffer.
- `linux.is_mountpoint(path: Path) -> Result[Bool]`, comparing the path and
  parent device metadata.
- `linux.disk_usage(path: Path = default) -> Result[Stream[Record]]`, returning
  `{device, mount, fstype, total, used, available}` byte counts from
  `/proc/mounts` and `statvfs(2)`. Omitting `path` returns all mounts; passing a
  path returns the best matching mount.
- `linux.sysctl_get(key: Str) -> Result[Str]`.
- `linux.sysctl_set(key: Str, value: Str) -> Result[Unit]`.
- `linux.file_attrs(path: Path) -> Result[Record]`, returning file attribute
  flags as `{flags, indexed_directory, secure_deletion, undelete, sync,
  dirsync, immutable, append_only, no_dump, no_atime, compression_requested,
  journaled_data, no_tailmerging, top_of_directory_hierarchies}`.
- `linux.set_file_attrs(path: Path, flags: Int) -> Result[Unit]`.
- `linux.file_version(path: Path) -> Result[Int]`.
- `linux.set_file_version(path: Path, version: Int) -> Result[Unit]`.
- `linux.sysctl_load_dirs(dirs: List[Path], fallback: Path = default)
  -> Result[Unit]`.
- `linux.kill_all(signal: Str = "TERM", except_pid1: Bool = false)
  -> Result[Unit]`. This is the Linux killall5-style broad process signaling
  primitive. It skips the current process and the caller's session; ordinary
  process-name killall behavior belongs to `unix.kill_all`.
- `linux.read_device(device: Path, dest: Path, bytes: Int) -> Result[Unit]`.
- `linux.write_device(device: Path, source: Path) -> Result[Unit]`.
- `linux.uevent_stream() -> Result[Stream[Record]]`, returning kernel uevent
  records with `{action, subsystem, devname, devpath, env}` fields. In real
  mode it opens a `NETLINK_KOBJECT_UEVENT` socket and yields one record per
  direct `for`-loop iteration.
- `linux.halt() -> Result[Unit]`.
- `linux.poweroff() -> Result[Unit]`.
- `linux.reboot() -> Result[Unit]`.
- `linux.hwclock() -> Result[Int]`, reading the hardware clock as epoch
  milliseconds.
- `linux.set_hwclock(epoch_ms: Int) -> Result[Unit]`, writing the hardware
  clock.
- `linux.set_system_clock(epoch_ms: Int) -> Result[Unit]`, setting
  `CLOCK_REALTIME` from epoch milliseconds.

`unix`:

The `unix` module contains portable Unix process, PID/session, hostname,
uptime, and exec helpers that are not tied to Linux kernel boot details.
Set `XSH_UNIX_DRY_RUN=1` to dry-run PID/session/hostname/exec helpers; dry-run
calls return typed fake PID records and append JSON-lines operation records
when `XSH_UNIX_DRY_RUN_LOG` is set. `unix.set_hostname` is gated unless
`XSH_UNIX_DRY_RUN=1` or `XSH_UNIX_REAL=1` is set. `unix.uptime_seconds` and
`unix.tty` read the host by default, with dry-run overrides through
`XSH_UNIX_UPTIME_SECONDS` and `XSH_UNIX_TTY`.

- `unix.reap_child_events() -> Result[Stream[Record]]`, returning a single-use
  stream of currently available `{pid, status}` records.
- `unix.wait_pid1_event(timeout: Duration = default) -> Result[Record]`,
  returning `{kind, signal, children}` for the next PID 1 event. `kind` is
  `signal`, `children`, `poll`, or `timeout`. With no `timeout`, the call does a
  single bounded poll and returns `poll` if no signal or child reaping is
  pending. With a `timeout`, it blocks until a signal or child event arrives or
  the deadline elapses, returning `timeout` on expiry — letting a supervisor
  sleep until its next scheduled action instead of busy-polling.
- `unix.spawn_process_group(command: Command, notify: Bool = false) -> Result[Record]`,
  returning `{pid, command, argv, detach: true, new_session: false,
  ignore_hup: true, notify_fd}`. With `notify: true`, the supervisor creates a
  readiness pipe, passes the write end to the child as the fd named by the
  `NOTIFY_FD` environment variable (sd_notify convention), and returns the
  non-blocking read end as `notify_fd`; otherwise `notify_fd` is `-1`. The child
  writes any byte to `NOTIFY_FD` when ready.
- `unix.notify_ready(fd: Int) -> Result[Bool]`, a non-blocking probe of a
  readiness fd returned by `spawn_process_group`. Returns `true` once the child
  has written a readiness byte; `false` while nothing has arrived or after the
  writer closed without notifying. A negative `fd` returns `false`.
- `unix.notify_close(fd: Int) -> Result[Unit]`, releasing a readiness fd. A
  negative or already-closed fd is a no-op.
- `unix.spawn_with_tty(command: Command, tty: Str) -> Result[Record]`,
  returning `{pid, command, argv, detach: true, new_session: true,
  ignore_hup: true}`.
- `unix.kill_process_group(pid: Int, signal: Str) -> Result[Unit]`.
- `unix.exec(command: Command) -> Result[Unit]`.
- `unix.set_hostname(hostname: Str) -> Result[Unit]`.
- `unix.uptime_seconds() -> Result[Int]`.
- `unix.tty() -> Result[Str]`, returning the controlling terminal path for
  standard input.
- `unix.kill_all(name: Str, signal: Str = "TERM")
  -> Result[Record]`, returning `{matched: Int, signaled: Int}` after sending
  the signal to processes whose executable name matches exactly. It skips the
  current process and PID 1, does not match later shell argv tokens, and returns
  `Err(process-missing)` if no matching process was signaled.

`cpu`:

- `cpu.count() -> Int`.

`process`:

- `process.list() -> Result[Stream[Record]]`.
- `process.current_pid() -> Result[Int]`, returning the current XSH process id.
- `process.stats(pid: Int) -> Result[Record]`, returning `{rss_kb: Int,
  vsz_kb: Int}`. Unavailable fields are `-1`.
- `process.port(port: Int) -> Result[Stream[Record]]`, returning visible
  socket-owner records for local TCP/UDP sockets using that port.
- `process.ports() -> Result[Stream[Record]]`, returning visible socket-owner
  records for listening local TCP sockets and local UDP sockets.
- `process.ports(pid: Int) -> Result[Stream[Record]]`, returning visible
  listening local TCP sockets and local UDP sockets owned by that process.
- `process.which(name: Str) -> Result[Path]`.
- `process.signal(signal: Str) -> Result[Record]`, returning
  `{name: Str, number: Int}` for platform signal names such as `"TERM"` and
  `"SIGTERM"`.
- `process.kill(pid: Int, signal: Str = "TERM") -> Result[Unit]`.
- `process.argv_words(text: Str) -> Result[List[Str]]`, splitting command text
  into argv words using whitespace, single quotes, double quotes, and
  backslash escapes. Unquoted shell operators, expansions, globs, command
  substitution, and compound-command syntax are rejected.
- `process.command_argv(target: Str|Path, argv: List[Str|Path], cwd: Path = default,
  env: Record = default, stdin: Path = default, stdout: Path = default,
  stderr: Path = default, stdout_append: Bool = false,
  stderr_append: Bool = false, timeout: Duration = default,
  detach: Bool = false, new_session: Bool = false, ignore_hup: Bool = false,
  cpu_max: Int = default) -> Command`.
- `process.run(command: Command) -> Result[Status, ProcessError]`.
- `process.spawn(command: Command) -> Result[Record]`, returning
  `{pid: Int, command: Str, argv: Str, detach: Bool, new_session: Bool,
  ignore_hup: Bool}` after starting the command without waiting for completion.
- `process.command { run ... } -> Command`.

`process.spawn(command)` is not deprecated by `spawn command_expr`; it remains
the detached-record API for callers that intentionally do not want an owned
`ProcessHandle`.

`process.command_argv` accepts `target` and `argv` positionally or by name.
Defaulted plan fields may be supplied by name without filling earlier defaults,
for example `process.command_argv(cmd, argv, timeout: 1s, ignore_hup: true)`.

Process entry records have `pid: Int`, `parent_pid: Int`, `command: Str`,
`argv: Str`, `argv0: Str`, `user: Str`, `uid: Int`, `status: Str`,
`start_time: Str`, `start_time_ms: Int`, and `runtime_seconds: Int`.

`process.threads() -> Result[Stream[Record]]` returns thread records for Linux
tasks and macOS threads. `process.threads(pid: Int)` returns threads for one
process. Thread records include all process entry fields plus `owner_pid: Int`,
`thread_id: Int`, and `thread_name: Str`. On Linux, `pid` is the task/thread id
and `owner_pid` is the process id. On macOS, `pid` and `owner_pid` are both the
process id and `thread_id` is the native thread id.

Process port records have process fields `pid`, `parent_pid`, `command`,
`argv`, `argv0`, `user`, and `uid`; socket fields `protocol`, `local_address`,
`local_port`, `local`, `remote_address`, `remote_port`, `remote`, `state`,
`fd`, and `inode`. The API is best-effort: sockets hidden by host permissions
are omitted rather than reported with partial process data.

`time`:

- `time.now() -> Int`, returning epoch milliseconds.
- `time.sleep(duration: Duration) -> Result[Unit]`.
- `time.millis(ms: Int) -> Duration` and `time.seconds(seconds: Int) -> Duration`,
  constructing a `Duration` from a computed `Int`. Both are pure. A negative
  input clamps to a zero-length duration; `time.seconds` saturates rather than
  overflowing. These are the only way to build a `Duration` from a runtime value
  (literals such as `200ms` aside).
- `time.measure(command: Command, quiet: Bool = false) -> Result[Record]`, returning
  `{status: Status, duration_ms: Int, wall_ns: Int, user_ns: Int, system_ns: Int}`.
  `wall_ns` is nanosecond wall-clock time; `user_ns`/`system_ns` are the child's
  user/system CPU time; `duration_ms` is `wall_ns / 1_000_000` (kept for
  compatibility). With `quiet: true` the child's stdout/stderr go to `/dev/null`.
- `time.format(epoch_ms: Int, format: Str, utc: Bool = false) -> Result[Str]`,
  converting epoch milliseconds with Jiff, using UTC when `utc` is true and the
  system local timezone when `utc` is false, and formatting with Jiff's strict
  `fmt::strtime` percent grammar. Invalid formats, unsupported directives,
  local timezone lookup failures, and timestamps outside Jiff's supported range
  return `Err(time-format)`. This API is not byte-for-byte compatible with host
  libc `strftime`; XSH owns the Jiff-based output contract, including timestamp
  range, timezone lookup behavior, locale-independent directive output, and
  error messages.
- `time.duration_compact(seconds: Int) -> Str`, formatting seconds as a compact
  fixed-width duration label.

`tui`:

- `tui.reset()`, `bold()`, `dim()`, `red()`, `green()`, `yellow()`, `blue()`,
  `magenta()`, `cyan()`, `white()`, and `gray()` return ANSI SGR sequences.
- `tui.clear()`, `home()`, `erase_line()`, `hide_cursor()`, and
  `show_cursor()` return basic terminal control sequences.
- `tui.left_pad(text: Str, width: Int) -> Str` and
  `tui.right_pad(text: Str, width: Int) -> Str` pad to visible width while
  ignoring ANSI CSI escape sequences.

`system`:

- `system.hostname() -> Result[Str]`.
- `system.uname() -> Result[Record]`, returning `sysname`, `nodename`,
  `release`, `version`, and `machine`.
- `system.memory() -> Result[Record]`, returning `total`, `available`, `free`,
  `swap_total`, and `swap_free` byte counts.
- `system.os_release() -> Result[Record]`, returning `name`, `pretty_name`,
  `version`, `version_id`, and `id`.

`user`:

- `user.current() -> Result[Record]`.
- `user.lookup(name: Str) -> Result[Record]`.
- `user.by_uid(uid: Int) -> Result[Record]`.

User records have `name: Str`, `uid: Int`, `gid: Int`, `home: Path`, and
`shell: Str`.

`group`:

- `group.current() -> Result[Record]`.
- `group.lookup(name: Str) -> Result[Record]`.
- `group.by_gid(gid: Int) -> Result[Record]`.

Group records have `name: Str`, `gid: Int`, and `members: List[Str]`.

Module errors are structured errors with source spans at the call site.

Standard module signatures may use defaulted named parameters and overloads.
Overloads must be distinguishable from argument names or argument types.

## 14. Structured Streams

Structured streams are distinct from byte pipelines. The structured pipeline
operator `|>` lowers expressions and stages into a stream plan that carries
input type, stage kind, block spans, and item-context spans.

**Auto-collection.** A pipeline expression evaluates to `List[T]`. Items are
collected automatically at the pipeline boundary. Use `collect()` as an
explicit pipeline terminal when the materialization should be visible in the
pipeline, or `.collect()` on a stream value when an explicit materialized list
is needed outside pipeline syntax.

**Terminal stages** produce a scalar value instead of passing items forward.
They end the stream and cannot be followed by further stages.

**For loops.** `for x in PIPELINE { }` iterates the pipeline directly without
materializing a `List`. This is the preferred form when items are consumed
once and the list is not needed.

**Lazy sources.** `fs.walk`/`fs.files`/`fs.dirs`, `Path.lines()`,
`Path.bytes_lines()`, `Str.lines()`, `Bytes.lines()`, `run.stream`, and
user-defined `stream` producers yield live streams. Pipelines and direct `for`
loops consume these streams item by item until a materializing boundary or a
terminal stage requires a final value.

**Integer sequences.** `range(n)` and `range(start, n)` are builtin call
expressions that produce `Stream[Int]`, usable as pipeline sources or directly
in `for` loops.

Accepted syntax:

```xsh
let files = fs.walk("src")
  |> where .kind == "file"
  |> map .path
  |> sort

for file in fs.walk("src") |> where .kind == "file" {
  run cc -c ${file.path} -o ${file.path.with_ext("o")}
}

for i in range(5) {
  print f"step ${i}"
}
```

Value pipeline calls are accepted when a stage is an ordinary expression call
rather than a stream stage. Without an explicit `.` placeholder, the previous
pipeline value is inserted as the first argument:

```xsh
let readme_text = p"README.md".read_bytes()?.utf8()?

let warnings = fs.read_text(p"build.log")?
  |> text.lines()
  |> where { "warn" in . }
```

Accepted **transformation** stage kinds include `where`, `map`, `par-map`,
`each`, `batch`, `sort`, `sort-by`, `take`, `drop`, `unique-by`, `enumerate`,
`zip`, `range`, `repeat`, `tee`, and `flat-map`. These produce a stream.

Accepted **terminal** stage kinds: `count()`, `collect()`, `sum()`, `min()`,
`max()`, `first()`, `last()`, `any`, `all`, `fold(init) { ... }`, `reduce`,
`shuffle`, `group-by`, and `table.print(...)`. These produce a scalar,
materialized list, or consume the stream.

`each` is for effects and accepts a block whose result is `Unit` or
`Result[Unit]`. `map` and `par-map` are for values and require a final
expression or command tail value. Stage blocks may bind one explicit item
parameter with `{ |item| ... }`, but the implicit `.` item is available in
one-expression and multi-statement stage blocks.

A tail proc call with `?` unwraps the `Ok` value and propagates errors: if
any item fails, the entire stage short-circuits with that error. Without
`?`, the `Result` value flows through as-is — errors stay in-band as
`Result::Err` values in the output stream, and all items are processed.
This lets the caller choose between short-circuit semantics (use `?`) and
collect-all semantics (omit `?`), matching how Rust's rayon, Go's
goroutines, and Haskell's `parMap` separate parallelism from error
handling.

`where`, `any`, and `all` require `Bool` or `Result[Bool]`. `min()` and
`max()` return `Result[T]`. `first()` and `last()` return `Result[T]`.
`count()` returns `Int`. `table.print(...)` is a structured stream sink for
record streams. It renders terminal-width UTF-8 tables by default and wraps
long cell contents vertically instead of truncating with ellipses.

`flat-map` accepts `List[T]` or `Stream[T]` from its block. A live stream
returned by a block is drained for that input item before the outer stream
continues.

`fs.ls(...) |> table.print(...)` is the accepted standard listing interface.

`par-map` defaults to one worker per CPU. Use `--jobs=N` to override the worker
count. `each` remains serial unless `--jobs=N` is supplied, because parallel
side effects should be visible in source. Explicit bounded parallel stage
limits must be positive.

When a block uses `?` and an item fails, parallel stages stop scheduling
new work (short-circuit). When a block returns `Result` values without `?`,
errors are just values in the output stream — all items run to completion
and the caller decides how to handle failures. Engine cancellation cancels
all running work immediately through the process cancellation rules.

Pipelines preserve laziness across `where`, `map`, `flat-map`, `tee`,
`enumerate`, `take`, and `drop` when the source is live. Sorting, grouping,
batching, zipping, shuffling, binding a pipeline to `let`, `collect()`, and
explicit `.collect()` materialize. `par-map` is a parallel materialization
boundary, but the runtime may fuse adjacent `par-map |> reduce-by` so
worker-local aggregation avoids building one intermediate list. Suffixes such as
`par-map |> where |> flat-map |> reduce-by` currently materialize between
stages.

## 15. Builder Blocks

Builder blocks are accepted only by APIs whose signatures declare a builder
parameter. They are not general command block literals and are not special
package grammar.

Accepted syntax:

```xsh
let exec = process.command {
  cwd = p"/"
  env = { RUST_LOG: "info" }
  timeout = 30s
  run --timeout=10s /sbin/sshd -D -e
}
```

Inside a builder block:

- `name = expr` is a builder field setter, not mutation of a lexical variable.
- Local scratch bindings still use `let` or `var`.
- Nested DSL commands are builder entries dispatched by the accepting API.
- Expressions may capture outer lexical values.
- Builder field names do not leak as ordinary variables.

Builder checks reports unknown fields, duplicate fields, invalid nested
commands, missing required fields, and domain check failures with source
spans from the builder block.

`process.command { ... }` accepts `cwd: Path`, `env: Record`, `stdin: Path`,
`stdout: Path`, `stderr: Path`, `stdout_append: Bool`,
`stderr_append: Bool`, `timeout: Duration`, `cpu_max: Int`, `detach: Bool`,
`new_session: Bool`, `ignore_hup: Bool`, and exactly one plain `run` entry. It
captures a typed process plan without executing it. `process.command_argv`
builds the same typed plan from data; its `argv` list is the full argv vector
and must include `argv[0]`. XSH resolves `target` as the executable and passes
the remaining argv items as process arguments. `process.run` executes a command
plan, returns completed nonzero exits and signal terminations as `Ok(Status)`,
and returns setup, timeout, or cancellation failures as `Err(ProcessError)`.
`process.spawn` consumes the detach/session/HUP fields. Both `process.run` and
`process.spawn` consume `cpu_max` by applying it to the child process tree when
supported. Plain `run` execution does not consume detach/session/HUP fields.
Pipelines, captures, process streams, redirections, and shell strings are
rejected as command-plan input.

## 16. JSON

The accepted public JSON surface is ordinary JSON only. Public tagged JSON is
deferred.

`json.read`, `json.decode`, `json.write`, `json.encode`, `json.write_lines`,
and `json.encode_lines` operate on JSON-compatible values:

- `Null`
- `Bool`
- `Int` where representable by the JSON implementation
- finite `Float`
- `Str`
- `List`
- string-keyed maps and records whose values are JSON-compatible

Values that JSON cannot represent faithfully require explicit conversion or a
module-owned serialization format:

```xsh
json.write manifest.json ({
  path: dest.display(),
  digest: digest.base64(),
  ok: status.ok,
}) ?
```

`json.read` and `json.decode` decode exactly representable integer JSON numbers
as `Int` and finite non-integer JSON numbers as `Float`.

`Path`, `Bytes`, `Digest`, `Regex`, `Duration`, `Status`, `Result`, `Error`,
`ProcessError`, `ProcessHandle`, builder task metadata, command plans, and
non-finite `Float` values are not implicitly accepted by public JSON APIs.
`ProcessHandle` also cannot be
encoded into cache keys or displayed implicitly as a command argument; scripts
must use explicit metadata fields such as `.pid` or `.argv` when they want
text.

`json.encode` and `json.write` emit ordinary JSON without XSH type tags. Record
keys are emitted in deterministic lexicographic order, and the public format is
stable for the JSON-compatible values listed above. With `pretty: true`, they
emit deterministic indented JSON. `json.encode_lines` and `json.write_lines`
emit one compact JSON value per line, with a trailing newline after each value.

JSON path helpers use explicit segment lists. A path segment is either a `Str`
object key or a non-negative `Int` list index. The empty path addresses the
root value. `json.get(value, [])` returns the root. `json.set(value, [],
replacement)` replaces the root. `json.remove(value, [])` returns `Null`.

Path lookup failures return structured `json-path` errors for the `Result`
forms. The fallback overload of `json.get` returns the fallback for a path
lookup failure. `json.set` updates existing list indexes and updates or inserts
object fields at the target; all intermediate containers must already exist.
List indexes must already exist. `json.remove` errors when the target is
absent.

## 17. Resolver And Checker

The checker resolves names and enforces value-boundary rules. It must reject:

- Unknown keywords or syntax.
- Syntax outside the accepted language scope.
- Duplicate names in the same scope.
- Assignment to `let`.
- Assignment to an undefined name.
- Reassignment to `var` with the wrong type.
- `?` outside a `Result`-returning proc, pure function, task, effect block, or
  top-level context.
- `?` applied to a non-`Result` value.
- Ignored `Result`.
- Unresolved proc command names.
- Command-style proc calls.
- Command-style pure function calls.
- Incorrect proc or pure function arity.
- Incorrect proc or pure function argument types.
- Incorrect proc or pure function return types.
- Non-tail expression statements in value-producing function and task bodies.
- Invalid operator operand types.
- Invalid `if` condition types.
- Invalid `for` iterator types.
- Invalid `@` splice targets.
- Implicit invalid argv conversion.
- Empty list literals without expected type.

The checker may leave explicitly dynamic record field access and host-derived
values to runtime, but every runtime type error must include the source span of
the expression or command argument that caused it.

`Any` is the public dynamic type. Default checking permits `Any` at concrete
boundaries for compatibility. `xsht check --strict` adds migration diagnostics
for assigning, passing, returning, indexing, field-accessing, or container
merging `Any` into concrete types without an explicit `value.require(Schema)?`
boundary. Strict diagnostics are rendered as warnings, but `xsht check --strict`
exits with status `2` when any strict warning is present. Field access on a known
non-empty record schema reports
`check.unknown-field` for missing fields in strict mode; field access on `Any`
or empty `Record` remains dynamic.
The detailed assignability and narrowing rules are specified in
`docs/SPEC-TYPING.md`.

## 18. Tracing And Tracebacks

Tracing is part of the execution contract. It is not an interactive-shell
feature. Trace events are a runtime graph projection: event ids name runtime
nodes, parent ids describe dynamic containment, source spans anchor nodes to the
source tree, and payloads record the process, dataflow, ambient-state, resource,
and failure relationships that matter at execution time.

CLI flags for `xsht trace`:

- `--raw` selects verbose per-event trace output.
- `--trace-format text` selects human-readable trace output.
- `--trace-format jsonl` selects machine-readable JSON Lines trace output.
- `--trace-file PATH` writes trace output to a file instead of stderr.

`xsht trace` without `--trace-format` means `--trace-format text`. `xsht trace`
without `--raw` renders a summary. Use `xsht trace --raw` for the verbose event
stream. The `xsh` command is a plain script runner and rejects public trace
flags with a usage error pointing to `xsht trace`.

Trace output must be separate from script stdout. When no trace file is
specified, trace output goes to stderr with diagnostics. Summary output
includes total event counts, script duration, proc and pure function call
frequency with p50/p75/p90/p99 duration distributions, and the top hot command
operations by total duration. Text summaries should render terminal-width
UTF-8 tables and wrap long cell contents vertically instead of truncating
source spans or names. Raw text trace output renders each trace event as one
physical output line. CLI raw trace output renders live process identifiers and
timings; test fixtures may normalize those unstable fields before comparing
output.

Every trace event has:

- `event_id`.
- `parent_event_id`, or `null` for the root event.
- `depth`.
- `kind`.
- `source_span`, when available.
- `name`, when applicable.
- `start_time` or `duration_ms`, unless normalized by the fixture harness.

Required event kinds:

- `script.enter`.
- `script.exit`.
- `proc.enter`.
- `proc.exit`.
- `pure.enter`.
- `pure.exit`.
- `core.call`.
- `core.result`.
- `module.call`.
- `module.result`.
- `run.start`.
- `run.end`.
- `spawn.start`.
- `spawn.ready`.
- `wait.start`.
- `wait.end`.
- `spawn.cancel`.
- `cwd.enter`.
- `cwd.exit`.
- `signal.received`.
- `signal.hook.enter`.
- `signal.hook.exit`.
- `signal.forward`.
- `signal.escalate`.
- `result.propagate`.
- `runtime.error`.

Additional trace kinds include process pipelines, redirection setup,
environment overlays, cancellation, structured stream stages, parallel jobs,
signal shutdown payloads, and builder checks.

`run.start` events include the executable target, argv items, and cwd context.
Argv items must be represented as an array, never as a reconstructed shell
string. Text trace rendering must quote whitespace, control characters, and
non-printable bytes unambiguously.

`spawn.start` events include target, argv, cwd, env overlay, and detached
policy. `spawn.ready` events include the allocated handle id and live pid when
raw tracing is not normalized. `wait.start`, `wait.end`, and `spawn.cancel`
include handle ids so a trace consumer can correlate a handle from spawn to
wait or cancel. `wait.end` carries either a status payload or a process error;
`spawn.cancel` carries the signal name, kill-after duration, and any process
error. A spawn setup failure before handle allocation may have no
`spawn.ready` event.

Traceback presentation is required for runtime failures and top-level
propagated `Err` values. A traceback includes:

- The failing source span.
- The user proc or pure function call stack.
- The call-site span for each user frame.
- The failing operation kind.
- The structured error kind and message.
- For external process failures, the executable, argv array, cwd context, and
  status or exec failure kind.

## 19. CLI

CLI commands:

- `xsh SCRIPT -- ARGS...`.
- `xsh -- SCRIPT ARGS...` for shebang-compatible script execution.
- `xshi`.
- `xsht --help`.
- `xsht help [COMMAND]`.
- `xsht COMMAND --help`.
- `xsht check [--strict] [--summary] [--annotate] [PATH...]`.
- `xsht fmt [--check] [FILE...]`.
- `xsht lint [--fix] [--runless] [FILE...]`.
- `xsht ast SCRIPT`.
- `xsht trace SCRIPT -- ARGS...`.
- `xsht trace --raw SCRIPT -- ARGS...`.
- `xsht trace --trace-format jsonl SCRIPT -- ARGS...`.
- `xsht trace --trace-file PATH SCRIPT -- ARGS...`.
- `xsht trace --syscalls SCRIPT -- ARGS...`.
- `xsht test [OPTIONS] [FILTER]`.
- `xsht grep PATTERN [FILE...]`.
- `xsht refactor PATTERN REPLACEMENT [FILE...]`.
- `xsht docs build`.
- `xsht docs check`.

Exit codes:

- `0`: success.
- `1`: lint failure for `xsht lint`, format mismatch for `xsht fmt --check`,
  or test failure for `xsht test`.
- `2`: source, lex, parse, resolve, check, or strict compact lowerability
  failure.
- `3`: runtime failure or top-level propagated `Err`.
- `4`: internal implementation error.
- `128 + signal`: tooling interrupted by a handled OS signal such as SIGINT.
Successful script evaluation may instead return any `0..=255` script-selected
status from a final top-level `Int` or `abort`; tool and runtime failures take
precedence over script-selected statuses.

`xsht check` accepts files and directories. With no path, it checks all `.xsh`
files under the current directory, plus configured `include` files or
directories from `xsht-config.ini`. Directory traversal uses the same `exclude`
patterns from `xsht-config.ini` as other path-oriented tooling.

After a program parses, resolves, and type-checks, `xsht check` also verifies
that the entry program can be lowered for the compact runtime. This pass does
not execute user code or perform script effects; it reports lowerability
diagnostics with the same renderer and exit status as other check failures.

`xsht lint` uses the nearest `xsht-config.ini` in each linted file's ancestor
directories. Relative `module_path` entries are resolved from that config
directory. No-argument discovery also adds configured `include` files or
directories from the current `xsht-config.ini`, and discovered files are filtered by
the nearest config's `exclude` patterns. This allows a parent directory lint run
to honor subproject configs as if lint had been run from each configured
subproject.

`xsht fmt` uses `format.line-width` from the nearest `xsht-config.ini` in each
formatted file's ancestor directories. The default line width is `120`. The
configured value must be a positive integer. Line width is a formatter target,
not a hard guarantee: unbreakable strings, paths, comments, and `fmt: skip`
regions may exceed it. The formatter uses the semantic AST for source shape and
the CST for source-faithful trivia. Comments must remain attached to the
construct they document; when the formatter cannot deliberately reattach nested
comments, it preserves the containing statement's raw source.

`xsht check --annotate[=CLASS,...] [PATH...]` runs the normal checker and, only
when source loading, parsing, module loading, and checking produce no
diagnostics, rewrites the requested scripts in place with safe inferred type
annotations and then formats the result. It must not rewrite imported modules.
Bare `--annotate` uses the exact class list in `check.annotate` from
`xsht-config.ini` when present, or the built-in default classes `params`,
`returns`, and `exports`. `params` annotates defaulted proc/pure parameters, `returns`
annotates defaulted exported proc returns, and `exports` annotates exported
simple `let`/`var` bindings. The opt-in `locals` class annotates local simple
bindings with non-trivial types (`List`, `Map`, `Result`, optional, `Command`,
`Pure`, `Proc`, or tag union).
`--annotate=locals` is shorthand for defaults plus `locals`; `--annotate=all`
enables every class. Dynamic, recovery, internal, anonymous-record,
destructuring, discard `_`, local scalar, and local `Unit` binding types are not
annotated.

`xsht lint --fix` applies safe fixes as non-overlapping source replacements
guarded by the CST. A replacement span containing comments is skipped unless the
specific fixer knows how to preserve or reattach them. Rewritten files must
parse, resolve imports, check, and format before they are written.
It removes provably needless local binding annotations and rewrites simple
`.contains(value)` membership or substring checks to `value in receiver` when
the checker proves that `in` has the same semantics and the rewrite does not
move effectful expressions.

During `xsht check`, `reveal_type(expr)` is a checker-only builtin that accepts
one positional argument, reports the inferred type as a note, and has type
`Unit`. Outside `xsht check`, the checker rejects `reveal_type` with
`check.reveal-type`; it is not a runtime API.

Runtime stdout and stderr are not decorated. Diagnostics are written to stderr.

`xsht grep PATTERN [FILE...]` performs AST-aware structural search. The
pattern is an XSH expression where uppercase-only identifiers are
metavariables that match any expression and bind to a name. `ARGS..` binds
zero or more call arguments. Matches are printed as `file:line: source_text`
with bindings shown on subsequent indented lines. When no files are given,
`xsht grep` searches all `.xsh` files under the current directory, plus
configured `include` files or directories from `xsht-config.ini`.

`xsht refactor PATTERN REPLACEMENT [FILE...]` applies a structural rewrite.
Metavariables in the pattern bind to source spans; the replacement is a
template where the same metavariable names are substituted with the captured
source text. When no files are given, it rewrites all `.xsh` files under the
current directory, plus configured `include` files or directories from
`xsht-config.ini`. `--dry-run` prints a diff without modifying files. Without
`--dry-run`, files are rewritten in place. Replacement is not equivalent to a
formatter pass; `xsht fmt --fix` should be run afterward.

Pattern examples:

```
X.len()                  # find all .len method calls
X.push(ITEM)             # find all .push method calls
M.set(K, V)              # find all .set method calls
for NAME in ITER         # find all for loops
```

Structural matching respects expression boundaries. `X.len()` does not match
`x.len_utf8()`. Whitespace and comment differences between pattern and target
are ignored.

## 20. Native Tests

`xsht test` discovers native tests in `tests/**/*.xsh` and
`showcase/tests/**/*.xsh` relative to the current working directory. Missing
test roots mean zero native tests and success. Test IDs are stable cwd-relative
names of the form `tests/file.xsh::test_name` or
`showcase/tests/file.xsh::test_name`.

Test files are module-shaped. The only allowed top-level forms are `use`,
`let`, `type`, `proc`, `pure`, and `export`; top-level commands, mutation, and
control flow are rejected. Top-level imports and constants are initialized
before each test proc runs.

Native tests are top-level `proc test_*` functions returning `Result[Unit]`.
They may accept no parameters or a single `ctx: TestContext` parameter. Each
test runs in a fresh evaluator with fresh stdout and stderr capture, cwd/env
state, mock registry, call log, and temp root.

`TestContext` is `{name: Str, file: Path, temp_root: Path}`. `TestCall` is
`{op: Str, args: Record}`. The standard `test` module provides assertions,
skip/fail helpers, temp path/file/dir helpers, whole-script subprocess helpers,
and v1 host-effect mocks for `dns.*` and `net.*` operations. Assertion failures
return structured test failure errors; skips return structured test skip errors.

`test.run_script(ctx, source, args: List[Str] = [], env: Record = {}, stdin:
Bytes = b"", name: Str = "script.xsh")` writes `source` under the test temp
root, runs it with `xsh`, and returns `{success: Bool, status: Int, stdout:
Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}`. The text fields
are lossy UTF-8 views of the captured byte fields.

`test.run_xsh(ctx, source, xsh_args: List[Str] = [], script_args: List[Str] =
[], env: Record = {}, stdin: Bytes = b"", name: Str = "script.xsh")` is the
same helper with a separate leading `xsh_args` list for flags before the script
path.

`test.run_xsht_trace(ctx, source, trace_args: List[Str] = [], script_args:
List[Str] = [], env: Record = {}, stdin: Bytes = b"", name: Str =
"script.xsh")` writes `source` under the test temp root and runs it through
`xsht trace`. A legacy `--trace` marker in `trace_args` is ignored so migrated
Rust tests can preserve their old argument lists while moving assertions into
native XSH.

Mocked host operations use public op names such as `dns.lookup` and
`net.request`. Matchers are partial records checked against normalized call
arguments. If an operation has mocks and no active mock matches, the API call
returns a structured unmatched-mock error; operations without mocks use real
host behavior.

Cataloged examples are not run by plain `xsht test`. They run through
`xsht test --examples`, or together with native tests through `xsht test --all`.

## 21. Fixtures

Every accepted feature must have at least one parser, checker, runtime, trace,
or example fixture. Every exclusion must have a parser or checker fixture
proving that it is rejected.

Fixtures must be able to assert:

- CLI exit status.
- stdout.
- stderr.
- Human diagnostic text.
- Machine-readable diagnostics.
- Typechecker expected/actual type diagnostics.
- Text trace summaries.
- Raw text traces with normalized pids and durations.
- Raw JSON-lines traces with normalized pids and durations.

Fixtures that depend on host-specific behavior, such as signal numbers,
permissions, or non-UTF-8 paths, must be isolated behind marked tests.
