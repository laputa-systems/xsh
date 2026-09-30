# XSH Specification

This is the living implementation contract for XSH.
`docs/CHAPTER-01-why-xsh.md` supplies philosophical grounding,
and `xsht api` is the canonical query interface for standard-library and
language reference data. When those documents disagree,
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
violations. List concatenation uses `left + right`, and mutable local list
updates use `items += [item]` or `items += more`. The
`lint.prefer-list-compound-assignment` rule recognizes safe local updates;
`push` and `extend` remain useful methods in expression chains.

**Pipelines, not text plumbing.** `|>` is a typed operator. Each stage knows
what flows through it. The pipeline result is a `List[T]`, collected
automatically. There is no word splitting, no IFS, no globbing, no xargs.
The pipeline describes a transformation; the types describe what is being
transformed.

**Results, not exceptions.** Fallibility is part of the signature. `?`
propagates, `??` recovers, `match` handles. Nothing can throw without the
caller seeing `?` in the code. The error kind is a plain string, not a class
hierarchy. Code that looks clean either handles errors or explicitly
propagates them. `error.fail(message)` constructs a `Result[Unit, Error]` with
kind `validation`; it is the explicit spelling for an expected validation
failure and requires the enclosing proc's `error` effect when propagated.

**Greppable by default.** XSH code must be searchable with ordinary text
tools. This means no sigils that change meaning based on context, no implicit
coercions that hide the underlying operation, no overloaded syntax. Every
call to `f(x)` is findable with `grep 'f('`. Every method call `.m(` is
findable without understanding the type. The AST-aware `xsht grep` tool
extends plain grep to handle whitespace and expression boundaries correctly,
but plain grep must also be sufficient for most searches.

**Testable at every layer.** The language has mocks, temp files, and
assertions built in. Tests are checked declarations; the test module is a standard
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
- Comments beginning with `#`, including retained `##!` module docs and `##`
  exported-declaration docs.
- Newline and semicolon statement terminators.
- `use`, `export`, `let`, `var`, `proc`, `pure`, `type`, `return`, `defer`,
  `if`, `else`, `while`, `for`, `break`, `continue`, and `match`.
- Required parameter lists, concrete return inference for private `pure` helpers, default
  `Result[Unit]` returns for annotation-free `proc`, typed defaults for simple
  defaulted parameters, plus default and rest parameters.
- Expression-style pure and proc calls, plus fully qualified standard-module
  command calls for effectful APIs returning `Result[Unit]`.
- `Ok(value)`, `Err(value)`, `Result`, `error.fail(message)`, postfix `?`,
  right-associative `??` fallback, `Result.context(...)`, and implicit
  `Ok(value)` wrapping for `Result[T]` tail values.
- User-defined tag union types with exhaustiveness checking: `enum Level { Info, Warn, Fault(Str) }`.
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
  integration tests, and source coverage reports through `xsht test --cov`.
  API coverage is an explicit `xsht test --cov --api` report section.
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
  language registry and queried with `xsht api`.
- Text and JSON-lines traces, method trace events, tracebacks, `xsh`, `xshi`,
  and `xsht`.

The following remain outside v1 unless this spec later promotes them:
shell-string process execution, first-class command block literals,
block-valued named arguments, public tagged JSON,
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

`##!` starts a module doc block and `##` starts an exported-declaration doc
block. Consecutive lines with the same prefix form one retained documentation
span. A module with any `export` must have one `##!` block before executable
module content, and every `export` must have a contiguous `##` block immediately
before it. `##` blocks that do not attach to an export and additional `##!`
blocks are checker errors. Ordinary `#` comments remain non-semantic.

Reserved keywords:

```text
and assert break const continue defer else enum false for if in let match not null or proc pure
retry return run spawn stream test true try type use var wait while yield
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
language-facing registry queried by `xsht api`. Those names are
reserved in simple binding and alias positions so module namespaces cannot be
shadowed or aliased by ordinary declarations. Record destructuring may bind
fields with those names because standard record schemas commonly contain fields
such as `path`.
`args` is a special case: it is also the predeclared script argument value, so
ordinary bindings named `args` are allowed in nested scopes; the root binding
remains the script argument value. This also applies inside imported functions.
Qualified `cli.parse(...)` still resolves to
the standard module.
`error` is another compatibility exception: `error` is the conventional payload
name in `Err(error)` bindings, and the `error.fail(...)` validation operation is
addressed through its qualified module name. Local bindings named `error` are
therefore legal in all ordinary binding positions; an unshadowed `error.fail`
call continues to resolve to the standard module and requires the `error` effect.
Other standard module names remain reserved.

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
Ordinary, raw, and formatted triple `Str` literals use block layout only when
an opening delimiter is immediately followed by LF, CRLF, or CR and the closing
delimiter is alone on its line apart from spaces/tabs. Trailing syntax or a
comment on the closing line makes the literal an exact non-block form. The
closing delimiter's exact space/tab prefix is the margin. Remove the opening
and closing structural line breaks, counting a shared break only once in an
empty block. Remove that prefix from every nonblank source content line;
a missing or different prefix is a source error. Whitespace-only content lines
lose only the longest matching initial part of the margin. Internal line endings
and remaining whitespace retain their exact bytes. There is no implicit trailing
newline; an extra blank content line expresses one.

Layout is removed before escape decoding and interpolation. Only literal text
participates: interpolation code, nested literals, original expression spans,
and multiline inserted values are not trimmed or reindented. Rawness retains
its existing escape/interpolation behavior. Bytes, Path, formatted Path, glob,
and regex literals retain exact layout. Triple literals outside the block shape
retain exact behavior; an explicit escaped opening break can preserve an old
leading newline and indentation without opting into block layout.
`lint.redundant-newline-triple-string` converts an exact one-newline block
(`"""` followed by three LF breaks and `"""`) to `"\n"`.
`lint.prefer-block-string` rewrites constant escaped-newline concatenations only
when a normally parsed candidate has the identical decoded `Str`. Dynamic or
formatted operands, comments, CR-containing values, and expression positions
where the closing delimiter cannot be alone receive no automatic rewrite.

Expression string literals do not interpolate. `${expr}` interpolation is
recognized only in command words and quoted command word parts. `$name` and
`$record.field` are accepted shorthand there for simple binding or field-access
chains. Arbitrary expressions still require `${expr}`. In an expression
string, a `$name` lookalike is plain text, not interpolation;
`lint.dollar-in-expression-string` warns when such text names an in-scope
binding. Interpolate with a display string (`f"..."`) or concatenation (`+`),
and keep a literal dollar sign with a raw string (`r"..."`) or the `\$`
escape.

Bytes literals are `b"..."` and support the same byte escapes except
`\u{HEX}`. They produce `Bytes`.

Regex literals are `rx"..."` or `rx"""..."""` and produce `Regex`.
They use raw-string delimiters: backslashes, dollar signs, and inline flags
are passed unchanged to the existing regex engine, with no escape decoding or
interpolation. Each occurrence is validated and compiled during checked
program/module preparation, including unreachable expressions. Invalid syntax
is a source-located preparation error; `xsht check` reports it without running
the script. Prepared engines are shared by repeated evaluations of the owning
program. Use `regex.compile(pattern)` for runtime strings and Result-valued
error handling.

Path literals are `p"..."` and support the same escapes as string literals.
They produce `Path` and do not interpolate. An unescaped `${...}` in a p-string
is rejected; use `fp"..."` when the path should interpolate.

Formatted path literals are `fp"..."` or `fp"""..."""` and support `${expr}`
interpolation. Path interpolands retain native bytes; other displayable values
use their established conversion encoded as UTF-8. They produce `Path`.

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

`UInt` is the non-negative integer type. Typed mutable storage preserves this
constraint through aliases, record fields, List elements, and Map values.
Assignment evaluates selectors and the RHS in the ordinary order, then validates
the replacement (including the result of compound arithmetic) before storing it.
A negative replacement fails with `type-error`; the selected storage and aliases
remain unchanged, while effects already performed by selectors or the RHS remain.
Replacing a container or appending elements validates its nested UInt constraints.
Checked collection creation, branch/fallback results, builtin results, and inferred
bindings retain this domain too. Direct collection inputs to loops and native
stages cannot carry negative elements under a UInt item type.
Nominal tag/error payloads and functional List/Map method operands also preserve
their declared constraints before a new value is published. All authored call
operands evaluate once in source order before call-level domain validation.
Ordinary call arguments, prepared defaults, and returned values preserve the same
constraint, including nested containers and implicit tails. These domain failures
are runtime type failures and run ordinary cleanup; `try` does not convert them
into Result data. Use `value.require(UInt)?` for recoverable validation.
Declared Stream item constraints are checked as each item is produced, including
items crossing retained delegation boundaries. No later item is pulled to validate
the current one; rejection stops delegated children before their parents and runs
registered cleanup once. Cancellation skips unreached invalid items.

Float literals require a decimal point followed by at least one digit or an
exponent: `1.0`, `0.25`, `10e-3`, and `1.5e6`. They produce `Float` values.
The runtime representation is IEEE 754 binary64.

Duration literals are decimal integers followed immediately by `ms`, `s`, `m`,
or `h`. They produce `Duration` values.

Display strings use `f"..."` or `f"""..."""` and support `$name` shorthand
and `${expr}` interpolation with display conversion. Shorthand names may use
field access, such as `$entry.name`. Interpolation scans balanced braces,
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

The compact runtime dispatches script arguments into an auto-invoked `main`
positionally and can collect a tail only into a spread (rest) parameter.
`main` should therefore declare script arguments through a spread parameter,
e.g. `proc main(...argv: List[Str])`. A `main` whose fixed (non-defaulted)
parameter is not a CLI scalar (`Str` or `Path`) can never bind a script
argument and cannot run under `xsh`, so `xsht check` reports a
`compact.main-missing-spread` diagnostic at check time naming the spread form
instead of letting the script fail only when it is run. An empty `main()`,
fixed scalar or defaulted parameters, and a `main` that also declares a spread
parameter remain valid.

`cli main(parameters) [effects] -> Return { ... }` declares a script entry,
with the ordinary proc effect/return rules and default `Result[Unit]` return.
It is neither callable nor exportable. Exactly one may occur at the entry
module's top level, and it cannot coexist with another `main` declaration.
Required scalar parameters are positional in declaration order; parameters with
prepared constant defaults are named options, and a final rest `List[Str]` or
`List[Path]` receives remaining operands. Required positionals precede options.
The supported scalar parsers are `Str`, `Int`, `UInt`, `Bool`, `Path`, and
`Duration`; aliases retain their resolved parser, including unsigned validation.
Omitted defaulted annotations use the checked semantic parameter type for parser
selection. Inferring that type never permits execution of an ordinary runtime
default: CLI defaults must still be prepared constants. Snake case option names
map to kebab case.

The entry derives the existing strict CLI schema and parser. Boolean options
accept bare switches and explicit Boolean values; other options accept attached
or following values. Defaulted Lists append repeated occurrences to their
prepared default, preserving the existing repeated-option policy. `--` preserves
remaining operand order, duplicate scalar options and unknown options are errors,
and `-h`/`--help` are reserved. Path conversion performs no existence check.
Help includes parameter types/defaults and applicable declaration/module docs.

Static validation and argument parsing complete before any executable entry or
imported initializer runs. Help exits successfully, and invalid arguments use
the existing usage-error status without executing initializers or the body.
Reading defaults executes no source code. `cli.parse`, `cli.parse_full`, and
`cli.commands` remain the explicit APIs for advanced policies and dynamic schemas.
`lint.prefer-signature-cli` fixes only literal defaulted scalar option schemas with exact
generated help, typed direct bindings, and no executable initialization or imports.
Comments retain the diagnostic without a fix. Positional descriptors, short aliases,
custom help, computed defaults, and advanced parser policies remain explicit.

Script arguments after `--` are available through the predeclared immutable
binding `args: List[Str]`. The former predeclared `ARGV` binding is rejected
with a removed-vocabulary diagnostic; an explicitly declared user binding with
that spelling remains valid.
The `xsh`, `xshi`, and `xsht` command-line parsers reject an argument that is
not valid UTF-8 with exit status 2 and an argument-index diagnostic, before
loading a script or command. XSH process calls can still pass native `Path`
bytes to external programs.

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
             | const_stmt
             | let_stmt
             | var_stmt
             | assign_stmt
             | proc_def
             | pure_def
             | stream_def
             | type_def
             | enum_def
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
             | assert_stmt
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
export_stmt  = "export" (const_stmt | let_stmt | proc_def | pure_def | stream_def | type_def | enum_def) ;

const_stmt   = "const" IDENT (":" type_expr)? "=" expression terminator ;
let_stmt     = "let" binding_target type_ann? "=" expr_or_run terminator ;
var_stmt     = "var" binding_target type_ann? "=" expr_or_run terminator ;
binding_target = IDENT | record_binding_target ;
record_binding_target = "{"
                 (destructure_field ("," destructure_field)* ","?)? "}" ;
destructure_field = FIELD_LABEL ":" binding_target | IDENT | ".." ;
assign_stmt  = assign_target assign_op expr_or_run terminator ;
assign_target = IDENT ("." FIELD_LABEL | "[" expr "]")* ;
assign_op    = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;
type_def     = "type" IDENT "=" type_body terminator ;
enum_def     = "enum" IDENT (":" "Str")? "{" enum_variant ("," enum_variant)* ","? "}" terminator ;
enum_variant = IDENT ("(" (type_expr ("," type_expr)* ","?)? ")")? ("=" expr)? ;
type_body    = type_expr | record_schema | module_contract ;
record_schema = "{" schema_field ("," schema_field)* ","? "}" ;
schema_field = FIELD_LABEL ":" type_expr ("=" expr)? ;

`FIELD_LABEL` is an identifier or keyword token used as a label, rather than a
lexical declaration. Field labels keep their exact spelling; quoted string
keys remain available in record literals for arbitrary text.
module_contract = "module" "{" module_contract_entry* "}" ;
module_contract_entry = "export" "optional"? module_contract_kind terminator? ","? ;
module_contract_kind = ("let")? IDENT ":" type_expr
             | "proc" IDENT param_list effect_ann? "->" type_expr
             | "pure" IDENT param_list "->" type_expr ;
type_ann     = ":" type_expr ;
defer_stmt   = "defer" (block | expr_or_run) terminator ;
yield_stmt   = "yield" (expr_or_run | "@" expr) (("when" | "unless") expr)? terminator ;
```

Module path segments accept hyphenated identifiers (proc-ident form) in addition
to ordinary identifiers. When the final module-path segment contains a hyphen, an
explicit `as` alias is required because the hyphenated form is not a valid
binding name in expression context. The checker rejects hyphenated-final-segment
`use` declarations without `as` with `check.hyphenated-module-alias`.

`let` bindings are immutable. `var` bindings are mutable. Assigning to a `let`
binding or an undefined name is a checker error. Assignment targets may name a
mutable local binding directly, a field below a mutable local record, or a
string-keyed entry below a mutable local map, or an element below a mutable
local List. Paths may mix existing record fields, Map keys, and List indices.
List writes use ordinary indexing: indices are Int and must be nonnegative
and less than the current length. They neither clip nor append, pad, replace
slices, mutate Str/Bytes, or write through temporary receivers.
Field and indexed assignment update the local value stored in the root binding;
prior bindings and aliases retain their contents. Compound assignment requires a mutable target and
follows the corresponding binary operator type rules for the target value.
`List[T] + List[T]` concatenates values in encounter order; `+=` appends a
list to a mutable target. A scalar append is written `items += [item]`.
Both operands use ordinary element compatibility, including expected types
for empty lists; concatenation does not implicitly widen heterogeneous lists.
Within a function, an unannotated empty List has one unresolved element type.
An explicit empty Map constructor has unresolved key and value types. Checked
writes and independently determined parameter or return expectations solve
these types across all branches and loop bodies, including loops that may run
zero times. A mutable binding initialized with `null` similarly has one fixed
optional type once non-null contributions determine its inner type. Immutable
`let value = null` remains Null. An unannotated `{}` remains a record.

Local aliases share the same unresolved type; each use cannot choose a new
instantiation. Incompatible concrete contributions are errors and never widen
to Any or a union. Material unresolved types require an annotation before a
concrete operation, checked signature, or indexed execution contract is
published. A wholly discarded inert literal needs no artificial annotation.
Checking uses source constraints without evaluating code or inspecting runtime
values. Diagnostics identify the initializer and incompatible contributions.
Assignment evaluates target selectors once in path order, then its right side
once, then reads the current root and selected old value and commits the update.
Changes selectors or RHS make to the same root remain visible; unrelated updates
are preserved. Bounds, required intermediate fields, and fallible arithmetic
are validated before rebuilding ancestors. Failed updates expose no partial
ancestor rebuild; already executed operand effects remain visible to cleanup.
Replacement values retain contextual element/schema typing. Assignment produces
Unit. Earlier aliases retain their contents, including self-concatenation.
`lint.prefer-list-element-assignment` recognizes exact prefix/replacement/suffix
reconstructions. It fixes only checked compatible types, proven current bounds,
stable replacement expressions, and preserved comments; clipping slices with
unproved lengths require manual review.
`List.push`/`List.extend` and `Map.set`/`Map.remove`/`Map.push` return updated
values; earlier bindings and aliases keep their previous contents.
Record destructuring selects required fields in source order. A field may bind
its own name, rename it (`{target: target_name}`), or select a nested record
(`{build: {jobs, ..}, ..}`). `..` marks ignored remaining fields without capturing
them. `_` discards a selected value; duplicate bound names and duplicate field
selections are errors. The source is evaluated once and every required field is
selected successfully before any binding becomes visible. Known record schemas
check nested field names and preserve their individual types; `Any` requires an
explicit schema check before destructuring, including annotated targets.

These targets are accepted by `let`, `var`, `for`, list/map comprehensions, and
guard-let. Destructured `let` and iteration bindings are immutable; destructured
`var` bindings are independent mutable local values with ordinary value semantics.
`export let` accepts simple names only. Function parameters do not destructure.

Control flow:

```ebnf
while_stmt   = "while" condition block ;
for_stmt     = "for" binding_target "in" expr block ;
match_stmt   = "match" expr "{" match_arm* "}" ;
match_arm    = pattern guard? "=>" (statement | block) ","? ;
guard        = "if" expr ;
pattern      = alias_pattern ("|" alias_pattern)* ;
alias_pattern = primary_pattern ("as" IDENT)* ;
primary_pattern = "_" | IDENT | type_pattern | literal | constructor_pattern
                | record_pattern | list_pattern | "(" pattern ")" ;
type_pattern = ("_" | IDENT) "is" type ;
constructor_pattern = IDENT "(" pattern? ")" ;
record_pattern = "{" record_pattern_field ("," record_pattern_field)* ","? "}" ;
record_pattern_field = FIELD_LABEL ":" pattern | IDENT | ".." ;
list_pattern = "[" (pattern ("," pattern)* ("," list_rest)? | list_rest)? ","? "]" ;
list_rest = ".." IDENT? ;
```

List patterns match exact lengths (`[]`, `[first, second]`) or a prefix with
one trailing rest (`[head, ..tail]`, `["build", target, ..]`). Elements may
contain nested list, record, constructor, literal, or dynamic type patterns.
Only List subjects and explicit dynamic pattern boundaries are accepted;
Streams, Str, Bytes, and wrapped Result values are not coerced. Known List[T]
elements retain T and named rest bindings have List[T]. Ordinary `let` targets
do not accept refutable list patterns. Nonbinding `is` rejects names inside
both elements and rest.

Length is checked before any element access. Mismatch continues to the next
arm without publishing captures. Captures become visible only after the full
nested pattern succeeds; a named rest preserves list value semantics and is
copied only for successful matches. Exhaustiveness recognizes catchalls,
`[..]`/`[..tail]`, and the partition `[]` plus `[_, ..]` for known List subjects.
Guards and literal element tests cannot establish general coverage; uncertain
value matches require a catchall.

`break` and `continue` affect the nearest `while` or `for`. They are checker
errors inside structured stream stage blocks.

`defer` registers a block-scoped cleanup expression, run command, or statement
block. Registration does not execute the action. Actions run in last-in-first-out
order when control leaves their registering block through success, `Err`
propagation, runtime failure, `return`, loop control, or cancellation. Forced
abort skips cleanup.

A deferred block resolves captures in its registration scope and reads their
values at cleanup time. Use an immutable `let` snapshot to retain an earlier
value. Its locals remain local to the cleanup action. All statements, including
the final statement, use Unit-compatible statement position: Bool values assert
and Result[Unit] failures propagate. A failing action stops its remaining
statements; other registered actions still run. The original failure remains
primary; otherwise the first cleanup failure becomes primary. Subsequent
cleanup failures are reported with their source locations.

Deferred blocks cannot return or yield from their enclosing callable, or break
or continue an enclosing loop. Loops and nested defers declared inside the
cleanup body retain their local targets and cleanup order. Normal effect checks
apply even when registration is in an unselected branch. Expression-form
`defer` keeps its existing type and registration behavior.

`ctx description { ... }` evaluates one `Str` description on entry and adds
an error-context frame with kind `ctx` when a failure propagates out of the
lexical body. The frame retains the region's source span. The body follows the
ordinary statement or value-block contract, and return/break/continue keep
their enclosing destinations. Context entry has no host effect; description
and body expressions retain their normal checked effects.

Region defers and owned-resource cleanup finish before its outbound failure is
annotated. A failed description receives only already enclosing contexts.
Nested failures acquire one frame per crossed region, from inner to outer;
nominal error identity, payload, primary source location, and prior contexts
remain intact. Cleanup failures use the existing primary/secondary failure
policy. Abort and cancellation retain their control behavior.

Handled failures and `Err` values stored or returned as data are unchanged;
use `Result.context` to annotate explicit error data. Context annotation owns
its error value and never changes another alias's context chain. The contextual
introducer is recognized only when `ctx` is followed by a description and a
body; ordinary `ctx` bindings, parameters, field access, and calls remain legal.
Moving explicit `.context(...)` calls into a region changes which failures are
annotated unless their boundaries and description evaluation timing coincide;
such a migration requires review rather than a general automatic rewrite.


Standard modules are built-in namespaces and cannot be aliased. User modules are
imported from sibling `.xsh` files relative to the importing source file, then
from each directory in `XSH_MODULE_PATH` when the file-relative path does not
exist. `XSH_MODULE_PATH` uses the host platform's path-list separator. User
modules may export `let`, `proc`, `pure`, `stream`, `type`, and `error`
declarations.
Non-exported module bindings, types, and error families remain local to the
imported module. `use helper` binds exactly one namespace, `helper`; `use
helper as h` binds exactly `h`. Exported values, procedures, pure functions,
streams, types, tag constructors, error families, error variants, and error
facets are accessed through that namespace, such as `helper.value`,
`helper.call(...)`, `helper.TypeName`, `helper.TagName(...)`, and
`helper.ErrorName.Variant(...)`; imports never inject exported names into the
importing scope. Types and error families are
compile-time names and are not fields in the runtime module record. Imported
modules may contain
top-level `use`, `let`, `proc`, `pure`, `stream`, `type`, and `error`
declarations, but not top-level mutation, commands, or control flow.
Different resolved module files remain separate even when they share a basename.

An exported named type is resolved in its declaring module before import. Its
record fields may therefore refer to other types local to that module, and an
importing module may embed the exported type in another record or pass it to an
effectful exported procedure without rebinding those field annotations in the
importer's scope. For example, `type Context = {target: policy.Target}` keeps
the complete `policy.Target` schema when `Context` is imported elsewhere.

Surface-only conveniences are lowered before checking and evaluation. The core
lowering includes path literals and p-strings, typed environment fields to an
environment lookup node, implicit value pipeline calls to ordinary calls with
the pipeline input inserted, and builder syntax to module-owned builder calls.
Explicit pipeline holes retain the input and ordinary call through checking,
then bind that input to hygienic temporary storage before the call.
Formatting preserves the readable surface form.

### Prepared immutable data

`const` declarations are checked data in the program, including exported module
values and local constants. Their finite lexical dependency graph may reference
other constants, including qualified exported constants; cycles are preparation
errors. Runtime bindings, parameters, ambient names, arbitrary calls or methods,
blocks, comprehensions, retry, and propagation are rejected. Record and tag
constructors may consume prepared constant arguments and schema defaults. Named
constructor spreads require prepared closed records and use only their statically
visible fields; Optional, Map, runtime, and erased sources are rejected. Explicit
and spread fields retain the ordinary duplicate and field-type checks. Closed
record projections read only prepared data and retain the declared field type;
a present prepared value does not erase an Optional annotation.

The bounded data subset includes scalar, path, and regex literals, homogeneous
constant containers, and checked primitive operations. Invalid integer arithmetic
is diagnosed during preparation. Empty List and Map values need concrete type
context; `Any` and resource values cannot enter constant data. Preparation admits
at most 128 nested expressions, 100,000 analysis steps, and 1,048,576 data units
per constant (container entries and literal bytes contribute to this bound).
Paths preserve their bytes without consulting cwd. Regex literals share their
already prepared pattern. Indexed reads reuse immutable container backing under
ordinary value semantics, so writes to a derived `var` preserve the constant.

An exported constant satisfies the ordinary read-only module value contract.
`let` retains runtime initialization. `lint.prefer-const` offers a keyword-only
edit for inert module literal data; runtime calculations and local bindings keep
their initialization boundary.

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
call signatures. Static module namespaces have inferred structural signatures
and may satisfy these contracts directly. A concrete static module with no
runtime exports is an empty module, not an unknown module, so it cannot satisfy
a nonempty contract. A runtime-loaded module has a separate internal dynamic
module representation and is checked against the contract when `.require(T)`
executes.

`optional` permits an export to be absent. When an optional export is present,
its kind and full signature must still match. Extra actual exports are allowed.
Value types, parameter lists, parameter types, and return types are invariant.
An implementation may require only effects permitted by the contract; the
current implementation uses exact declared-effect equality.

Streams are compile-time namespace members, not first-class module-contract
members. `module { ... }` contracts therefore reject stream entries. A
stream-only static module still has a concrete empty runtime signature and
cannot satisfy a nonempty runtime contract. Qualified stream dispatch remains
available as `helper.stream_name(...)` without a bare fallback.

```xsh
type BuildPlugin = module {
  export let name: Str
  export optional let description: Str
  export proc build(root: Path) [fs, process, error] -> Result[Unit]
  export pure label(name: Str) -> Str
}
```

**Tag unions** use `enum T { A, B, C(Type, ...) }`. An enum requires at least
one variant and accepts multiline bodies and a trailing comma:

```xsh
enum Level { Info, Warn, Fault, Debug }
enum Token { Present(Str) }
```

`type Alias = Token` remains a type alias. The former `type T = A | B`
declaration is a migration error (`parse.enum-migration`) and cannot execute.
`export enum` exports the nominal type and its constructors; constructors stay
in the declaration module namespace, rather than under the enum name.

Each variant is a constructor. Zero-field variants are bare names; non-zero
variants are called as functions: `Info`, `Stopped("disk full")`. Tag union
values are matched with constructor patterns. When the matched value is a
`Tag(T)` type and no arm is a wildcard or binding, the checker emits
`check.non-exhaustive-match` for uncovered variants.

`enum State: Str { Seen = "seen", Missing = "" }` declares a payload-free
nominal enum with explicit wire strings. Every variant supplies a unique bounded
constant Str; empty strings are allowed. Ordinary assignment never converts Str
into an enum. Explicit `.require(Schema)` converts exact wire strings in enum
slots recursively through records, lists, optional slots, and supported Map
values, publishing a trusted value only after the complete schema succeeds.
Unknown strings and mistyped values produce field/index-aware schema errors;
missing fields are never filled from defaults. Type patterns test actual enum
values without conversion. JSON encoding and writing emit declared strings,
including nested occurrences; raw JSON decoding continues to return untyped data.
Ordinary enums remain incompatible with JSON. This boundary does not widen
CLI, environment, argv, or non-Str Map-key conversion.

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

Builtin collection contracts preserve their checked receiver parameters through
arguments and results, including nested containers and nominal Result errors.
Each call instantiates its own internal signature parameters; this does not add
generic functions or expression-level type arguments. Concrete operands must
fit the established collection type. An erased Any element remains dynamic;
inserting one known value cannot establish the type of older elements.
Overloads are selected from receiver and argument contracts, never from a
desired return type alone. Independently grounded expectations can constrain
the selected signature under the same local inference rules as other calls.
Named arguments and static record spreads use ordinary parameter binding and
preserve written evaluation order. `List.join` requires `List[Str]`; it does
not apply display conversion to non-string elements.

`Str` is valid UTF-8 text. `Bytes` is arbitrary byte data. `Path` stores native
Unix path bytes and cannot contain NUL; it can represent paths that are not
valid UTF-8. Formatted Path literals and compound process words append Path
fragments as native bytes, evaluating fragments once in source order. Ordinary
f-strings and print remain display text; an explicit `.display()` produces
UTF-8 text and cannot recover the original native bytes. Concatenation does not
join, normalize, expand, glob, or confine paths: separators and `..` remain
exactly present, and rooted filesystem APIs retain confinement ownership.
NUL remains invalid in paths and argv; interpolation does not split words.

`Map[K, V]` is a deterministic ordered collection with homogeneous scalar keys
and values of type `V`. `Map[V]` is shorthand for `Map[Str, V]`. Key types resolve
to `Str`, `Int`, `UInt`, `Bool`, `Bytes`, `Path`, or
`Duration`; `Any`, `Float`, records, collections, and handles are excluded.
UInt retains a nonnegative semantic constraint through aliases; its runtime keys
use the same Int representation, with checked literal, lookup, update, and schema
boundaries rejecting negative values. Numeric and duration keys order numerically, Boolean keys order false before
true, strings use their ordinary order, and Bytes and Path keys use native byte
identity and lexicographic byte order. Keys are never implicitly displayed or
converted between domains.

`Float` is an IEEE 754 binary64 scalar for measured quantities such as rates,
percentages, load averages, and JSON metrics. Float equality is exact over the
runtime value's binary representation. Display conversion renders finite values
as decimal text and renders non-finite values as `NaN`, `Infinity`, or
`-Infinity`. Sort keys use the IEEE total order, so `NaN` values have stable
ordering. Public JSON encoding rejects non-finite `Float` values.

`Duration` stores nonnegative unsigned 64-bit milliseconds and is accepted by
timeout policy. `Duration + Duration` and `Duration - Duration` produce a
`Duration`; multiplication by a nonnegative `Int` works in either operand order.
Division by a positive `Int` produces a `Duration`, discarding sub-millisecond
remainders; division by a positive `Duration` produces an `Int` interval count.
Duration ordering compares milliseconds. Operands evaluate once, left to right;
these operations are pure and require no time effect. Addition and scaling
report `duration-overflow`, subtraction below zero reports `duration-underflow`,
negative multipliers report `duration-negative-factor`, nonpositive divisors
report `division-by-zero`, and interval counts outside `Int` report
`integer-overflow`, each at the operator expression. Float scaling, modulo,
implicit numeric conversions, and timestamp arithmetic are invalid.
`time.millis` and `time.seconds` retain their clamping and saturation conversion
behavior, which is distinct from checked arithmetic. Compound `+=`, `-=`, `*=`,
and `/=` preserve these rules when their result remains a Duration.

`Digest` is a module-owned typed hash
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
`FsError.NotFound(file: target)`. Imported families use their checked module
namespace or import alias; named payload expressions evaluate once in written
order without evaluating a runtime family receiver. Error values expose `.message`. Exact variant
payload fields are available after exact variant matching, and facets are
matched with `is Facet`. Family and variant labels may be rendered in
diagnostics, but source programs must not branch on string error kinds.
Facet tests preserve a known error family or variant on an immutable subject.
A dynamic subject narrowed to a facet retains the common `Error` interface,
including `.message`, but gains no variant payload fields.

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

- `p"literal"` produces a `Path` from UTF-8 source text. `fp"${value}"`
  preserves native bytes of Path fragments and UTF-8 encodes other fragments.
- `Path(str) -> Path` remains a direct cast from text for values that are
  already known to be valid path text. `xsht lint` may recommend path-string
  syntax for this spelling, but the recommendation is advisory because the
  direct cast is a documented typed-`Path` boundary.
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

Named user record schemas support static construction with named fields,
including puns and qualified imported schema names: `BuildOptions(root:)`.
Aliases resolve to the defining schema's constructor and defaults. Constructors
accept no positional arguments; unknown, duplicate, and missing required fields
are errors. Supplied values evaluate once in source order. A schema name is not
a first-class callable value, and callable/type name collisions are errors.

User record schemas and aliases may declare type parameters, for example
`type Observation[T] = {value: T?, samples: List[T]}` and
`type CountObservation = Observation[Int]`. Arguments must be fully supplied;
qualified applications such as `model.Observation[Int]` resolve private schema
dependencies in the declaring module. Duplicate and reserved parameter names,
wrong arity, unknown types, and recursive or expanding applications are errors.
Substitution produces an ordinary concrete record schema with existing
assignability rules. Generic functions, error families, enums, and module
contracts are not supported.

Use `let value: Observation[Int] = {...}` for a direct application or the
existing `CountObservation(...)` constructor for a concrete named alias.
`Observation(value: 12, samples: [])` infers `Int` from supplied fields. Each
constructor occurrence has fresh monomorphic parameters; repeated evidence
must agree without numeric widening or a guessed `Any`. Non-null values for
`T?` constrain `T`; null and empty containers do not select it. A concrete
annotation, parameter slot, or return contract can supply the instance,
including parameters absent from fields. Nested constructors share their
surrounding field expectations until all fields have contributed. An unresolved
parameter requires an annotation or concrete field evidence.

Named spreads, puns, defaults, and field evaluation order retain ordinary
constructor semantics. Inference does not validate untyped external data or
fill missing fields during `.require`. Expression-level type argument syntax
is not supported. Defaults on a parameterized declaration must work for every
substitution: `null` for `T?` and `[]` for `List[T]` are valid; `1` for `T` is
not. Concrete aliases retain schema defaults and validation, including
`.require(CountObservation)`.

Schema field defaults are bounded immutable constants: `null`, `Bool`, `Int`,
`Float`, `Duration`, `Str`, `Bytes`, and `Path` literals, signed numeric literals,
recursively literal lists and records, and references to previous immutable
constants composed from those forms. Constants resolve in the
schema declaration's lexical module. Calls, ambient state, mutable captures,
field dependencies, and propagation are forbidden. Defaults are checked once
against field types, including contextual empty containers; each constructed
value retains independent value semantics under mutation.

Defaults apply exclusively to explicit constructor calls. They neither make
schema fields optional nor fill missing fields in record literals, JSON, or
`.require(Schema)`. `lint.prefer-record-constructor` rewrites proven schema-typed
record initializers while retaining annotations and field evaluation order. It
omits an explicit default only when the value is the identical bounded constant;
comments and uncertain conversions retain the original spelling.

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
postfix_op   = "." FIELD_LABEL | "." "require" "(" type_expr ")" | "?." FIELD_LABEL
             | "[" expr "]" | "[" expr? ".." expr? "]" | call_args | "?" ;
call_args    = "(" arg_list? ")" ;
arg_list     = arg ("," arg)* ","? ;
arg          = expr | named_arg | "..." expr | "@" expr ;
named_arg    = FIELD_LABEL ":" expr | IDENT ":" ;
primary      = literal | IDENT | list_lit | record_lit | map_comp | if_expr | match_expr
             | capture_expr | retry_expr | context_expr | context_scope_expr | run_form | spawn_form | wait_form | "(" expr ")" ;
spawn_form   = "spawn" (run_form | expr) ;
wait_form    = "wait" expr ;
if_expr      = "if" condition "{" expr "}" ("else" "if" condition "{" expr "}")*
               "else" "{" expr "}" ;
match_expr   = "match" expr "{" match_expr_arm* "}" ;
match_expr_arm = pattern guard? "=>" expr ","? ;
capture_expr = "try" block ;
context_scope_expr = ("cd" | "env") "(" expr ")" block ;
retry_expr   = "retry" "[" (expr ("," expr)* ","?)? "]" ("on" "(" pattern ")")? block ;
context_expr = "ctx" expr block ;
```

In expression-call argument lists, `name:` followed by a comma or closing
parenthesis is shorthand for `name: name`. Whitespace, comments, and newlines
may separate the colon and delimiter, including trailing commas in multiline
calls. The
implied identifier resolves in the caller's lexical scope at the written name;
missing names receive the ordinary name-resolution error on that name. Punning
preserves named-argument ordering, arity, defaults, duplicates, overloads,
effects, and source-order evaluation. It adds no command-argument syntax or
record-field lookup. Explicit named arguments and positional arguments can be
mixed with puns wherever the ordinary call rules permit them.

Formatting retains `name:`. `lint.prefer-named-argument-pun` replaces a checked
`name: name` whose value resolves as the same lexical identifier, and retains
comments by withholding fixes that would remove them.

`...record_expression` in an expression call supplies named arguments from
exactly the fields visible in its checked finite `Record` type. Runtime fields
hidden by a checked schema boundary are excluded. `Any`, open `Record`, `Map`,
`Optional`, and `Result` operands require an explicit checked narrowing or
unwrapping first. The callee must have a statically checked callable signature.
Multiple disjoint spreads can mix with positional, explicit named, and punned
arguments. Unknown names and duplicate names, including parameters already
occupied by positional arguments, are errors. A null field is supplied and does
not select the parameter's default. Overload selection, rest parameters,
parameter types, omitted defaults, and effects retain their ordinary contracts.

The receiver is evaluated before argument entries. Entries evaluate once in
written order; a spread evaluates its operand once and projects its visible
fields before the following entry. Static parameter slots retain this order
even when named arguments are written in a different parameter order. `@list`
remains positional rest splicing and does not supply names.

`lint.prefer-named-argument-spread` recognizes contiguous forwarding of every
visible field from one stable immutable binding. It withholds fixes for partial
coverage, extra visible fields, mutable or effectful receivers, comments, type
conversions, and candidates that fail parsing or checking. Formatting preserves
spreads and the fixed form converges.

Literals:

```ebnf
literal      = "null" | "true" | "false" | INT | FLOAT | DURATION
             | STRING | FMT_STRING | BYTES | REGEX | PATH | PATH_FMT | GLOB ;
list_lit     = "[" list_body "]" ;
list_body    = (expr ("," expr)* ","?)?
             | expr comp_qualifiers ;
record_lit   = "{" (record_field ("," record_field)* ","?)? "}" ;
record_field = FIELD_LABEL ":" expr | STRING ":" expr | IDENT | field_path ":" expr | "..." expr ;

Explicit field labels may use keyword spellings such as `type`, `in`, and
`match`. This applies to record/error schemas and constructors, literal keys,
member access and update paths, record patterns, renamed destructuring, and
named arguments. Labels preserve exact key bytes and do not declare lexical
names. Keywords remain invalid variable, parameter, declaration, or import
names; keyword labels require an explicit value or renamed binding, such as
`{type: entry_kind}`, rather than shorthand or puns. Callable labels still
match the checked signature. Quoted literal keys retain arbitrary text, and a
quoted key containing a dot remains a single key. Dynamic field access retains
its existing validation boundary.
map_comp     = "{" field_path ":" expr comp_qualifiers "}" ;
comp_qualifiers = "for" binding_target "in" expr comp_qualifier* ;
comp_qualifier = "for" binding_target "in" expr | "if" expr ;
field_path   = FIELD_LABEL ("." FIELD_LABEL)* ;
```

List and map comprehensions share a textual sequence of one or more `for`
clauses with interleaved `if` filters. Each later clause and the projection
see earlier loop bindings; each loop introduces its own lexical scope.
Execution follows nested ordinary for/if control flow: an inner iterable is
evaluated anew for each reached outer binding, a false filter skips every
subsequent clause and the projection, and each surviving combination evaluates
the projection once. List results retain encounter order. Filters consume
boolean values and never assert.

List comprehensions use `[expr for target in iterable]`; map comprehensions
use `{item.key: value for item in iterable}` with the existing key syntax. The iterable may be a `List[T]`, `Stream[T]`, `Map[T]`, `Str`, or `Bytes`,
including their supported outer `Result` wrappers; result iterables are unwrapped
like `?` before iteration. Comprehension guards must be `Bool` or `Status`.
Map comprehension keys must be `Str`. When two items produce the same key, the
later value replaces the earlier value. Each surviving map entry evaluates
its key before its value. Streams are pulled lazily in nested encounter order
and closed on exhaustion, propagation, or early return. Failed comprehensions
do not expose a partial collection. `ArenaCompQualifier`,
`Checker::check_comp_qualifiers_arena`, and the indexed comprehension frames
own this shared contract. `lint.prefer-list-comp` and `lint.prefer-map-comp`
retain annotations and conservatively recognize adjacent fresh accumulators
with a single nested loop/filter path; accumulator-dependent clauses, shadowed
accumulators, extra statements, transfers, and comments prevent an autofix.

Empty `{}` remains an empty record unless it appears in a context that expects
`Map[T]`; in a map-typed context, `{}` is sugar for an empty map.

A computed entry `[key_expression]: value_expression` selects Map literal mode.
A nonempty constant-key literal also constructs a Map in an expected `Map[T]`
context; otherwise ordinary brace literals remain records. Map entries may mix
computed keys, constant labels, and `...Map` spreads. A spread-only literal
requires Map context; a bound record or dynamic object is not a Map spread.
Computed keys infer one supported scalar domain, or use the expected key type.
Mixed concrete domains are errors. Constant labels remain Str keys. Quoted dots
remain part of one key. Computed comprehension keys use the same `[expression]`
syntax when the key is more than a name or field path. Values use ordinary homogeneous inference and contextual typing;
incompatible concrete values are not weakened to Any.

Entries evaluate once from left to right, each computed key before its value.
Overwritten values still evaluate, later entries and spreads replace duplicates,
and failure stops before subsequent expressions. One Map builder preserves
aliases and snapshot iteration; iteration remains in canonical key order.
`lint.prefer-map-literal` recognizes checked fresh initialization and compatible
set chains. Observed or escaping intermediate maps, uncertain conversions,
and comments prevent unsafe fixes.

A record literal containing a dotted replacement is a functional update:
`{...config, build.jobs: jobs, build.flags.debug: true}`. It requires exactly
one leading spread of a statically known record. All replacements, including
single field entries and shorthands, must select existing fields through known
records and retain their checked types. New fields, further spreads, computed
keys, Map or indexed paths, and duplicate or ancestor-overlapping targets are
rejected. Sibling targets may share ancestors. Quoted keys containing dots
remain singular literal keys; ordinary record construction and spreads retain
their existing behavior.

The base evaluates once and provides an immutable snapshot. Replacement
expressions run once in source order in the surrounding scope, then the
successful replacements rebuild that snapshot. A replacement may read or
mutate the original binding without changing the captured base. Failure stops
later replacements and publishes no partial record; effects already performed
remain visible. Schema defaults do not run again. Reconstruction groups shared
ancestors and preserves value semantics through copy on write.
`lint.prefer-nested-record-update` collapses equivalent nested spreads only
when repeated reads use an immutable, checked record binding and all selected
fields exist. Comments, unstable reads, extra spreads, and newly added fields
prevent the fix.

Ordinary list literals admit explicit spliced elements: `["cc", @flags,
"-o", output_name, @source_names]`. A splice requires `List[T]` and inserts
its elements in place; a list-valued element without `@` remains one nested
list. Several splices, empty lists, multiline expressions, and trailing commas
are supported. Element compatibility and expected types are the same as for
ordinary list elements, including contextual empty lists; no additional Any
widening or argv conversion occurs.

Literal elements and splice expressions evaluate once in source order. A
failure stops construction before later elements run. A Result requires
explicit handling, such as `@(load_flags()?)`; Streams require `.collect()`.
Map, Str, and Bytes do not splice. The builder preserves earlier aliases and
uses one list construction path with checked capacity growth. Splicing does
not add mixed comprehension clauses or unpack call keywords.
`lint.prefer-list-splicing` rewrites checked compatible concatenation and
extension chains when element types and conversions are preserved. Ordinary
nested elements stay ordinary elements; uncertain annotation conversions and
comment-bearing constructions prevent fixes. Simple mutable updates retain
`+=` as their canonical spelling.

Operators:

- `or`, `and`, and `!` operate on `Bool`. `!` also accepts `Status`, using the
  inverse of `status.ok`. `and` and `or` short-circuit: the right side of
  `false and expr` and `true or expr` is not evaluated.
- `??` operates on `Result`; it evaluates to the `Ok` value when the left side
  is `Ok`, otherwise it evaluates and returns the fallback expression.
- `==` and `!=` compare values of the same runtime type.
- `<`, `<=`, `>`, and `>=` operate on `Int` and `Str`.
- `+` operates on `Int`, `Str`, and compatible `List` values; `-`, `*`, `/`, and `%` operate on `Int`.
  Integer `/` truncates toward zero. Integer arithmetic overflow produces an
  `integer-overflow` runtime error, and division or remainder by zero produces a
  `division-by-zero` runtime error rather than a host panic. `//` and `div`
  are not operators; the parser reports the supported `/` spelling when either
  is used.
- `Int` bitset methods operate on non-negative values without adding operator
  syntax: `.bit_and(mask: Int) -> Int` retains shared bits,
  `.bit_or(mask: Int) -> Int` sets mask bits, and
  `.clear_bits(mask: Int) -> Int` removes mask bits. A negative receiver or
  mask is a runtime `integer-bitset` error.
- Path composition is written with formatted path literals, such as
  `fp"${root}/child"`. The `/` operator is numeric division only.
- `in` and `not in` test membership for `List`, substring containment for
  `Str`, byte containment for `Bytes`, display-text substring containment for
  `Path`, and exact entry membership for `env.PATH`.
- `.` accesses record fields and standard methods.
- A newline immediately before a `.` postfix operator continues the same
  expression, so long method chains may use one method per line.
- `.require(Type)` validates the receiver against a type expression and returns
  `Result[Type]`. The type argument is syntax, not a runtime identifier. Named
  record fields are validated recursively inside lists, maps, and optional
  values; a nested missing or mistyped field rejects the entire value before
  typed field access.
- `.require()` uses a concrete target independently supplied by an annotated
  binding, an annotated return or value tail, or a uniquely selected checked
  parameter contract. Expected types flow through value blocks, branches,
  `Ok`, and exactly one propagation layer at each `?`. Validation still runs
  and returns `Result[T, Error]`; annotations alone never validate input.
  `Any`, erased `Record`, unresolved type arguments, other arguments checked
  later, and fallback values cannot supply the target. Named generic schemas
  retain their declaring identity and concrete arguments. The checker reports
  `check.require-target` when the target is not independently known.
  `lint.inferred-require-target` removes only an explicit schema argument when
  that same concrete schema and conversion are supplied by the boundary.
- `?.` guards Optional field access and method calls; `?[index]` and
  `?[start..end]` guard the ordinary indexing and half-open slicing domains.
  Evaluate the receiver once. A null receiver returns null without evaluating
  the field operation, method arguments, index, or explicit bounds. A present
  receiver performs the ordinary operation and lifts its result into Optional,
  flattening redundant Optional layers. Every nullable hop needs its own guard:
  `config?.server?.host?.trim()`. Missing fields, missing keys, invalid indices,
  and method failures retain their ordinary behavior.
- On a checked Result receiver, `?.` and `?[...]` propagate exactly the outer
  Result before performing the ordinary operation, with the ordinary error
  effect and error-type checks. `result?.require(Type)` preserves its dedicated
  propagation and validation route. Optional schema validation is unsupported.
  Optional methods returning Result produce `Optional[Result[T, E]]`;
  `(text?.parse_int() ?? Ok(0))?` handles the Optional layer before the Result.
  `?` accepts only a Result. Mixed Optional/Result layers are never recursively
  unwrapped. New guarded method and index overloads require a checked receiver
  domain; bare Any retains only its existing dynamic field route. A known
  outer `Result[Any]` may propagate once and use the ordinary dynamic operation.
- `?[` is adjacent postfix syntax. Command argument expressions preserve their
  usual boundaries; group a guarded operation and its fallback together when
  passing them as one command argument.
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

An identifier followed by a spaced subtraction operator, such as `value - 1`,
is an expression statement, including in branch tails. Adjacent negative command
arguments such as `command -1` retain command parsing.

Ordering sequences such as `0 <= offset < limit` compare adjacent operands from
left to right. Each reached operand is evaluated once; a false pair skips all
later operands. Every pair uses the ordinary ordering type rules, including
Float/NaN behavior. A single comparison keeps its existing behavior, and
`(a < b) < c` compares the parenthesized Bool value rather than forming a chain.
Ordering binds below arithmetic and above equality and `is`; membership (`in`
and `not in`) shares ordering precedence. `and` binds below equality, while
`or` and the right-associative `??` bind below `and`. Ungrouped mixtures of
ordering with equality, membership, or pattern tests are rejected; use
parentheses to state which Boolean value is being tested.

A failed bare ordering-chain assertion reports the failed adjacent pair and
its evaluated values. Diagnostics never evaluate the skipped operands.


### Half-open Slicing

Half-open `value[start..end]` slicing accepts `List[T]`, `Str`, and `Bytes`,
returning the same collection type. Either bound may be omitted; omitted start
is zero and omitted end is the receiver's length. Bounds are `Int`. Negative
bounds count backwards from the end; each bound is clamped into `[0, length]`.
An end before the normalized start produces an empty value. Receiver, explicit
start, and explicit end are evaluated once, in that order. Omitted bounds reuse
the evaluated receiver. List slices have ordinary list value semantics; text
and bytes may share immutable backing storage through internal views.

`Str` slice indices count Unicode scalar values, including combining marks as
separate scalars. `Bytes` slice indices count bytes. Text slicing does not use
the byte units of `.byte_slice()`. For example, `"aé🦀"[1..3]` is `"é🦀"` and
`b"abcdef"[2..5]` is `b"cde"`.

The offset/count `.slice()` API remains distinct: a negative offset or an offset
past the end is an error, whereas bracket slicing normalizes those bounds.
`lint.prefer-slice` fixes nonnegative constant prefixes and proven in-range
constant suffixes; uncertain offsets, count arithmetic, effectful counts, and
comments inside a call retain the method with an explanation. No fix introduces
an addition that could overflow or changes a count into an end bound.

### Local Result Capture

`try { ... }` executes a value block once and produces `Result[T, E]`.
Normal completion wraps the outgoing value in `Ok`; a Result tail is data,
so `try { operation() }` retains a nested Result, while `try { operation()? }`
propagates one layer into the local boundary. Empty bodies produce `Ok(Unit)`.
An inferred Bool tail remains a value, including false; non-tail Bool statements
and tails checked against Unit remain assertions. Non-tail Result[Unit]
statements retain their ordinary automatic propagation.

Explicit `?`, statement propagation, assertion failures, and plain-run failure
are captured by the nearest try/retry boundary. Ordinary return, break, and
continue keep their lexical destinations. In particular, `return Err(error)`
leaves the enclosing function while `Err(error)?` targets the local boundary.
Abort, cancellation transfers, checker failures, and evaluator defects are
outside capture. Errors describing canceled operations remain ordinary data.

The outgoing value is evaluated before the region's defers. Cleanup runs once
before exposing the Result; a failed cleanup becomes Err when no primary failure
exists, and an existing primary failure wins. Host effects remain required;
locally caught propagation alone needs no outer error effect. Applying `?` to
the resulting Result requires the usual outer error contract. Success and error
types use annotation context and compatible nominal error families, with Error
as the default when no narrower family is established. Error-only blocks with
an unconstrained success type need a Result annotation. Capture emits no retry
attempt metadata and performs no sleep.

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

An optional `on (PATTERN)` clause selects retryable errors through the shared
non-binding pattern rules. Exact nominal variants, applicable facets, wildcard
and grouped alternatives are checked against the attempt's error type. Captures
and aliases are invalid. A nonmatching failure returns that original `Err`
immediately, without consuming a delay or starting another attempt. Matching
failures consume the next delay, or return the final original `Err` on exhaustion.
The delay list still evaluates once at entry, even when the first failure does
not match. Omitting `on` retries every failed attempt.

A manual selective loop can be migrated only when its attempt count, original
error identity, cleanup order, and delay effects are equivalent. A delay computed
only after failure cannot generally move to retry entry; observable counters
cannot disappear. Tooling preserves these loops when equivalence is unproved,
and never adds a filter to an unconditional retry. The maintained
`showcase/run-retry.xsh` uses unconditional retry and retains that policy.

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
returns. Selection happens after attempt cleanup, using the error selected by
the existing primary/secondary cleanup-failure rules.

Effects are the union of the delay expressions and the attempt body. A
non-empty delay list additionally requires the `time` effect because the runtime
sleeps between failed attempts.

Each attempt emits a structured `retry.attempt` trace event with the source
span, attempt number, maximum attempts, next delay when another attempt will be
made, and the failed error kind/message when the attempt failed. `selected` is
present for filtered failures; `stop_reason` identifies success, nonmatching
failure, or delay exhaustion. Continuing failures have no stop reason.

## 7. Pure Functions And Procs

An unannotated immutable `let` directly naming a checked user pure/proc or a
qualified user module export retains that callable's parameter labels,
defaults, return type, callable kind, and effect contract. Another such alias
retains the same signature. Calls use ordinary syntax, including named and
spread arguments; alias creation executes no body or default expression.
The alias retains the original callable handle and capture lifetime. A checked
constant-key module field/index or explicitly propagated getter also retains
the visible callable contract. A validated runtime module contract supplies
argument and effect checks while executing the captured handle; it supplies
no declaration identity and cannot establish an exported alias contract.

`var`, conditional/computed callable selection, and explicitly erased
`Pure`/`Proc` annotations retain the existing dynamic callable boundary.
An exported alias exposes only its public binding name and must retain an
explicit return/effect contract; a private inferred signature alone cannot
establish that public promise. Module contracts recognize callable aliases
as pure/proc exports. Cyclic or unavailable initializer names remain ordinary
lexical errors.

`lint.prefer-callable-alias` replaces exact transparent forwarders only when
parameter order, types, prepared defaults, return annotations, and effect
contracts agree. Wrapping, propagation, conversions, cleanup, and comments
prevent automatic replacement. Removing a forwarding function intentionally
removes its redundant traceback frame; the original callable frame remains.

Definitions:

```ebnf
proc_def     = "proc" PROC_IDENT "(" param_list? ")" effect_list? "->" type_expr block ;
pure_def     = "pure" IDENT "(" param_list? ")" ("->" type_expr)? block ;
stream_def   = "stream" IDENT "(" param_list? ")" effect_list? "->" "Stream" "[" type_expr "]" block ;
param_list   = param ("," param)* ","? ;
param        = IDENT (":" type_expr ("=" expr)? | "=" expr) ;
effect_list  = "[" (IDENT ("," IDENT)*)? "]" ;
return_stmt  = "return" expr_or_run? terminator ;
```

A defaulted parameter may omit its type when ordinary semantic checking of its
default establishes one concrete type. Constants, imported constants, field
projections, primitive expressions, and already permitted calls retain their
checked types. Null and unconstrained empty collections require an annotation;
callers and the function body do not supply parameter constraints. Parameters
without defaults and rest parameters retain their explicit type requirements.

Defaults resolve in the callee's lexical declaration environment, with no access
to other parameters. Supplied arguments evaluate eagerly in source order;
defaults for omitted slots evaluate once in parameter order before the body.
Ordinary callable defaults run during the call. A lazy stream producer evaluates
its omitted expression defaults on the first pull, before executing its body;
an unconsumed producer evaluates no expression defaults. Supplying a slot skips
its default. Default expressions retain ordinary effects, propagation and cleanup
within the callable; no additional callable boundary is introduced.
CLI entrypoint defaults retain their stricter preparation-only value contract.
Signature dependencies must establish concrete types without guessing through a
cycle; an explicit type provides a boundary where inference cannot resolve.

Private pure functions may omit `-> Type`. Infer from typed parameters,
checked callee signatures, explicit returns, and reachable fallthrough tails;
call sites provide no return context. A final Bool is a value. Non-tail
statements retain their statement behavior. Compatible branches must produce a
concrete shape; inconsistent value/missing-return paths require an annotation.
Empty collections without another source of element type, error-only returns,
and dynamic return shapes report `check.infer-return`.

Local callable definitions are analyzed in declaration dependency order.
Captured values retain visibility from the lexical prefix before the function
definition. Every
unannotated pure member of a recursive dependency component reports
`check.required-return`; exported pure functions and module-contract signatures
also retain explicit return annotations. Inference does not create a Result
boundary from `?`: the body must independently determine a compatible Result,
or declare its return type. Annotation-driven conversions, contextual schema
constraints, and implicit Ok wrapping retain their declared boundary.

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
`@expr`, for example `main(@args)?`. Splicing preserves the source list and
accepts prepared constant lists under the same contract as ordinary lists.

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
`Stream[T]`, and each yielded value must match `T`. Ordinary `yield [a, b]`
emits one list item; ordinary `yield stream` is rejected.

`yield @source` delegates the elements of a `List[T]` or `Stream[T]` to the
current producer. Evaluate `source` exactly once when reached. Lists preserve
order; delegated streams pull only on demand and resume the parent after
exhaustion. Handle Results explicitly, as in `yield @(load_rows()?)`; Map, Str,
and Bytes are not delegation sources. A guarded delegation evaluates its
source only when its guard succeeds. Early termination closes the child before
the parent's cleanup, once each; failures retain their original error identity
and stop the parent. Delegation retains one-shot stream alias semantics and
resource ownership. Chained delegation uses the existing frame engine through
an iterative pull and cancellation driver, without a worker or native stack
growth proportional to delegation depth.

`lint.prefer-yield-delegation` fixes checked transparent single-binding loops
whose sole body statement yields that binding unchanged. It preserves explicit
Result propagation and refuses implicit Result sources, conversions, comments,
filters, transformations, cleanup, effects, additional transfers, and source
pipelines whose direct-loop cursor differs from expression materialization.

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

An ordinary private proc without an effect clause receives a checked effective
summary from its body and resolved callees. Explicit clauses, including `[]`,
remain checked upper bounds. A proc with an empty inferred summary remains a
proc; pure/proc separation and return typing are unchanged.

Missing clauses at exported APIs, module contracts, CLI entries, native test
entries, and stream declarations remain unrestricted. Conventional `proc main`
also retains its entry contract. A private implementation cannot establish an
exported callable alias contract merely through inference.

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
- A restricted proc may call procs whose effective effects are covered by its
  upper bound, plus `pure` functions.
- An opaque callable, unrestricted external callee, or unresolved call dependency
  has an unknown summary. Restricted callers cannot use it. Diagnostics identify
  the resolved call chain and the dependency needing a checked contract; unknown
  effects are never approximated as an empty set or `io`.
- Direct calls to standard-module functions (e.g. `fs.read_text`) and standard
  methods (e.g. `path.read_text()`) are checked against `E`; the `io` effect
  covers `fs`, `net`, `process`, and `env` but not `time` or `error`.
- `run` forms, `spawn`, `wait`, and `ProcessHandle.cancel` require the
  `process` effect.
- The `?` propagation operator requires the `error` effect, except inside retry
  attempt blocks where it fails the current attempt instead of propagating from
  the enclosing proc. A proc that declares `error` may use `?` regardless of
  whether its return type is `Result`; a propagated failure exits that proc and
  becomes the caller-visible failure, while a successful value keeps the
  declared return type.
- Unrestricted procs (no annotation) may call anything — no restriction.
- Diagnostic code: `check.effect-violation`.

**Inference.** The checker records resolved calls and direct requirements, then
computes a least fixed point over the finite effect domain. Recursive forwarding
and declaration order produce the same effective summary. Method operations,
executed stage bodies, host forms, and implicit statement assertions and Result
propagation contribute effects; merely referencing a function does not. Local
`try`/`retry` capture removes outward `error` only, retaining host requirements.

Full and compact checking, private signatures, lowering, and linting share these
facts. `xsht lint` does not reinsert an effect clause on an inferred private proc.
It can still suggest missing effects for an explicit upper bound or a missing
entry/public contract. `[lint] prefer-inferred-private-effects = true` opts into
`lint.prefer-inferred-private-effects`: remove a private clause only when a
fresh check preserves effective caller contracts, expression types, and
statement purposes. Deliberately wider bounds, entry/public contracts, unknown
summaries, and commented source retain their clauses.

Statement and value positions are checked language contexts. Initializers,
arguments, explicit return payloads, and tails whose enclosing function, task,
callback, or retry attempt consumes a value use value position. Lowering and
tooling preserve this distinction independently of whether an optimizer uses
the result. Non-tail bare boolean statements assert; boolean value tails
preserve false as a value. Unit and Result[Unit] bodies retain boolean
assertions, including inside their statement-position branches. Top-level
control flow retains statement and integer-exit behavior.

Value-position `if` requires an `else`; value-position `match` requires
exhaustiveness. Their branches may contain ordinary statements followed by a
compatible tail value. Diverging branches retain their lexical return, loop,
and error targets and do not contribute a fabricated Unit to unification.
Incompatible reachable branch values are errors rather than implicit Any
widening. Expression match arms preserve `{}`, shorthand/explicit record fields, quoted
record keys, and spreads as record literals. Braces containing ordinary statements
form value blocks; braces in an expression `if` delimit its branch block.
Bare lexical blocks use this same value-block representation. In statement or
Unit-consuming positions they consume Unit and retain boolean assertions and
Result[Unit] propagation; explicit value positions and genuine value tails
consume the final value. Braces are classified by their first entry's syntax,
independently of expected type: empty braces, identifier shorthand, labeled or
computed fields, and spreads remain literals. `{value}` is a shorthand record;
`{ (value) }` is a value block, and formatting preserves those parentheses.
Malformed field-shaped syntax retains literal diagnostics. A bare block adds
only a lexical binding and cleanup scope; it adds no error or function-return
boundary, module statement permission, or top-level integer-exit permission.
It leaves cwd/env handling unchanged. Scope defers run exactly once before an
outgoing value is exposed; deferred invalidation still invalidates an escaping
handle. `lint.lexical-block` removes a literal true conditional only in checked
statement position, preserving the body scope and its comments. Conditions with
comments before the opening brace and literal-shaped bodies remain explicit.
Selected values are evaluated before scope cleanup; implicit Ok
wrapping occurs only at the established Result boundary. Callback tails share this
scope rule, including fold and keyed stages. Explicit callback returns retain the
enclosing function target. Parallel callback transfers are selected in input order;
started workers finish their cleanup before the transfer reaches its lexical owner.

Function bodies use a contextual tail-value rule. If the final statement in a
`proc` or `pure` body is an expression statement, that expression produces the
function result. If the final statement is a command statement, the command's
statement result produces the function result. A final expression-style proc
call that returns `Result[Unit]` propagates failure and produces `Unit`. A
final plain `run ...` in statement position asserts success and produces
`Unit`.
`lint.redundant-tail-return` removes explicit returns from checked function
value tails, including exhaustive final branches, when the retained return-type
context preserves the conversion and comments. It leaves callback lexical returns
and conditional guarded returns intact.

`lint.redundant-tail-return-binding` flags a final `let` or `var` binding that
is immediately returned, and autofixes it to the initializer as the final tail
expression when doing so would not remove intervening comments. Annotated
bindings are autofixed only when checked types show the initializer already has
the annotated type, so the edit does not remove a binding-level conversion.
Record-schema annotations are not suggested for removal: a record literal may
rely on the binding annotation for its field types, and the corresponding
postfix `: Type` tail expression is not valid XSH syntax. This applies inside
ordinary function bodies and record-producing block/map helpers.
This also covers typed empty-list temporaries such as
`let empty: List[T] = []; return empty` when the function tail type provides
the needed context. `lint.redundant-ok-tail` flags final `return Ok(value)` in
`Result[T]` functions and autofixes to the plain tail value when checked types
show the value already has type `T`.

`assert condition, message` is a Unit statement requiring concrete `Bool` and
`Str` expressions. A message is required; the message-free assertion is an
ordinary bare Bool statement. For example, `assert actual == expected,
f"package $name"` adds context to the failed comparison. The condition runs once. The message runs once
only after a false condition, and supplements the expression and reached operand
diagnostics. Reporting bounds operand rendering and identifies skipped operands
without evaluating them. Containers are reported by type rather than traversed.

Message expressions retain ordinary type, effect, and propagation checks even
when the condition is true. A message failure propagates as its own failure;
otherwise the assertion emits the same core `Error` with kind
`assertion-failed` as a bare Bool statement. A local capture must admit this
core Error type; an assertion cannot be narrowed to a nominal error family.
Assertions use ordinary propagation,
retry capture, and lexical cleanup, including in builds without native-test
support. They are statements, and do not produce a Result value for a consumer.
`lint.core-assert` fixes checked statement `test.ok`/`test.eq`/`test.ne` calls
with inert literal messages and compatible concrete operand types. Eager
nonliteral messages receive guidance to retain their evaluation point; consumed
Results and dynamic equality remain explicit calls.

Non-tail expression statements inside value-producing function bodies must have
type `Unit`, `Result[Unit]`, or `Bool`; Bool statements assert and
`Result[Unit]` statements propagate failure by
default. Otherwise bind the value, return it explicitly, or make it the final
statement. A final exhaustive `if` or `match` produces the enclosing value
when that context consumes one. `while` and `for` retain statement semantics.

`return` without a value returns `Unit`. `Result[Unit]` procs, pure functions,
builder tasks, and effect blocks may fall off the end or tail-produce `Unit`;
the runtime converts that to `Ok()`. A function returning `Result[T]` may
tail-produce or return either a `Result[T]` value or a plain non-`Result` `T`
value; plain `T` is wrapped as `Ok(value)`. `Ok(value)` and `Err(error)` remain
valid when the result shape should be visible at the call site.

`Err(error, cause: failure)` deliberately translates an `Error` into another
nominal error while retaining `failure` as diagnostic metadata. The outer error
and the cause must both be assignable to `Error` for this overload. Generic
one-argument `Err(value)` remains available for every Result error type. The
optional cause is named; both operands are evaluated once in written order,
including a finite named argument spread. Construction produces Result data;
propagation still requires the usual `?` or statement Result boundary.

Cause attachment copies the outer error's metadata without changing its nominal
family, facets, payload, or Result error type, and leaves other aliases unchanged.
An explicit cause replaces the copy's immediate cause and preserves the supplied
cause's chain, spans, contexts, and process status. Matching inspects the outer
error only. A declared payload field named `cause` remains an ordinary field;
metadata has no script introspection API. Owned host resources reachable through
error payloads or causes transfer with the escaping value, just as resources in
ordinary containers do. Checked propagation retains them before lexical cleanup
and across local try/retry capture or callee error transport. A primary cleanup
failure retains its resources before exhausted blocks close; a secondary cleanup
failure releases its resources locally while the original failure wins. A propagated payload
or cause cannot carry live resources across a restored cd/env context; an inner
try may consume the failure while that context is still active. Diagnostic rendering
limits do not limit ownership checks.
`ctx` adds context to the same failure,
whereas `cause` records a translation into a different failure.

Tracebacks and structured traces retain actual nominal identities and render at
most 32 causes. Cause messages are limited to 4096 Unicode scalars, with bounded
facets and contexts, and human output escapes control characters. Omitted suffixes
are marked as truncated. Shared immutable links avoid copying all descendants
at each propagation. Abort and cancellation control transfers do not become
ordinary constructor values.

Signatures support default parameters, rest parameters, type aliases, richer
module signatures, overloads, and known record shapes. Overloads are selected
from argument names and argument types; return type alone does not
disambiguate.

## 8. Control Flow And Results

Implemented control flow:

```ebnf
if_stmt      = "if" condition block ("else" "if" condition block)* ("else" block)? ;
condition    = expr | "let" pattern "=" expr ;
while_stmt   = "while" condition block ;
for_stmt     = "for" binding_target "in" expr block ;
break_stmt   = "break" terminator ;
continue_stmt = "continue" terminator ;
with_stmt    = "with" with_binding ("," with_binding)* ","? block
               "else" ("|" IDENT "|")? block ;
with_binding = IDENT "=" expr ;
guard_stmt   = "guard" "let" binding_target (":" type_expr)? "=" expr_or_run
               "else" ("|" IDENT "|")? block ;
boolean_guard_stmt = "guard" expr "else" block ;
loop_stmt    = "loop" block ;
break_stmt   = "break" expr? (("when" | "unless") expr)? terminator ;
continue_stmt = "continue" terminator
              | "continue" ("when" | "unless") expr terminator ;
return_stmt  = "return" expr_or_run? (("when" | "unless") expr)? terminator ;
yield_stmt   = "yield" (expr_or_run | "@" expr) (("when" | "unless") expr)? terminator ;
match_stmt   = "match" expr "{" match_arm* "}" ;
```

Conditions evaluate to `Bool` or `Status`. A `Status` condition is true when
`status.ok` is true. `while` repeats until its condition is false.

`guard condition else { ... }` evaluates its Bool or Status condition once.
Success continues after the guard; failure runs the ordinary lexical block.
Every reachable path in that block must leave the enclosing continuation by
return, applicable break/continue, or a checked terminating operation such as
`abort`. An arbitrary fallible call, including one followed by `?`, can succeed
and is not termination evidence. A break inside a nested loop does not leave
the guard. The failure block supplies no value or error parameter and establishes
no Result or catch boundary; explicit errors, effects, defer order, and lexical
return/loop/retry targets retain their ordinary meaning.

Checked condition refinements apply to following statements on success and to
the failure block on failure. Null, type/pattern, and field-presence proofs may
refer to a binding or a statically known record-field path. Immutable Bool aliases
retain bounded shared proof provenance through `!`, `and`, and `or`; they emit no
additional checks. Facts identify the original binding and its mutation history.
Parent replacement and overlapping writes invalidate them; proved disjoint sibling
writes preserve them. Unknown or procedure calls invalidate mutable capture facts
regardless of their effect summary. Immutable record copies remain snapshots.

Branches, guards, and successful statement assertions preserve only facts true
on every reaching continuation. Caught assertion failure supplies no success proof
after recovery. Deferred or callable mutable captures require fresh checks inside
the body. Shape predicates keep their existing field/type information and never
authorize an unchecked schema conversion, bounds check, filesystem observation,
or effectful getter. Boolean subexpressions remain values rather than assertions.
Imported modules retain their executable-statement restrictions.

A previously Optional receiver proved present may retain an authored `??`
fallback; indexed lowering reuses the present receiver without evaluating the
unreachable fallback. `lint.redundant-optional-fallback` removes it only with
checked presence provenance and inert, type-equivalent fallback data, preserving
comments. Plain non-Optional receivers without that proof still reject `??`.

`lint.boolean-guard` replaces a leading negative if only with checked evidence
that its body always exits. It retains the authored failure body and error.
Ordering inversions require Int operands; Float comparisons retain logical
negation so NaN behavior is unchanged. Binding a Result continues to use the
separate `guard let` form.

`if let Pattern = subject` and `while let Pattern = subject` use ordinary
literal patterns. They do not unwrap Result or Optional values: `Ok(value)`
selects an Ok payload, and mismatch chooses the next branch or ends the loop.
Subject evaluation errors and explicit `?` retain ordinary propagation.
An if-let subject executes once; a while-let subject executes once per check,
including after `continue`. Successful captures are immutable branch or
iteration locals, published only after the complete pattern matches. Captures
may shadow outer bindings without changing them; failed captures are unavailable
in else branches and after the conditional. Irrefutable binding and wildcard
conditions are rejected because they cannot test anything. Condition binding
chains are unsupported, and `guard let` retains its separate Result contract.

Value-producing if-let requires an else and compatible branch values wherever
ordinary if expressions are legal. While-let remains a statement. Each pattern
condition and selected branch has a resource scope; cleanup runs on mismatch,
continue, break, return, propagation, and runtime failure. Escaping values retain
their ordinary ownership. `lint.pattern-conditional` can replace an unguarded
two-arm match with a selected binding pattern and proven complement. It retains
meaningful else behavior and declines fixes that would lose comments or error
payload bindings.

Postfix `when` and `unless` apply to `return`, `break`, `continue`, and
`yield`. The condition executes first, and the payload executes only if the
selected branch is reached; an unselected guard continues at the next statement.
Selected-branch narrowing applies to the payload: `return cached when cached != null`
can return a non-null value. Return wrapping, loop targets, stream
ownership, effects, and deferred cleanup follow the ordinary control statement.
A guarded return does not make subsequent statements unreachable. A break payload
follows the existing break rules; only a `loop` expression consumes it as a loop
result. `while` and `for` retain statement semantics.

Group a run-valued payload: `return (run.status /usr/bin/true) when ready`.
Ungrouped `return run.status /usr/bin/true when ready` passes `when` and `ready`
as external argv words, preserving the ordinary command boundary. The same
explicit grouping applies to guarded `yield` and run-valued `break` expressions.

Each `if`, `else if`, and `else` block has its own lexical scope. A local
declared in one branch is unavailable in sibling branches; the same spelling
in two branches denotes separate bindings, including when the `if` is the
last statement of a proc or pure function.

`PATTERN as name` captures the complete value matched at that node while
preserving inner captures. Names must be ordinary, non-discard bindings;
repeated names anywhere in one pattern are rejected. Alias binding is tighter
than `|`: `P | Q as name` aliases Q; `(P | Q) as name` aliases the whole
alternative. Groups control pattern precedence and do not create tuple values.

Alternatives are checked in isolated scopes and must bind exactly the same
names with identical resolved types. Type aliases may have different spelling
while resolving to the same type. Incompatible payloads are rejected rather
than widened to Any. An outer alias retains the common complete subject type,
including its nominal identity, rather than one alternative's payload shape.

The subject is evaluated once. Alternatives are tried in textual order, and
only the first complete match publishes captures. Failed alternatives leave
capture slots untouched and create no list rest copies. An arm guard runs
after that first match; guard failure advances to the next arm. Captured values
retain ordinary value ownership. These rules also apply to `if let` and
`while let`. Exhaustiveness and reachability inspect grouped and aliased
patterns conservatively and exclude guarded arms from total coverage.

`lint.identical-match-arms` combines adjacent, unguarded bodies only after
reparsing the CST and AST and checking the combined capture contract. Comments,
incompatible bindings or types, and uncertain source ownership prevent a fix.

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

`value is Pattern` evaluates its subject once and returns Bool using the same
nominal constructor, error variant, facet, literal, and record pattern matcher.
Its RHS cannot bind names: write `outcome is Ok(_)`, not `Ok(payload)`; record
shorthand that would bind is rejected. Nested constructor and record payloads
may contain literals or other non-binding patterns. Capture-free alternatives
require explicit pattern grouping, as in `value is (P | Q)`; aliases remain
forbidden. `or` may also join complete tests. Negation is `!(value is Pattern)`.
Selected-branch narrowing uses a type shared by every alternative and preserves
nominal error identity; incompatible alternatives do not invent a union type.
In control conditions, qualified error payload patterns use explicit fields such
as `error is Family.Variant {message: "missing"}` before the branch body.

A bare RHS name resolves to a type, error facet, or zero-field constructor.
Unknown names are errors; ambiguous namespaces require a qualified name.
Type tests require the existing dynamic subject boundary. Facet and error
variant tests preserve nominal identity. Tests on stable immutable bindings
narrow the selected true branch; payload variables are never introduced.
Testing a Result preserves the wrapper and does not propagate an Err.
`is` shares equality precedence, above `and` and `or` and below arithmetic and
ordering. A bound, returned, or filtered test is a value; a bare test uses the
Bool statement assertion contract.

`lint.boolean-pattern-test` identifies trivial boolean matches with a proven
complement. Its safe fix preserves one subject evaluation, rejects binding
patterns and guards, and retains matches containing comments. Explicit Result
Ok/Err complements are eligible only when the checked subject is a Result.

`for` and comprehension clauses iterate over `List[T]`, `Stream[T]`, `Map[K, V]`,
`Str`, and `Bytes`, including their supported outer `Result` wrappers. A map item has
structural type `{key: K, value: V}` for both simple and destructured targets.
Entries follow the same deterministic key order as `Map.keys()` and
`Map.values()`. The receiver is evaluated once; a cursor retains its storage as
a snapshot, so later assignments to the original map do not change the keys or
values encountered. Only the current entry record is constructed. Nested record
and Result values remain the entry's `value` without further unwrapping.
`Result[Map[T], E]` propagates its outer failure through the surrounding lexical
Result boundary, preserving `E` and checking the required error effect.
`.keys()`, `.values()`, and `.get()` remain available for their distinct uses.
This entry iteration rule does not change pipeline map-source semantics.

Direct `Str` iteration produces one-scalar `Str` values in Unicode scalar order;
combining marks remain separate scalars and no normalization occurs. `Bytes`
produces `Int` values in 0..255 without decoding. Empty sources have no items.
Ordinary loops and comprehension clauses evaluate each reached source once and
retain its storage and view bounds as a snapshot. Reassigning a source binding
inside a loop does not replace that snapshot. Cursors construct only the next
scalar view or byte value, preserving checkpoints, immutable bindings, lexical
transfers, and cleanup without an intermediate element List. Supported outer
`Result[Str, E]` and `Result[Bytes, E]` sources propagate through the existing
iterable boundary with `E` and its ordinary error effect. This direct iteration
rule adds no string/byte pipeline source, List splice, or yield delegation.

`lint.prefer-scalar-iteration` replaces directly iterated checked
`text.split("")` expressions. Split adapter bindings and bounded splits remain
Lists. A byte offset loop is eligible only for a proved immutable Bytes source,
its exact full range, and an offset used solely by the first byte extraction.
Indexed scanners with meaningful offsets retain their access and bounds.

For existing fallible list and stream sources, `Result` wrappers are auto-unwrapped; an `Err` propagates
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
} else { |e|
  print f"setup failed: ${e.message}"
}
```

### Block parameter conventions

All parameterized blocks use `{ |parameters| ... }`. The header is recognized
at the start of the block after whitespace and comments; it is never a pipeline
expression. Stream stages and other existing parameterized constructs retain
their arities and checked input types.

An error handler on `with` or `guard let` accepts zero parameters or one
immutable parameter; `_` discards that input. A guard handler receives the
initializer's exact Result error type. A `with` handler receives the common
error type of its sequential initializers: a shared nominal type is retained,
and differing error families use `Error`. Bindings introduced by successful
`with` initializers are visible to later initializers and the body, but not to
the error handler. Handler names are local to that block, may shadow an outer
binding, and cannot escape or be redeclared in that scope.

```xsh
with result = fallible_op() {
  use_result(result)
} else { |failure|
  print f"failed: ${failure.message}"
}

guard let n = parse(input) else { |failure|
  return Err(failure)
}
```

Plain conditional branches, boolean guard failure blocks, and deferred blocks
do not receive an input and reject parameter headers. The outside-brace
`else |failure| { ... }` spelling is invalid source. `lint.block-header` can
move a comment-free legacy header into its block and recheck the resulting
program; ambiguous comment layouts receive guidance without a rewrite.
Headers establish lexical bindings without a callable frame or a new Result
boundary. Return, loop control, propagation, and cleanup retain their enclosing
targets.

A `guard let` else block inside a loop may use `break` or `continue`; either
statement controls that enclosing loop.

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

`abort(status: Int, force: Bool = false)` exits the script with `status` as an
explicit deliberate process termination. It does not use Result propagation and
therefore emits no runtime traceback on stderr. Deferred cleanup runs while
unwinding by default. `abort(status, force: true)` skips deferred cleanup.

`left ?? fallback` operates on `Result` and `Optional` values. For `Result`:
`Ok(value)` evaluates to `value`; `Err(error)` evaluates the fallback. For
`Optional`: a non-null value evaluates to itself; `null` evaluates the fallback.
`??` is right-associative. `or` remains Bool-only; use `??` for Result and
Optional fallback.

`result ?? { |failure| statements; tail_value }` selects a lexical error handler
only for `Err`; `Ok` returns its payload without evaluating the handler. The
parameter is required, exactly one, immutable, and scoped to the handler; `_`
discards it. Its type and runtime value retain the Result's nominal error.
The `{ |...|` prefix distinguishes this form from an ordinary record fallback.
Optional values have no error payload and cannot use a parameter block.

Handler tails must match the success type, including nested Results. Boolean
tails are values. The handler creates no function, loop, or Result boundary:
`return`, legal loop transfers, and `?` retain their enclosing targets, including
retry-local propagation. Values are selected before scope cleanup; handler
failures retain their own error identity and source. `??` remains lazy and
right-associative. `lint.error-fallback-block` replaces checked identity-Ok
expression matches only when the complete unguarded Err arm can be retained;
it leaves success transformations, error-case distinctions, and comments intact.

Ignoring a value-producing `Result` is a checker error. A `Result[Unit]`
statement propagates failure by default. Assign to `_` only when an ignored
value-producing result is intentional:

```xsh
let _ = fs.remove(path)?
```

`_` is a discard binding, not a reusable variable. Repeated `let _ = ...`
bindings are allowed and each initializer is still evaluated.

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
             | "env" "(" expr ")" block
             ;
env_assignment = IDENT "=" command_arg ;
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
dollar_shorthand = "$" IDENT ("." FIELD_LABEL)* ;
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
conversion. `env (overlay) { ... }` evaluates one Record or `Map[Str, V]`
before entering the body and converts each supplied value with the existing
argv-item conversion. Null is a supplied unsupported value, rather than an
unset request. Every name and value must satisfy the existing environment name,
NUL, and native encoding checks. Inherited environment bytes remain unchanged.
The former `env { NAME = expression } { ... }` form is rejected with a narrow
migration to a parenthesized ordinary record literal.

Command-style proc calls are not accepted. Use expression-call syntax so
argument boundaries and types remain explicit.

`print` writes its arguments separated by one space and appends a newline to
stdout. `eprint` does the same on stderr. `print --flush` and `eprint --flush`
write to the process's inherited stdout or stderr immediately instead of the
captured script-output buffer; `--flush` is recognized only as the first
argument. Both return `Unit` and require no declared effect. They accept human-facing scalar output: `Str`,
`Int`, `Bool`, and `Path`. `Path` interpolation and printing use display
conversion and must not canonicalize, resolve, or otherwise change the path.

Script stdout and stderr are byte streams. Text-producing APIs append UTF-8
bytes; `io.write_stdout_bytes` appends bytes exactly and does not check
UTF-8.

`cd path { ... }` changes the evaluator cwd context while its statement body
runs and returns `Result[Unit, Error]`. Parenthesized `cd (path) { ... }` and
`env (overlay) { ... }` in a value position consume the ordinary body tail and
return `Result[T, Error]`. A Result-valued tail remains nested; a false predicate
tail is data in a value body. Statement scopes retain command and assertion
classification, including plain statement-position runs.

These scopes restore the evaluator's previous context on normal completion,
propagation, lexical return, loop transfer, cancellation, and runtime failure.
Inner defers execute before restoration under the scoped context; existing
primary and cleanup failure precedence applies. Entering or restoring a scope
follows its Result contract, while body `?` retains its enclosing function,
retry, or explicit `try` destination. A scope does not capture body failures
locally. Use `try` when local capture of the whole operation is intended.
Neither scope mutates the embedding host process's global context. Live
producers or handles, including those retained in error payloads and causal
chains, cannot escape a scope as its value, through an outer
assignment, a lexical return, or a yielded item. Consume them inside the body.
A producer containing a scope may yield ordinary data: its selected cwd/env is
private while suspended and is reattached for pulls, delegated children, and
cancellation cleanup. Consumers retain their own context between pulls.

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
             | run_head "(" command_arg+ redirection* ")"
             ;
run_head     = "run" | "run.status" | "run.text" | "run.bytes"
             | "run.capture" capture_mode | "run.stream" capture_mode ;
capture_mode = "--text" | "--bytes" ;
run_target   = word | typed_arg ;
```

All run forms accept `--timeout=<Duration>`, `--cpumax=<Int>`, and
`--accept=<List[Int]>` immediately
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
- `run.builtin*` is rejected before execution. The narrow migration removes
  only the redundant qualifier, preserving modes, options, argv, and redirections.

Process results:

- Plain `run` and byte pipelines in statement position assert success by
  default, update `$?`, and propagate `ProcessError` for nonzero exits, signal
  termination, setup failures, and failed pipeline segments.
- Plain `run` in value position evaluates to `Status` and updates `$?`.
- `run.status` is the explicit status-preserving form. It evaluates to
  `Status`, updates `$?`, and does not propagate unsuccessful completion
  unless followed by trailing `?`.
- In statement position, `run.status` is the best-effort form: it evaluates and
  discards the status without requiring a discard binding.
- `run.text` returns `Result[Str, ProcessError]`.
- `run.bytes` returns `Result[Bytes, ProcessError]`.
- `run.capture --text` returns
  `Result[{status: Status, stdout: Str, stderr: Str}, ProcessError]`.
- `run.capture --bytes` returns
  `Result[{status: Status, stdout: Bytes, stderr: Bytes}, ProcessError]`.
- `run.stream --text` returns `Result[Stream[Str], ProcessError]` after explicit UTF-8 decoding.
- `run.stream --bytes` returns `Result[Stream[Bytes], ProcessError]`.

Explicit completion policy:

`--accept=EXPR` evaluates once with the run options before spawning. The value
must be a nonempty List[Int] containing unique codes in `0..255`; invalid literal
policies are diagnosed during checking, and dynamic policies are validated before
any child starts. Malformed dynamic configuration raises `RuntimeError` with
kind `accept-policy` at this earlier option-conversion boundary, including when
the selected run mode normally returns a Result. The option contributes the
`error` effect. An ordinary exit is
accepted exactly when its actual code belongs to the set. `Status.ok`, the exit
code, capture records, and `$?` retain the actual child result. Signals are never
accepted as shell-style `128 + signal` exit codes. Setup, timeout, cancellation,
capture-limit, I/O, and decoding failures remain errors.

Configured Result forms return `Err(ProcessError.UnexpectedExit)` for a rejected
ordinary exit, including rejected exit zero. Direct Status forms propagate the
explicit validation failure. A pipeline applies each set to its own segment;
other segments must complete successfully, and the first rejected segment retains
the pipeline's actual status and source metadata. With no option, the mode-specific
contracts above remain unchanged.

Policy-bearing process streams yield stdout incrementally; their completion check
can fail after rows have been consumed. Cursor completion and decoding failures
are checked `ProcessError` values that a `try` around consumption can capture.
Consumers that stop early cancel and reap the owned child. Text decoding and the
existing output limit remain enforced.
`Command` stores the same policy through the `accept` builder field, a run entry's
`--accept` option, or the optional `accept` argument to `process.command_argv`.
Ordinary owned waits apply it; list waits still drain every requested handle after
a rejection. Explicit cancel and scope cancellation retain cancellation semantics.

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
arguments, argv splices, environment overlays, `--timeout`, `--cpumax`, `--accept`, cwd,
and redirection behavior used by `run`, and inherits stdio by default. V1
rejects byte pipelines, `run.text`, `run.bytes`, `run.capture`, `run.stream`,
and any form that cannot map to exactly one child process. There is still no
shell-string process execution form.

`spawn command_expr` uses the command plan's target, argv, cwd, env overlay,
timeout, `cpu_max`, `accept`, `detach`, `new_session`, and `ignore_hup` fields. This is
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

Stdin `<` also accepts Bytes, evaluated once at the redirection position and
sent exactly, including NUL, without adding a newline or creating a temporary
file. Empty Bytes closes stdin with no content. Str remains a file path; text
content requires explicit UTF-8 encoding. Result operands require handling.
Bytes cannot redirect output, compete with another stdin source, or replace
stdin wiring on a later byte pipeline segment. Command plans retain byte input
until delivery or termination; owned spawn continues delivery at process-owner
checkpoints and wait/cancel/cleanup. Capture drains output while feeding input,
and a consumer closing stdin early does not turn successful exit into failure.

Redirection targets otherwise are typed path-like values or non-negative file descriptor
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
  members: List[Path] = []) -> Result[Stream[Record]]`. Creating the stream
  validates and buffers selected entry records in archive order; file contents
  are not materialized. Consume it directly or use `.collect()` for a list.
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
the portable `newc` format. Zip support lists and extracts existing Stored and
Deflate archives. Extraction verifies each file's decompressed size and CRC.

Extraction rejects absolute paths, parent traversal, existing destination files
unless `overwrite` is true, symlink destinations, symlink ancestors, and symlink
or hardlink targets that escape the destination tree.

Archive entry records have `path: Path`, `kind: Str`, `size: Int`, `mode: Int`,
`modified: Int`, and `link_name: Str`.

`cli`:

- `cli.applet(argv: List[Str], schema: Record, command: Str = current script) -> Result[Record]`.
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
fields. When the descriptor is established constant data, the checker infers a
result record shape. Inline descriptors, `const` references, imported constants,
closed constant field projections, and admitted constant record composition
use the same normalized descriptor plan as argument parsing. Required and
defaulted scalar fields are concrete, non-required scalar fields are optional,
repeated fields are
`List[T]`, and flags are `Bool`. Schema descriptors may be a type string such as
`"Str"` or a record with `kind`, `form`, `default`, `required`, `repeated`,
`flag`, `long`, `short`, `choices`, `conflicts`, `requires`,
`required_group`, `env`, `hidden`, `deprecated`, `help`, `optional_value`,
`optional_default`, `min`, `max`, `positive`, `nonzero`, `exists`, `file`, and
`dir` fields. The `form` field is a compact usage spelling such as
`"-j --jobs N"`, `"--root DIR"`, `"--color[=WHEN]"`, or `"...FILE"`; option
spellings become long and short aliases, non-option forms before any option
mark positionals, `[=...]` marks an optional option value, and `...` marks
repeated positionals. A non-repeated positional is required unless it declares
a `default` or an explicit `required: false`; an explicit `required` field
always wins. If `kind` is absent, the value type is inferred from
`default`; absent defaults use `Str`, and repeated fields without a default use
`List[Str]`. Supported option value types are `Str`, `Int`, `UInt`, `Bool`,
`Path`, `Duration`, and `List[...]` through repeated options. `UInt` parses
non-negative decimal integers and returns an `Int` value. Runtime results
include `null` for absent optional scalar fields. Errors include the failing
argv index in the diagnostic message when the failure comes from an input item.

`cli.applet` uses the same typed schema and result inference as `cli.parse`, but
its scalar options use last-occurrence-wins semantics. This supports common
Unix applet forms such as short-option clusters and attached short values, with
a later occurrence overriding an earlier one. `cli.parse` remains strict:
repeating a non-repeated scalar option is a usage error. Both APIs reject
undeclared options and preserve the existing `--` operands-only marker; use a
repeated positional form such as `...FILE` for applet operands.

`cli.parse` and `cli.parse_full` reserve `-h` and `--help` implicitly. Schemas
do not declare a help option or include a help field in their result record.
The optional `command` label controls the rendered usage prefix; scripts
normally omit it, while subcommands can pass labels such as `"pm world-plan"`.
When help is requested before `--`, parsing returns a `cli-help` error whose
message is the rendered usage. If that result is propagated with `?` at script
top level, XSH prints the usage to stdout and exits successfully. Other parse
failures keep the `cli-parse` error kind, include the rendered usage after the
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

Known descriptors are validated during checking, with diagnostics attributed to
their declaration source. This intentionally rejects malformed static
descriptors before execution; dynamically computed descriptors retain runtime
validation and dynamic result types. A string field type without a constant
value does not establish a descriptor. Preparation never calls user functions
or reads runtime bindings or host state to infer its contents.

`cli.parse_full` returns `{values: Record, sources: Record, warnings:
List[Str]}`. For a known descriptor, `values` retains the same concrete record
type inferred for `cli.parse` and `cli.applet`; `sources`
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
  The walk is lazy and serial. Entries follow filesystem traversal order,
  which is not guaranteed to be sorted; use `|> sort-by .path` when a
  deterministic order matters. `stat: false` skips the per-entry `stat` for a
  cheaper traversal; stat-derived fields are unavailable and reading them
  returns a `metadata-unavailable` runtime error. Hidden entries are skipped
  by default; pass `hidden: true` to include dot-prefixed files and directories.
- `fs.files(path: Path, gitignore: Bool = true, stat: Bool = true, exts: List[Str] = [], hidden: Bool = false) -> Result[Stream[Record]]` —
  equivalent to `fs.walk |> where .kind == "file"`. Preferred over the full
  walk when only files are needed. When `exts` is non-empty, only files whose
  no-dot, case-sensitive `ext` field is in the list are emitted; directories are
  still traversed. The filter is applied before file records are built, so it
  avoids per-file `stat` work for non-matching files when `stat: true`. Include
  `""` to emit extensionless files. Supplied `gitignore`, `stat`, and `hidden`
  expressions on `fs.walk` and `fs.files` are evaluated once when called.
- `fs.dirs(path: Path, gitignore: Bool = true, stat: Bool = true, hidden: Bool = false) -> Result[Stream[Record]]` —
  equivalent to `fs.walk |> where .kind == "dir"`.
- `fs.children(path: Path, stat: Bool = true, ordered: Bool = true) -> Result[Stream[Record]]` —
  enumerates only the entries directly under `path`; it never recurses. With
  `ordered: false`, the stream reads directory entries lazily in host order.
  `ordered: true` materializes and sorts the entries by path before yielding
  them. `stat: false` uses the directory entry type; stat-derived fields are
  unavailable and reading them returns a `metadata-unavailable` runtime error.
  Each entry's `path` retains native bytes. Its `name` and `ext` fields are
  `Str` display text and may replace invalid UTF-8; use `path` for filesystem
  operations.
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
- `FsRoot.close() -> Result[Unit]`.
- `FsRoot.host_path() -> Result[Path]`.
- `FsRoot.open_root(path: Path) -> Result[FsRoot]`.
- `FsRoot.read_bytes(path: Path) -> Result[Bytes]`.
- `FsRoot.read_result(path: Path,
  max_bytes: Int = 1048576) -> Result[FsRootReadResult]`.
- `FsRoot.filesystem_stats(path: Path) -> Result[FsRootFilesystemStats]`.
- `FsRoot.read_text(path: Path) -> Result[Str]`.
- `FsRoot.children(path: Path,
  max_entries: Int = 65536) -> Result[FsRootChildrenResult]`.
- `FsRoot.write(path: Path, data: Bytes) -> Result[Unit]`.
- `FsRoot.write(path: Path, data: Str) -> Result[Unit]`.
- `FsRoot.write_atomic(path: Path, data: Bytes) -> Result[Unit]`.
- `FsRoot.write_atomic(path: Path, data: Str) -> Result[Unit]`.
- `FsRoot.metadata(path: Path) -> Result[FsEntry]`.
- `FsRoot.exists(path: Path) -> Result[Bool]`.
- `FsRoot.mkdir(path: Path, mode: Int = 0o777, parents: Bool = false) -> Result[Unit]`.
- `FsRoot.remove(path: Path, dir: Bool = false) -> Result[Unit]`.
- `FsRoot.readlink(path: Path) -> Result[Path]`.
- `FsRoot.readlink_result(path: Path) -> Result[FsRootReadlinkResult]`.
- `FsRoot.symlink(target: Path, path: Path, parents: Bool = true,
  overwrite: Bool = false) -> Result[Unit]`.
- `FsRoot.chmod(path: Path, mode: Int) -> Result[Unit]`.
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
extension without a leading dot. When `stat: false`, stat-derived fields are
unavailable: reading a numeric or permission field returns a
`metadata-unavailable` runtime error rather than a zero or false placeholder.
`path`, `name`, `kind`, and `ext` remain populated.
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
opaque `FsRoot` capability backed by an open directory handle owned by
the evaluator. A record containing an `id` cannot construct or validate as a
capability. Aliases share close state; opening a child creates an independent
handle that remains active after its parent closes. No implicit destructor is
introduced. `fs.tempfile` returns `{root: FsRoot, path: Path}` where `path`
is relative to the returned root. `FsRoot.host_path` is an explicit escape hatch for
APIs or subprocesses that still require host paths; it returns `Err` when the
root is closed or the platform cannot expose the path. `FsRoot` methods
resolve relative paths from the handle rather than by joining strings. Their
filesystem opens are kernel-confined below the root: absolute paths, `..`
traversal that escapes the root, symlinks that escape the root, and concurrent
pathname manipulation fail. Relative symlinks whose final resolution remains
below the root work normally. `FsRoot.readlink` and `FsRoot.symlink` operate
on symlink target text without traversing it. This makes the rooted APIs the
preferred surface when a trusted root directory is combined with untrusted
relative names. `FsRoot` confines pathname resolution; it is not a process
sandbox and does not restrict mounts or device nodes below the root.
Receiver and argument entries evaluate once in source order, including reordered
named arguments. Each method retains its original filesystem effect, error kind,
and host operation. Factories stay in `fs`; `fs.root_install_file` retains its two
capabilities. The old single-root module names are rejected and recognized only
for checked migration guidance. Automatic promotion requires the receiver to be
the first evaluated argument and preserves the remaining named argument text;
comments, spreads, and reordered receivers require a manual rewrite.
`FsRoot.readlink_result` rejects a relative path whose `..` components cross
the root as invalid input; a missing parent within the root remains an absent
source result.
`FsRoot.children` returns child paths relative to the root, ordered by their
raw filename bytes so non-UTF-8 names remain lossless. `max_entries` may be
between zero and 65,536. `FsRootChildrenResult` carries `state`,
`enumeration_succeeded`, `children`, `errno`, and `error_kind`; state is one of
`complete`, `absent`, `permission_denied`, `read_failure`, or `truncated`.
Children are sorted by raw filename bytes before applying `max_entries`.
Enumeration reads at most 65,537 directory entries to detect a larger
directory. Partial paths remain available when a directory read fails or
reaches either bound, and an empty `complete` result means the directory was
successfully enumerated and contained no entries.
The confined open requires a readable directory, so a file or FIFO at the
requested path is a `read_failure` observation rather than an iterable source.
`FsRoot.read_result` reads at most `max_bytes` and permits values from zero
through 16,777,216. Its result records `state`, optional `data`, optional
`errno`, optional stable `error_kind`, and `truncated`; expected missing and
permission-denied paths are observations, while an invalid bound is an error.
`FsRootReadResult` uses `state` values `observed`, `absent`,
`permission_denied`, or `read_failure`. `error_kind` uses stable I/O classes
such as `not_found`, `permission_denied`, `interrupted`, and `other`.
`FsRootReadlinkResult` observes the symlink target without following it. Its
optional `target`, `errno`, and `error_kind` distinguish an existing non-link
or failed read from an absent entry; state uses the same four values as
`FsRootReadResult`. Invalid or escaping rooted paths remain errors.
`FsRoot.filesystem_stats` queries capacity through a directory opened below
the root, without converting the path back to an ambient host path. Its path
must be relative to the root. `FsRootFilesystemStats.state` is `observed`,
`absent`, `permission_denied`, `malformed`, `read_failure`, or `range_failure`; byte fields
are exact signed integers when observed, and remain null when the platform
counters cannot fit that representation. The record preserves `errno` and a
stable `error_kind` for filesystem-query failures.
`FsRoot.mkdir` applies the requested mode to the created directory through a
handle resolved below the root, so the caller's umask does not change the final
mode.

`path`:

- `path.absolute(path: Path) -> Result[Path]`.

`path.absolute` joins relative paths to the current XSH cwd and lexically
normalizes `.` and `..` components without requiring the resulting path to
exist. Absolute inputs are normalized in place. Use `.resolve()` when the path
must exist and symlinks should be resolved by the host filesystem.

Path values also expose `.parent`, `.name`, `.basename()`, `.dirname()`, `.ext`,
`.ext_or(fallback)`, `.display()`, `.normalize()`, `.resolve()`, `.exists()`,
`.executable()`, `.du()`,
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
`.get(field: Str)`, `.keys()`, field access for known exports, and string
indexing. A proven visible export selected by a literal or prepared constant
Str key retains its checked success type in `.get`; dynamic or unknown keys
retain `Result[Any]`. Use `.get()` or `.has()` before accessing optional exports
when absence is expected. Exported types are checker-visible through static
imports, but they are not runtime module fields.

The legacy `record.require` module API and its runtime type-string grammar
are removed. Declare a named schema and validate with `value.require(Schema)`.
This keeps extra fields and checks nested schema fields. An absent optional key
needs explicit `.has()`/`.get()` validation; a `T?` schema field is still required
and admits a present null. Callable contracts use the existing typed module
contracts or explicit application validation. Runtime-selected policy remains
application-owned dynamic validation. Retained CLI descriptor strings are a
separate configuration format.

Removed calls report `check.removed-record-require`. A migration fix is offered
only for required-only constant contracts with an existing exact named schema,
plain records already proving every required field, and identity scalar checks.
Opaque records, optional fields, callable strings, dynamic values, source paths,
conversions, and comments require manual review. Preserve distinct error kinds
or source context with explicit error translation or context at that boundary.

`lint.removed-record-require` can combine the exact identity edit with recognized
syntax migrations across imports. The entire rewritten graph must parse and
check before any source is written. Other checker failures still block the
migration, including field access on an untyped removed-call result; those
consumers need manual repair. Syntax/API repair precedes ordinary lint rewriting:
a following pass can remove redundant validation or normalize layout after the
source becomes valid.

Record values also expose `.has(field: Str)`, `.get(field: Str) ->
Result[Any]`, and `.keys()`. A literal or prepared constant Str key selecting a
visible checked field preserves that field's type: `.get(key)` returns
`Result[FieldType]`, while indexing has the ordinary field type and retains its
existing access errors. A present nullable field returns `Ok(null)` from get.
Unknown keys and fields hidden by a narrower record contract remain dynamic;
a checked width-compatible record does not reveal hidden runtime field types.

The same rule applies to module exports visible in a checked module contract.
Optional exports still return the existing missing-field Result when absent;
known callable exports retain their checked signature and effect contract.
Receiver and key expressions retain their evaluation order and execute once.
A key proof does not remove access errors or perform schema validation.

`net`:

- `net.pool(name: Str = "default", max_idle_per_host: Int = 8,
  idle_timeout: Duration = 90s) -> Result[Record]`.
- `net.close_pool(name: Str = "default") -> Result[Unit]`.
- `net.close_all_pools() -> Result[Unit]`.
- `net.request(request: Record) -> Result[Record]`.
- `net.request_many(batch: Record) -> Result[List[Result[Record]]]`.
- `net.download(request: Record) -> Result[Record]`.
- `net.download_many(batch: Record) -> Result[List[Result[Record]]]`.
- `net.upload(request: Record) -> Result[Record]`.
- `net.start(request: Record) -> Result[NetJob]`.

`net` supports HTTP and HTTPS only. XSH keeps its record adapters, redirect and
body-limit policy, and evaluator-owned named pools, while `h12tiny-client`
owns HTTP framing, handshakes, ALPN, and connection reuse. Each named pool has
a persistent HTTP/1.1 h12 client for `net.request`, `net.download`, and
`net.upload`, with at most eight idle connections per origin and a 90-second
idle timeout by default. The same agent also owns a persistent auto HTTP/1.1 or
HTTP/2 client for `net.request_many`, `net.download_many`, and `net.start`.
Agents are keyed by complete pool and TLS policy. Closing or reconfiguring a
pool retires only those agent entries: accepted work retains its client clone
and may finish, while a later call constructs a new generation.

Each evaluator lazily owns one private network runtime. A real accepted
transport operation starts its named driver; `net.pool`, pool closure, mocks,
invalid input, and disabled-network builds do not. The driver advances only
DNS, sockets, TLS, HTTP, timers, and its bounded network-specific file lane; it
cannot evaluate XSH code, run signal hooks, or invoke callbacks. The runtime
admits at most 32 active and 128 pending transport operations. Its two lazy file
workers prepare file request bodies and upload sources and perform download
writes, cleanup, and atomic finalization without blocking the transport driver.
File-backed request bodies, upload sources, and download destinations are
prepared through that lane before an active transport permit is acquired.

Single calls remain synchronous XSH operations and use the H1-only client. They
submit an internal operation then checkpoint while waiting, so pending signals
can cancel and terminally drain the transport. Batches use the auto client and a
completion-driven sliding window: at most `concurrency` inputs are submitted,
the next input starts as soon as any item completes, and results remain in input
order. Ordinary per-item transport failures remain inner `Result` values;
malformed batch records, evaluator cancellation, and runtime failure are outer
errors. Separate batch calls reuse the pool's auto client rather than creating a
fresh client.

`NetJob` is an opaque, evaluator-owned host resource. `NetJob.wait()` returns
the same response record as `net.request`; `NetJob.cancel()` returns `Unit` only
after a terminal outcome has been recorded. Both require the `net` effect. A
handle aliases its numeric job ID, and the first wait or cancel consumes it;
later aliases return `net-job-not-live`. Jobs transfer through ordinary returned
or assigned composite values, and a nonescaping job is canceled and drained
during scope cleanup. Up to 64 live jobs and 64 MiB of conservatively reserved
response capacity are admitted per evaluator. `NetJob` cannot be serialized,
cached, converted to a command argument, or constructed directly. Native
`test.mock(ctx, "net.start", request, Ok(response))` creates an owned completed
job without starting the transport driver.

Structured traces correlate a network job by its opaque job ID. The evaluator
emits `net.job.accepted`, `net.job.scheduled`, `net.transport.started`,
`net.transport.completed`, and the consuming wait, cancel, or cleanup event.
They contain safe timing, status, byte-count, and structured error-kind fields
only; a consuming wait, cancel, lexical cleanup, or shutdown cancellation is
recorded separately. Request and response bodies, headers, query strings, credentials, and
filesystem contents are never retained for tracing.

Both client policies use Rustls with the Rust-only Graviola `CryptoProvider`,
keyed by caller-visible pool name and TLS configuration. On Linux, certificate
validation reads the `SSL_CERT_FILE` and `SSL_CERT_DIR` overrides when set;
otherwise it loads PEM roots from standard locations including `/etc/ssl/certs`.
On macOS, the platform verifier remains a target-specific dependency so Keychain
trust evaluation is preserved; it is not the TLS `CryptoProvider` and does not
add a C/C++ build dependency. HTTP hostnames are resolved asynchronously by the
XSH dialer through the platform resolver. XSH supplies h12tiny's TCP dialer hook
with explicitly resolved nonblocking sockets; h12tiny retains TLS, ALPN,
protocol selection, handshakes, and pooling. The `dns` module's explicit lookup
APIs retain their separate resolver and timeout behavior.

`net.request` and `net.start` accept a request record with `method: Str`, `url: Str`,
`headers: List[Record]`, optional `body: Bytes`, `body_text: Str`, or
`body_file: Path`, `pool: Str`, `timeout: Duration`, `dns_timeout: Duration`,
`connect_timeout: Duration`, `tls_timeout: Duration`, `headers_timeout:
Duration`, `body_idle_timeout: Duration`, `redirects: Int`, `tls_verify: Bool`,
`ca_certificate: Path`, `fail_status: Bool`, and `max_body_bytes: Int`. The same
deadline fields apply to `net.download`, `net.upload`, and each item in a batch.
Methods are `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, and `DELETE`.

`net.request` and `NetJob.wait()` return `status: Int`, `reason: Str`, `bytes: Int`, `headers:
List[Record]`, `url: Str`, and `body: Bytes`. `net.download` and `net.upload`
return the same metadata without `body`; downloads write through a temporary
file and rename atomically by default. Unsupported schemes, invalid URLs,
unsupported methods, TLS failures, DNS failures, redirects, timeouts, status
failures when `fail_status` is true, and response-size limits use structured
error kinds.

`net.request_many` executes a bounded batch through its evaluator runtime. It
uses HTTP/2 when HTTPS ALPN negotiates `h2`, otherwise HTTP/1.1. Canceling one
active HTTP/2 job resets only that stream: healthy sibling streams and the
pooled connection remain usable. Its batch record takes
`requests: List[Record]`, optional `concurrency: Int = 16`, and the same
`pool`, TLS, and CA fields accepted by `net.request`; those options apply to
every request in the batch. Timing fields belong to the individual request
records, so an inactive item does not start its total deadline. Connections are
retained by the pool across batches.
It preserves request order and returns one inner `Result` per request, allowing
independent failures without exposing futures, callbacks, or `await`.

`net.download_many` takes `downloads: List[Record]`, optional `concurrency: Int
= 16`, and the same outer pool, TLS, and CA fields. Each download record uses
the `net.download` fields and streams its response directly to its atomic
destination; it never materializes a response body in XSH memory. It preserves
input order and returns one inner `Result` per download.

`timeout` is the total deadline from runtime admission through file preparation,
queueing, transport, redirects, response transfer, and finalization. It uses
`net-timeout`; it wins when it expires at the same instant as a phase deadline.
`dns_timeout` limits one DNS resolution and uses `net-dns-timeout`.
`connect_timeout` limits aggregate TCP establishment across all resolved
addresses and uses `net-connect-timeout`. `tls_timeout` limits TLS plus ALPN
and uses `net-tls-timeout`. `headers_timeout` covers request dispatch through
response headers, restarts on every redirect hop, and uses
`net-headers-timeout`. Reused pooled connections skip DNS, TCP, and TLS phases.
`body_idle_timeout` begins after response headers and applies to each network
body-frame wait; it resets after each frame and uses `net-body-idle-timeout`.
Time waiting for a local download file write is outside that idle window, while
the total deadline continues. User and evaluator cancellation use
`net-canceled`, never a timeout error.

List values expose collection operations as methods:

- `.len() -> Int`.
- `.push(item: T) -> List[T]`.
- `.extend(more: List[T]) -> List[T]`.
- `.contains(item: T) -> Bool`.
- `.get(index: Int) -> Result[T]`.
- `.join(separator: Str = "") -> Str` (only when `T` is `Str`).

`map`:

- `map.empty() -> Map[K, V]`; empty maps need an expected type from a
  binding annotation or later typed API boundary. In those map-typed contexts,
  `{}` is equivalent to `map.empty()`.

List and Map `.get` have one Result-returning lookup form. Missing entries
retain their typed lookup errors unless the caller uses `?` or `??`; a present
null value returns `Ok(null)` and does not invoke a Result fallback. The removed
two-argument fallback overloads evaluated their fallback eagerly. Migration to
`??` is automatic only for inert non-failing fallback expressions; effectful
fallbacks need explicit snapshots that retain receiver, index/key, and fallback
evaluation order. Str/Bytes byte access and Str search use null for ordinary
absence, with no configurable fallback argument. Legacy numeric policy may be
spelled `find(...) ?? -1`; nullable arithmetic remains invalid.

Map values expose routine methods with receiver-bound key and value types:

- `.len() -> Int`.
- `.has(key: K) -> Bool`.
- `.get(key: K) -> Result[V]`.
- `.set(key: K, value: V) -> Map[K, V]`.
- `.push(key: K, value: T) -> Map[K, List[T]]` for list-valued receivers;
  missing keys are created with a singleton list.
- `.remove(key: K) -> Map[K, V]`.
- `.keys() -> List[K]` and `.values() -> List[V]`, both in canonical key order.

Lookup uses borrowed scalar views. Updates retain source evaluation order and
copy storage only when shared. JSON objects and environment names require Str
keys; non-string maps must be converted explicitly by the application before
crossing those boundaries. Numeric keys can change traversal order relative to
encoded decimal strings, so textual-key migration is never a general autofix.

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
- `.lines() -> Stream[Str]`. Lines are separated by `\n`; a terminal newline
  terminates the final line but does not produce an empty final element. To
  reassemble a newline-terminated value with one `\n` per input line, join the
  collected lines with `"\n"` and append one final `"\n"` (for example,
  `f"${text.lines().collect().join("\n")}\n"`).
- `.words() -> List[Str]`, split with Unicode whitespace semantics.
- `.split(separator: Str, maxsplit: Int = -1) -> List[Str]`; an empty separator
  splits into Unicode scalar values. A negative `maxsplit` performs unlimited
  splits, zero performs no splits, and a positive value limits the number of
  separators consumed. The final item contains the unsplit remainder.
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
- `.byte_len() -> Int`, counting UTF-8 bytes independently of Unicode scalar counts.
- `.byte_at(index: Int) -> Int?`, returning the byte value at a nonnegative
  byte index, or null when out of range. Negative indices do not count from the end.
- `.byte_slice(offset: Int, length: Int = rest) -> Str`, slicing by byte offset
  and length.
- `.find(needle: Str, start: Int = 0) -> Int?`, returning the byte offset of
  `needle` at or after byte index `start`, or null when missing or start is
  negative or exceeds the byte length. An empty needle matches at any start
  from zero through the byte length, including the end; starts may address
  individual UTF-8 bytes. Subsequent byte slicing retains its UTF-8 boundary checks.
- `.parse_int() -> Result[Int]`, accepting decimal, `0x` hexadecimal, `0o`
  octal, `0b` binary, `_` separators, and an optional leading sign.
- `.parse_int_decimal() -> Result[Int]`, accepting only nonempty decimal
  digits without leading zeros (except `0`); whitespace, signs, radix
  prefixes, separators, and out-of-range values return an error.
- `.parse_uint() -> Result[Int]`, trimming surrounding whitespace and accepting
  only non-negative decimal digits; signs, radix prefixes, malformed, and
  out-of-range text return an error.
- `.parse_uint_positive() -> Result[Int]`, trimming surrounding whitespace and
  accepting only positive decimal digits; zero, signs, radix prefixes,
  malformed, and out-of-range text return an error.

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
`\P{...}` are outside the v1 surface. Dynamic compile errors return structured
regex compile errors from `regex.compile(...)`; literal errors occur at checked
preparation. A `Regex` value has already validated its pattern, so its methods return plain values instead of `Result`. `captures`
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

- `.lines() -> Stream[Bytes]` splits on `\n`, drops a trailing `\r`, and does
  not emit an empty final element for a terminal newline, like `Str.lines()`.
- `.count_lines() -> Int` counts those lines without allocating them.
- `.trim() -> Bytes` removes leading and trailing whitespace, matching
  `Str.trim()`'s Unicode `White_Space` semantics on valid UTF-8.
- `.starts_with(prefix: Bytes) -> Bool`, `.ends_with(suffix: Bytes) -> Bool`,
  and `.contains(needle: Bytes) -> Bool` are byte searches.
- `.lower() -> Bytes` lowercases ASCII bytes only, leaving other bytes intact.
- `.byte_at(index: Int) -> Int?` returns the byte value at
  a nonnegative `index`, or null when out of range. Negative indices do not
  count from the end; byte access does not decode UTF-8.

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
- Path overloads read files incrementally, so hashing a file does not retain
  its full contents in memory. Bytes overloads hash the supplied value.
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
In real mode, mount syscall failures from `linux.mount` or `linux.mount_all`
return `Err` with kind `linux-mount`, so scripts can inspect or recover from
them. A failure reading `/etc/fstab` in `linux.mount_all` has kind
`linux-mount-all`.

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
- `linux.network_dump() -> Result[LinuxNetworkDump]`, collecting links,
  addresses, routes, and policy rules through read-only `NETLINK_ROUTE` dumps.
  Its typed records preserve each raw attribute as `LinuxNetlinkAttribute`
  bytes, including attributes unknown to this version. Link names also retain
  their original bytes when they cannot be represented as UTF-8. A malformed
  object is skipped with a `LinuxNetworkIssue`; dump-level framing, kernel,
  sender, sequence, port, interruption, truncation, and limit failures are
  represented in the result state and issue list while other dump groups are
  still attempted. A nonempty dump status shorter than four bytes or a positive
  netlink error code is malformed. `enumeration_succeeded` requires all four
  dump groups to finish and every returned entity to decode. Individual field
  issues can still make `state` partial without losing enumeration success.
  Interrupted dumps are
  retried once. Each request has a
  three-second deadline and limits of 8 MiB, 65,536 messages, and 4,096
  datagrams. Interface indices are scoped to the current network namespace and
  can change when a link is recreated. This API is the complete routing view;
  `linux.routes()` remains a compatibility projection over the procfs route
  files. Multipath route attributes retain per-nexthop interface, flags, raw
  hop weight, and gateway data. A route's single output interface remains a
  separate field and does not become a fabricated multipath entry.
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
  env: Record = default, stdin: Path | Bytes = default, stdout: Path = default,
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
  overflowing. Use checked multiplication by `1ms` or `1s` when a nonnegative
  runtime count should fail on overflow rather than clamp or saturate.
- `time.measure(command: Command, quiet: Bool = false) -> Result[Record]`, returning
  `{status: Status, duration_ms: Int, wall_ns: Int, user_ns: Int, system_ns: Int}`.
  `wall_ns` is nanosecond wall-clock time; `user_ns`/`system_ns` are the child's
  user/system CPU time; `duration_ms` is `wall_ns / 1_000_000` (kept for
  compatibility). With `quiet: true` the child's stdout/stderr go to `/dev/null`.
- XSH deliberately has no general civil-timestamp formatter. Invoke the host
  `date` command when locale or timezone formatting is required.
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
- `system.execution_units() -> Result[SystemExecutionUnits]`, returning the
  actual `page_size_bytes` and `clock_ticks_per_second` values for the current
  process environment. Use these values when converting procfs page and tick
  counters.
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
For a live source, `count()` drains a contiguous prefix of `tee`, `where`,
`map`, `flat-map`, `drop`, and `enumerate` stages without retaining their rows.

**For loops.** `for x in PIPELINE { }` passes rows from serial stream stages
directly to the loop body without materializing a `List`. Stages whose contract
requires ordering, grouping, or parallel work may buffer their input. This is
the preferred form when items are consumed once and the list is not needed.

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
rather than a stream stage. A bare method name uses the previous value as its
receiver (`value |> split(",")` is the same call shape as
`value.split(",")`). A qualified function call uses the previous value as its
first argument. The stage may end in `?`, which propagates a `Result` returned
by that call. Stages without an argument hole keep this call and receiver
insertion behavior.

An immediate ordinary call may contain exactly one `_` as a whole positional
argument or named argument value. `data |> render(template, data: _)` performs
an ordinary call to `render` with the input at that position, without additional
receiver or first-argument insertion. A bare callable with a hole resolves as an
ordinary function, even when its name is also a method name. Parentheses around
a whole hole normalize to `_`; nested, embedded, spread, multiple, or free holes
are rejected. Discard bindings, wildcard patterns, and discard block parameters
retain their existing meanings.

Input evaluation completes once before the stage's callee/receiver and other
arguments. Those arguments retain ordinary source order. Optional calls still
skip their other arguments when the receiver is absent, after the input has
already been evaluated. Explicit `?` keeps its normal propagation boundary.
Recognized structured stage names keep their stage dispatch: holes do not add
per-item mapping, collection, or implicit unwrapping. Use an ordinary `map` block
for per-item work. Qualified stages without a hole still receive the previous
value as their first argument:

```xsh
let readme_text = p"README.md".read_bytes()?.utf8()?

let warnings = fs.read_text(p"build.log")?
  |> text.lines()
  |> where { "warn" in . }
```

Accepted **transformation** stage kinds include `where`, `map`, `par-map`,
`batch`, `sort`, `sort-by`, `take`, `drop`, `unique-by`, `enumerate`,
`zip`, `range`, `repeat`, `tee`, and `flat-map`. These produce a stream.

Accepted **terminal** stage kinds: `count()`, `collect()`, `sum()`, `min()`,
`max()`, `first()`, `last()`, `any`, `all`, `fold(init) { ... }`, `reduce`,
`shuffle`, `group-by`, `each`, and `table.print(...)`. These produce a scalar,
materialized list, or consume the stream.

`each` is for effects and accepts a block whose result is `Unit` or
`Result[Unit]`. A pipeline that ends in a `Unit`-valued terminal stage such as
`each` or `table.print(...)` evaluates to `Unit`, so it may be a procedure's
final statement and is accepted by both `xsht check` and the runtime. `map`
and `par-map` are for values and require a final
expression or command tail value. An `if`/`else` expression is valid in that
position, including as a direct conditional tail without an intermediate
binding. Stage blocks may bind one explicit item
parameter with `{ |item| ... }`, but the implicit `.` item is available in
one-expression and multi-statement stage blocks. `fold`/`reduce` additionally
accept a two-parameter block `{ |acc, item| ... }` whose first parameter is the
accumulated value (typed by the initial value) and whose second is the stream
item. These blocks may contain ordinary statements and nested conditionals;
their tail must produce the accumulator's type, and the stage returns that
accumulated value. Fold and reduce blocks run serially, including their effects:
the block for one item finishes before the next live item is pulled. A block
may print or run a process directly; its defers run at that item's block exit.
Use `each` when no accumulated value is needed.

`map`, `where`, `flat-map`, `each`, `tee`, `sort-by`, `group-by`,
`unique-by`, `any`, and `all` also accept a statically resolved named
callable: `map(normalize_name)`, `where(block: is_valid)`, or
`sort-by(key, desc: true)`. The descriptor occupies the ordinary `block`
argument slot and cannot accompany an explicit block. Configuration arguments
retain their existing named argument, pun, and static record spread rules.
The callable itself must retain a direct declaration identity; a function
value projected from a record spread is not a static descriptor.

The descriptor means the same checked ordinary one-item call as
`map { |item| normalize_name(item) }`. Qualified imported functions and
registered standard calls are accepted when that call selects one signature.
Parameter conversions, supported defaults, effects, and source spans follow
ordinary calls. Defaults are supplied for each actual call, including fresh
aggregate defaults; an empty source makes no calls. Erased `Any`, `Pure`, or
`Proc` values, callable-producing expressions, bound methods, ambiguous
overloads, and `_` placeholders require an explicit block. Other stage bodies
retain their existing arity and block syntax.

A Bool predicate remains data. A Result-returning `map` produces
`List[Result[T, E]]`; it does not propagate errors per item. Side-effecting
`each` and `tee` retain their Unit-consuming automatic propagation. A block
ending in `f(item)?` remains explicit because it selects a different error
policy. `lint.stage-callable` only rewrites a checked single-call wrapper with
its exact bound item and no comments, extra arguments, cleanup, or propagation.

A tail proc call with `?` unwraps the `Ok` value and propagates errors: if
any item fails, the entire stage short-circuits with that error. Without
`?`, the `Result` value flows through as-is — errors stay in-band as
`Result::Err` values in the output stream, and all items are processed.
This lets the caller choose between short-circuit semantics (use `?`) and
collect-all semantics (omit `?`), matching how Rust's rayon, Go's
goroutines, and Haskell's `parMap` separate parallelism from error
handling. The unwrapped value may be used inline as a receiver in the same
expression, for example `map { |s| (s.split(".") |> last())?.lower() }`.

`where`, `any`, and `all` require `Bool` or `Result[Bool]`. `min()` and
`max()` return `Result[T]`. `first()` and `last()` return `Result[T]`.
Direct `any` and `all` stages over a live source stop after the first decisive
item, including block forms, and close a stopped script producer so its defers
run before the stage returns.
`count()` returns `Int`. `table.print(...)` is a structured stream sink for
record streams. It renders terminal-width UTF-8 tables by default and wraps
long cell contents vertically instead of truncating with ellipses.

`flat-map` accepts `List[T]` or `Stream[T]` from its block. A live stream
returned by a block is drained for that input item before the outer stream
continues.

`fs.children(...) |> table.print(...)` is the accepted standard listing interface.

Structured stage configuration uses ordinary named arguments, including
punning and statically checked record spreading. Option labels use snake_case:
`par-map(jobs:)`, `sort-by(desc: true) .size`, and
`batch(count: 2, max_bytes: 4096, max_argv: true)`. External argv and `run`
options retain their command syntax. Stage flags such as `--jobs` are migration
errors; tooling can replace a diagnosed flag range when comments and argument
order are preserved.

`xsh_registry::stream_parameters::stage_parameters` owns each accepted label, type, default,
and validation. No other stage accepts these configuration parameters.
`reduce-by` requires exactly one enabled Bool among `sum`, `min`, and `max`.
`batch` requires at least one enabled count or byte limit; combined limits close
a batch at the first reached bound and retain a final short batch. Disabled
`max_argv: false` adds no limit. Batch `count` and `max_bytes`, chunk `size`, and
`jobs` must be positive; take/drop/repeat counts may be zero.

Configuration arguments are evaluated once, in their written order, at the
stage's established entry boundary. They do not run once per item. In
particular, preceding serial stage effects finish before a downstream worker
configuration runs; a direct worker configuration runs before source pulls.
`sort-by` checks its direction before pulling input; `sort` checks its direction
after materializing input. Nameable positional roles retain their stage timing:
`count`, range `start`/`end`, chunk `size`, zip `other`, fold/reduce `init`, shuffle
`seed`, and table `columns`. Inline projections and block parameters retain
their per-item roles.

`par-map` defaults to a bounded worker count based on available CPUs. Use
`par-map(jobs: N)` to override the worker count. `each` runs serially and does not
accept `jobs:`; it does not emit parallel-job trace events.
Every accepted `jobs:` expression runs once before its stage consumes input,
and its result must be positive. Explicit bounded parallel stage limits must be
positive. `group-by` and both forms of `count` also reject `jobs:` because
their indexed handlers run serially.

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
stages. `reduce-by` folds a live source one item at a time before pulling the
next item and closes that source if reduction fails. Its `jobs:` parameter is
currently accepted but does not start reduce workers; the indexed fold is serial.
An explicit `reduce-by(jobs: ...)` prevents adjacent `par-map` fusion so its option
expression runs at the reduction stage.
`fold` also combines each live item before pulling the next and closes the
source if the combine fails. `each` runs its body before pulling the next live
item and closes the source when the body fails. `group-by` and keyed
`count { block }` evaluate each item's key before pulling the next live item;
a key error closes the source without evaluating later items. `unique-by` has
the same live key timing and keeps the first item for each distinct key.
`group-by` retains the items in encounter order within each group, while keyed
`count` retains one count per distinct key.
`zip(other)` evaluates and collects its right list or stream before pulling the
left source, then pairs one left item with each right item until either side
ends. When the right side ends first, it closes a live left producer without
pulling later items.
`batch(max_bytes: N)` checks each live item as it arrives. An item larger than
the byte budget fails the stage, closes the producer, and leaves later items
unpulled.
`repeat(0)` produces an empty list without pulling a live source.

`sort` and `sort-by` order by a defined key ordering. Supported items and
projected keys are `Int`, `Str`, `Bool`, `Path`, and `Record`s whose fields are
themselves supported (recursively). Statically-`Any`/unknown keys (for example
an `Any`-typed record field produced by `Map.get(key) ?? fallback` on a
`Map[Any]`) are also accepted because the runtime sorts the actual supported
scalar value; such keys fail loudly at runtime only when the actual value is
not orderable. Records compare field by field in sorted
field-name order, so `sort-by { |r| {c: r.count, n: r.name} }` sorts by `count`
then `name`. A `group-by` result exposes its projected key as the concrete type
of the grouping block, so `group-by { |x| x.id } |> sort-by { |g| g.key }` is
valid when that key is sortable. The default order is ascending and `desc: true`
reverses it. `sort-by(desc: expr)` evaluates the option before pulling its
source or projecting keys, so an option error stops before those effects.
Both stages are stable: items with equal keys keep their source
order, so sorting by
a secondary key first and the primary key second is a reliable two-pass idiom
for compound ordering. Any other item or key type is rejected at check time and
fails at runtime with a diagnostic naming the stage and the offending type
rather than silently returning unsorted input.

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

`process.command { ... }` accepts `cwd: Path`, `env: Record`, `stdin: Path | Bytes`,
`stdout: Path`, `stderr: Path`, `stdout_append: Bool`,
`stderr_append: Bool`, `timeout: Duration`, `cpu_max: Int`, `detach: Bool`,
`new_session: Bool`, `ignore_hup: Bool`, and exactly one plain `run` entry. It
captures a typed process plan without executing it. `process.command_argv`
builds the same typed plan from data; its `argv` list is the full argv vector
and must include `argv[0]`, the child program name. `argv[0]` may be a custom
name; XSH resolves `target` as the executable and passes the remaining argv
items as process arguments. An empty argv list is a checker diagnostic when
known statically and a runtime error otherwise. `process.run` executes a command
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
- `Optional[T]` when `T` is JSON-compatible; a missing value encodes as `null`
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
lookup failure. A dynamic non-list path fails the `List[Any]` argument boundary
with `type-error` before traversal and does not select the fallback.
`json.set` updates existing list indexes and updates or inserts
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
- `xsht -h` or `xsht --help`.
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
- `xsht api [OPTIONS] QUERY...`.

`xsht --help`, `xsht help`, and the no-argument form render the generated
hybrid command reference: a task-oriented command index followed by the usage
and options for every subcommand. `xsht COMMAND --help` and `xsht help
COMMAND` render the same metadata for one command. Structural-search examples
are part of the `grep` section.

For `xsh`, `--` separates the interpreter's options and script path from the
script's own arguments. Both `xsh SCRIPT -- ARGS...` and the shebang-compatible
`xsh -- SCRIPT ARGS...` forms are accepted; the separator is optional when the
script path is unambiguous.

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
directories from `xsht-config.ini`. An explicit directory checks only files
under that directory; configured `include` roots do not extend it. Directory
traversal uses the nearest config's `exclude` patterns.

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
parse, resolve imports, and format without introducing new checker diagnostics;
pre-existing checker diagnostics may remain.

`xsht lint` reports `lint.dead-code` for statements that follow an unconditional
`return`, `break`, or `continue`, or follow a conditional or match statement
whose reachable branches all return. The detector continues to analyze the
unreachable statement so other diagnostics remain visible, but it does not
rewrite code automatically.
It removes provably needless local binding annotations and rewrites simple
`.contains(value)` membership or substring checks to `value in receiver` (or
`value not in receiver` for negated calls) when the checker proves that
membership syntax has the same semantics and the rewrite does not move
effectful expressions.

`lint.prefer-value-pipeline` offers explicit argument placement for a checked
nested call at a whole statement value or a single-use linear temporary chain.
It requires stable reads before any moved input and a full-source recheck with
unchanged concrete input and result types. Guarded receivers, spreads, ambiguous
or contextual types, and changing mutable reads keep their ordinary calls.
Comments in the proposed edit produce a warning without a fix.

`lint.prefer-guard` rewrites a single-action `if` without `else` to a guarded
`return`, `break`, `continue`, or `yield`. It preserves condition-first payload
laziness and groups run payloads. Comments, multiple actions, multiline payloads,
and long proposed one-liners keep their readable blocks.

`lint.prefer-inferred-pure-return` is opt-in with
`[lint] prefer-inferred-pure-returns = true` in `xsht-config.ini`. It removes a
private pure return annotation only after a complete source recheck preserves
all function and caller types, expression types, effects, and statement/value
classifications. Named schema constraints and unresolved imported contexts
receive no fix. The rule is disabled when configured `check.annotate` includes
`returns` (directly or through `default`/`signatures`/`all`), preserving annotation
tooling round trips. `xsht check --annotate=returns` renders inferred private
pure returns as well as defaulted exported proc returns.

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
before each declared test runs.

Native tests are top-level `test NAME [effects]? { ... }` declarations with a
`Result[Unit]` body contract. The ordinary inside-brace header may bind one
immutable `TestContext` parameter (`{ |ctx| ... }`) or discard it (`{ |_| ... }`);
zero parameters are also valid. Omitted effects retain unrestricted proc behavior;
explicit effects are checked normally. Declared names need no `test_` prefix.
Names share the top-level namespace, and duplicates or collisions are errors.
Declarations are checked and registered without executing their bodies. They
cannot be called, exported, or nested, and importing a module or running a script
does not execute them, including a declaration named `main`. Qualified `test.*`
operations remain ordinary module access. Builds without `native-tests` diagnose
declarations with instructions to use a build that supports native tests.

The harness discovers declarations only within its existing configured roots.
Legacy `proc test_*` harness signatures produce migration diagnostics rather than
prefix-based discovery; preserve the exact old name when converting a signature.
If callers use a legacy test proc, extract the callable work into an ordinary
helper and retain one declared test. Ordinary helper procs remain valid. `xsht lint --fix` offers a checked migration
for private, unreferenced legacy signatures in configured test roots; callers or
signature comments require manual conversion.
Each test runs in a fresh evaluator with fresh stdout and stderr capture, cwd/env
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

### Removed compatibility vocabulary

Only the former predeclared `ARGV`, `run.builtin*` qualifier, ambient `fs.ls`,
and Str `.count_bytes()` are removed. Canonical spellings are `args`, the
corresponding `run*` form, `fs.children`, and Str `.byte_len()`. Rooted filesystem
operations, recursive walks/files, character counts, and text stream stages keep
their distinct contracts. Public API inventories and dispatch expose canonical
operations only; runtime error kinds remain unchanged. Operation names in source
attribution and traces now use the canonical spelling.

Ordinary preparation rejects removed vocabulary before effects. Tooling retains
fatal `parse.compatibility-vocabulary` / `check.compatibility-vocabulary` recovery
facts solely to offer `lint.compatibility-vocabulary`. Fixes require resolved
standard names or a checked Str receiver, keep comments, strings, external argv,
environment names and serialized field keys intact, and normally parse/check the
complete rewritten import graph before writing. A user `ARGV` binding or user
`count_bytes` member is untouched; a shadowed canonical `args` target receives no
fix. Standard API members used as unsupported first-class values also receive
no fix. Record shorthand migration retains its original wire key. Unrelated errors
and unsupported argument contracts remain errors.
