# XSH Language Reference

This is the contract for XSH: what a script author can rely on and what an
implementation must preserve. It describes observable behavior, not how the
current implementation achieves it. When behavior changes, this file changes
first or in the same change.

Related documents:

- `docs/user-tour.md` explains why XSH exists and teaches it by example; read it first.
- `docs/SPEC-INTERACTIVE.md` is the complete contract for the interactive
  shell `xshi`. Nothing in `xshi` changes how `.xsh` files parse, check, or run.
- `docs/reference/stdlib.md` is the generated index of standard modules,
  methods, and records. `xsht api` answers the same questions with full
  signatures and details.

Diagnostic codes such as `check.bool-statement` are quoted where they help
locate a rule; `xsht lint` and `xsht check` report the complete set.

## 1. Design

XSH is a language for system glue: scripts that orchestrate processes, files,
paths, byte streams, structured data, and host state. It keeps the shell's
composition model and replaces its semantics.

- **Explicit boundaries.** Nothing crosses into a subprocess, the filesystem,
  or the environment without a visible form. `run` starts a process, `@xs`
  splices a list into argv, `${expr}` interpolates one value, `?` propagates an
  error. Command words are never split, globbed, or expanded.
- **Types without ceremony.** Every value has a type. Locals infer their types;
  annotations appear at module, function, and data boundaries.
- **Results, not exceptions.** Fallibility is part of a signature. A failure
  can leave a function only through a visible `?`, a statement-position
  `Result[Unit]`, an `assert`, or a failed plain `run`.
- **Effects are tracked.** Pure functions cannot touch the host. Procs may
  declare which host effects they use, and private procs have their effects
  inferred.
- **Predictable execution.** Evaluation order is source order. The concurrency
  units are host processes and bounded stream stages; there are no futures,
  callbacks, event loops, or green threads.
- **Methods first.** Operations on a value are methods (`xs.len()`,
  `p.read_text()`). Module functions exist where there is no natural receiver,
  such as `map.empty()` or `regex.compile(...)`.
- **Greppable.** No sigil changes meaning by context, and there are no
  implicit coercions. A call to `f` is found by searching for `f(`.
- **Traceable.** Every run can produce a structured trace of processes, scopes,
  calls, and failures anchored to source spans.

## 2. Source And Lexical Structure

### 2.1 Source files and diagnostics

Source files are UTF-8; invalid UTF-8 is a source-loading error. `\n` and
`\r\n` both end a line. Spans are byte ranges; diagnostics render one-based
line and column numbers, with columns counted in Unicode scalar values.
Every diagnostic from loading, parsing, checking, linting, or evaluation
carries a source span when one exists, and human and machine output render the
same diagnostic value.

### 2.2 Comments and documentation

`#` starts a comment that runs to the end of the line, outside strings. A
comment may stand on its own line or follow a complete statement.

`##!` starts a module documentation block and `##` starts a declaration
documentation block; consecutive lines with the same prefix form one block. A
module that exports anything must begin with one `##!` block, and every
`export` must be immediately preceded by a `##` block. A `##` block that does
not attach to an export, or a second `##!` block, is an error.

### 2.3 Keywords

Reserved keywords:

```text
and assert break const continue defer else enum export false for guard if in
let loop match not null or proc pure retry return run spawn stream true try
type unless use var wait when while with yield
```

`not` appears only in the binary operator `not in`; unary negation is `!`.

Contextual words keep their special meaning only in their syntactic position
and remain ordinary identifiers elsewhere: `as`, `cli`, `ctx`, `error`, `is`,
`on`, `test`, the core commands `print`, `eprint`, `cd`, and `env`, and builder
entries such as `run` inside a builder block.

Builtin type and constructor names:

```text
Any Bool Bytes Command Digest Duration Err Error Float Int List Map Module Null
Ok Path ProcessError ProcessHandle Proc Pure Record Regex Result Status Str
Stream UInt Unit
```

### 2.4 Identifiers and reserved names

Expression identifiers match `[A-Za-z_][A-Za-z0-9_]*`. Proc names and module
path segments may also contain `-` after the first character. A hyphenated
final module segment requires an `as` alias (`check.hyphenated-module-alias`).

Standard module names are reserved in binding and alias positions, so a module
namespace cannot be shadowed. Two names are exceptions:

- `args` is the predeclared script-argument list. Nested scopes may declare
  their own `args`; the root binding stays the script arguments.
- `error` is the conventional name for an error payload (`Err(error)`), so it
  may be bound anywhere. Unshadowed `error.fail(...)` still names the module.

Record destructuring may bind fields named like modules, because standard
records commonly have fields such as `path`.

Field labels in record literals, schemas, member access, patterns, and named
arguments may be keywords (`{type: "file"}`, `entry.type`). Such labels need an
explicit value or a renamed binding; keywords never become variables through
shorthand or punning.

### 2.5 Statement terminators and line continuation

A newline or `;` ends a statement. A newline continues the current expression
only when the next line begins with a token that cannot start an expression:

```text
|>  and  or  *  /  %  ==  !=  <  <=  >  >=  in  not in  ??  .
```

The final `.` means a line that begins with a method call or field access. A
line beginning with `-` starts a new statement, because `-x` is a valid
expression. A line that ends with a binary operator, or inside an open
bracket, always continues; so to split around `+` or `-`, leave the operator at
the end of the first line.

```xsh
let files = fs.files(p"src")?
  |> where .ext == "rs"
  |> sort-by .path

let ready = config.enabled
  and target.exists()?
```

### 2.6 Literals

**Strings.** `"..."` supports the escapes `\\`, `\"`, `\$`, `\n`, `\r`,
`\t`, `\0`, `\xNN`, and `\u{HEX}`, and must decode to valid UTF-8. Raw strings
`r"..."` decode no escapes. Expression strings never interpolate: `"$name"` is
literal text (`lint.dollar-in-expression-string` warns when it names a
binding).

**Display strings.** `f"..."` interpolates `${expr}` and the shorthand
`$name` / `$name.field`, using display conversion. Interpolation scans
balanced brackets and nested strings, so `f"${if ok { "yes" } else { "no" }}"`
works. `\${` writes a literal `${`.

**Triple-quoted strings.** `"""..."""`, `r"""..."""`, and `f"""..."""` may
span lines. When the opening delimiter is immediately followed by a line break
and the closing delimiter stands alone on its line, the literal uses block
layout: the opening and closing line breaks are removed, and the closing
delimiter's exact indentation is removed from every nonblank content line. A
content line without that indentation is an error. Whitespace-only lines lose
as much of the margin as they have. Internal line endings are kept exactly;
there is no implicit trailing newline. Layout is removed before escapes and
interpolation, and inserted values are never reindented. Any other triple
literal keeps its text exactly.

```xsh
let unit = f"""
  [Unit]
  Description=${name}
  """
```

**Bytes.** `b"..."` produces `Bytes` and supports the string escapes except
`\u{...}`.

**Regex.** `rx"..."` and `rx"""..."""` produce a compiled `Regex`. Contents are
passed to the regex engine unchanged (no escapes, no interpolation). Each
literal is validated when the program is prepared, including unreachable ones,
so an invalid pattern is a check-time error. Use `regex.compile(text)` for
patterns built at runtime.

**Paths.** `p"..."` produces a `Path` and supports string escapes but no
interpolation; an unescaped `${` in a `p` literal is an error. `fp"..."`
interpolates: `Path` fragments contribute their native bytes and other values
their UTF-8 display text. A token that begins with `/`, `./`, or `../` and
contains no whitespace or delimiters is also a path literal:

```xsh
let cc = /usr/bin/cc
let out = ./target/build
```

**Globs.** `g"..."` expands against the current directory and produces
`List[Path]`. Globbing is a filesystem effect and is not allowed in pure
functions. Nothing else globs.

**Numbers.** Integer literals are decimal or octal (`0o755`); a leading `-` is
unary minus. Float literals need a digit after the decimal point or an
exponent: `1.0`, `0.25`, `1.5e6`, `10e-3`.

**Durations.** A decimal integer followed immediately by `ms`, `s`, `m`, or
`h`: `250ms`, `30s`, `2h`.

## 3. Programs And Modules

### 3.1 Scripts

A script's top-level statements run in order. Top-level code may run commands,
mutate `var` bindings, and use control flow. `return` outside a callable is an
error (`check.return-outside-callable`).

A script's exit status is chosen as follows:

- A final top-level `Int` or `UInt` value in `0..=255` becomes the exit status.
- `abort(status)` exits immediately with `status` (see 8.9).
- A top-level `?` on `Err`, or any uncaught runtime failure, prints a traceback
  and exits `3`.
- Otherwise the script exits `0`.

### 3.2 Script arguments, `main`, and `cli main`

Script arguments are available as the immutable `args: List[Str]`. The `xsh`,
`xshi`, and `xsht` command lines reject arguments that are not valid UTF-8
with status `2` before loading anything; `Path` values built inside a script
can still carry arbitrary bytes to child processes.

If the entry script defines a top-level `proc main` and the last top-level
statement does not call it, `main` is called with the script arguments after
all other top-level statements finish. Arguments bind positionally; a rest
parameter collects the remainder, so the usual form is
`proc main(...argv: List[Str])`. A `main` whose required parameter is neither
`Str` nor `Path` can never bind an argument and is reported at check time.

`cli main(parameters) [effects] -> Return { ... }` declares a typed command-line
entry instead:

```xsh
## Copy a build artifact.
cli main(source: Path, dest: Path, jobs: UInt = 4, verbose: Bool = false) {
  print f"copying ${source} with ${jobs} jobs"
}
```

- Required parameters are positional operands, in order. Parameters with
  defaults are options named in kebab case (`dry_run` becomes `--dry-run`). A
  final rest parameter of type `List[Str]` or `List[Path]` receives the
  remaining operands. Required parameters come before defaulted ones.
- Parameter types may be `Str`, `Int`, `UInt`, `Bool`, `Path`, or `Duration`,
  or aliases of them. `Bool` options accept a bare switch or an explicit value.
  Defaulted `List` options append repeated occurrences.
- Defaults must be constants; reading them runs no code.
- `-h` and `--help` print generated help (types, defaults, and doc comments)
  and exit `0`. Unknown or duplicate options and malformed values print usage
  and exit `2`. Both happen before any top-level statement or module
  initializer runs.
- `cli main` is not callable or exportable, appears only at the entry script's
  top level, and cannot coexist with `proc main`.

For subcommands, dynamic schemas, or other advanced policies, use `cli.parse`,
`cli.parse_full`, and `cli.commands`. A `cli.parse` error propagated with `?`
at the top level prints usage and exits `2` (or `0` for help) without a
traceback.

### 3.3 Modules

`use name` imports a module and binds exactly one namespace, `name`;
`use name as alias` binds `alias`. Standard modules are always available
without `use` and cannot be aliased. User modules resolve relative to the
importing file, then through each directory in `XSH_MODULE_PATH` (separated by
the platform's path-list separator). Dotted paths name subdirectories.

A module's top level may contain only `use`, `const`, `let`, `proc`, `pure`,
`stream`, `type`, `enum`, and `error` declarations, optionally exported. It may
not run commands, mutate state, or use control flow. Exports are reached only
through the namespace (`helper.build(...)`, `helper.Config`,
`helper.ConfigError.Missing(...)`); importing never injects names. Types and
error families are compile-time names, not runtime fields of the module value.
An exported type keeps the meaning it has in its declaring module, including
references to that module's private types.

### 3.4 Constants

`const NAME = expr` declares prepared, immutable data. The initializer may use
literals, other constants (including exported constants of imported modules),
record, list, map, and tag constructors over constants, and primitive
operators. It may not use runtime bindings, calls, methods, blocks,
comprehensions, or propagation. Cycles are errors, and invalid arithmetic is
reported at check time. Empty containers need a type annotation; `Any` and
resource values cannot be constants. Constants are bounded in size and nesting.

`let` at module level runs its initializer at load time. Prefer `const` for
literal configuration data (`lint.prefer-const`).

## 4. Types And Values

### 4.1 Types

| Type | Values |
|---|---|
| `Null` | `null` |
| `Unit` | the value of statements; `Ok()` is `Ok(Unit)` |
| `Bool` | `true`, `false` |
| `Int` | signed 64-bit integers |
| `UInt` | non-negative integers, checked wherever stored |
| `Float` | IEEE 754 binary64 |
| `Duration` | non-negative millisecond spans |
| `Str` | valid UTF-8 text |
| `Bytes` | arbitrary bytes |
| `Path` | native path bytes without NUL |
| `Regex`, `Digest` | compiled regex; typed hash digest |
| `List[T]`, `Map[K, V]`, `Stream[T]` | collections |
| records | named schemas `{name: Str, ...}` and builtin erased `Record` |
| enums | nominal tag unions |
| `T?` | `null` or a `T` |
| `Result[T, E]` | `Ok(T)` or `Err(E)`; `Result[T]` is `Result[T, Error]` |
| `Error`, error families | structured errors |
| `Status`, `ProcessError`, `ProcessHandle`, `Command` | process values |
| `Pure`, `Proc` | dynamic callable handles |
| `Module[T]` | runtime module values |
| `Any` | dynamic value of unknown type |

There is no implicit conversion between any two types: not between strings and
numbers, not between `Int` and `Float`, and not from anything to `Bool`.

### 4.2 Numbers

`Int` arithmetic is checked. Overflow fails with `integer-overflow`, and
division or remainder by zero fails with `division-by-zero`. `/` truncates
toward zero. `%` is integer-only. Non-negative bitset operations are methods:
`.bit_and(mask)`, `.bit_or(mask)`, `.clear_bits(mask)`.

`UInt` shares `Int`'s representation and operators but is never negative. The
constraint is enforced wherever a value is stored or crosses a typed boundary:
assignment (including compound assignment), record fields, list elements, map
keys and values, arguments, defaults, returns, and stream items as they are
produced. A negative value fails at that point with `type-error`, leaves the
target unchanged, and runs ordinary cleanup; `try` does not turn it into data.
Use `value.require(UInt)?` to validate untrusted integers recoverably.

`Float` and `Int` never mix: convert with `.float()`, or back with
`.floor()`, `.ceil()`, or `.round()`, which return `Result[Int]` and reject
NaN, infinities, and out-of-range values. Float equality compares exact
values. Non-finite values display as `NaN`, `Infinity`, and `-Infinity`; sorts
use IEEE total order, so NaN orders stably. JSON encoding rejects non-finite
values.

### 4.3 Durations

`Duration` holds unsigned milliseconds.

| Expression | Result |
|---|---|
| `Duration + Duration`, `Duration - Duration` | `Duration` |
| `Duration * Int`, `Int * Duration` | `Duration` |
| `Duration / Int` | `Duration`, truncating sub-millisecond remainders |
| `Duration / Duration` | `Int` interval count |
| `<`, `<=`, `>`, `>=`, `==`, `!=` | `Bool` |

These are pure and fail at the operator with `duration-overflow`,
`duration-underflow` (a negative result), `duration-negative-factor`,
`division-by-zero` (a zero or negative divisor), or `integer-overflow` (an
interval count outside `Int`). Float scaling and `%` are type errors. Compound
assignment follows the same rules. `time.millis(n)` and `time.seconds(n)`
convert computed integers by clamping negatives to zero and saturating; write
`n * 1s` when overflow should fail.

### 4.4 Text, bytes, and paths

`Str` is valid UTF-8. `Bytes` is arbitrary data; `.utf8()` decodes it with a
`Result`. Text methods count Unicode scalar values (`.count_chars()`) unless
their name says bytes (`.byte_len()`, `.byte_at()`, `.byte_slice()`, `.find()`
returns a byte offset). There is no `Str.len()`.

`Path` stores native Unix path bytes and may hold names that are not valid
UTF-8. It never contains NUL. `fp"..."` and compound process words append
`Path` fragments byte for byte. Display conversion (`print`, `f"..."`,
`.display()`) produces UTF-8 text and cannot recover non-UTF-8 bytes. Path
construction never joins, normalizes, expands, globs, or checks the filesystem:
separators and `..` stay exactly as written. `Path(text)` converts trusted
text and `Path.parse_bytes(bytes)` converts bytes with a `Result`. A string
literal is accepted where a `Path` is statically expected (a typed parameter,
binding, or redirection target); a runtime `Str` always needs explicit
conversion.

### 4.5 Lists, maps, and records

`List[T]` is an ordered, homogeneous sequence. `+` concatenates two lists;
`items += [x]` and `items += more` append to a mutable binding. Methods such as
`.push(x)` return a new list.

`Map[K, V]` is ordered by key. `Map[V]` means `Map[Str, V]`. Keys must be
`Str`, `Int`, `UInt`, `Bool`, `Bytes`, `Path`, or `Duration`; `Float`, `Any`,
records, collections, and handles are not keys. Iteration, `.keys()`, and
`.values()` follow key order: numeric and duration keys numerically, `false`
before `true`, strings in ordinary string order, and `Bytes`/`Path` by native
byte order. Keys are never converted between domains. `.set`, `.remove`, and
`.push` return a new map. JSON objects and environment names require `Str`
keys.

Records are field collections. A named schema (`type T = {...}`) fixes field
names and types. Records are width-compatible: a value with extra fields fits
a schema that names fewer. The builtin `Record` type erases field knowledge.

Lists, maps, and records have value semantics: assigning, passing, or storing
one behaves as a copy, so mutating a `var` never changes another binding
(copies share storage until written).

`List.get(index)` and `Map.get(key)` return `Result[T]`. A present `null`
value is `Ok(null)`; `??` handles only the missing case. `Str.find`,
`Str.byte_at`, and `Bytes.byte_at` return `Int?` and use `null` for absence.

### 4.6 Optional values

`T?` accepts `null` or a `T`. It is a type, not a runtime wrapper. Use `??` for
a fallback, `?.`/`?[...]` to guard an operation, or a null test to narrow (see
5.4).

### 4.7 Record schemas and constructors

```xsh
type BuildOptions = {root: Path, jobs: UInt = 4, flags: List[Str] = []}

let opts = BuildOptions(root: p"src")
let wide = BuildOptions(root:, jobs: 16)
```

A named schema has a constructor that takes only named arguments (with puns,
`root:` meaning `root: root`). Unknown, duplicate, or missing required fields
are errors, and arguments evaluate once in source order. Field defaults are
constant data (literals, constant lists and records, and earlier constants)
and apply only to constructor calls. They never fill missing fields in record
literals, JSON, or `.require`. A schema name is not a callable value.

Schemas and aliases may take type parameters:

```xsh
type Observation[T] = {value: T?, samples: List[T]}
type Count = Observation[Int]
let obs = Observation(value: 12, samples: [])
```

Arguments must be fully supplied in type position. A constructor call infers
them from its fields or its expected type; `null` and empty containers alone do
not decide them, and conflicting evidence is an error, never a guessed `Any`
(`check.constructor-inference`). Defaults must be valid for every
substitution. Functions, enums, error families, and module contracts are not
generic, and there is no expression-level type-argument syntax.

### 4.8 Enums

```xsh
enum Level { Info, Warn, Fault(Str) }

let level = Fault("disk full")
match level {
  Info => print "info"
  Warn => print "warn"
  Fault(reason) => print f"fault: ${reason}"
}
```

An enum declares one or more variants; payload-free variants are bare names
and payload variants are called like functions. Constructors live in the
declaring module's namespace, not under the enum name. A `match` over an enum
without a catch-all reports uncovered variants (`check.non-exhaustive-match`).
`type Alias = Level` aliases the same nominal type.

A Str-backed enum gives each payload-free variant a unique constant wire
string:

```xsh
enum State: Str { Ready = "ready", Missing = "" }
```

It is still nominal: assigning a `Str` never produces a `State`, and type
patterns never convert. `.require(Schema)` converts exact wire strings in
enum-typed slots, recursively through records, lists, optional slots, and map
values, and publishes the result only if the whole value validates. Unknown
strings report the field or index path. JSON encoding writes the declared
strings. Ordinary enums cannot be JSON-encoded.

### 4.9 Module contracts

```xsh
type BuildPlugin = module {
  export let name: Str
  export optional let description: Str
  export proc build(root: Path) [fs, process, error] -> Result[Unit]
  export pure label(name: Str) -> Str
}

let plugin = module.load(plugin_path)?.require(BuildPlugin)?
plugin.build(root)?
```

A module contract describes a runtime module's exports. Each entry's kind and
full signature must match exactly; `optional` entries may be absent, and extra
exports are allowed. Statically imported modules satisfy contracts directly;
`module.load` values are checked when `.require(Contract)` runs. Streams are
not contract members.

### 4.10 Errors

`Error` is the common structured error. Programs declare nominal error
families whose variants carry fixed payloads and may implement facets:

```xsh
error ConfigError = Missing(file: Path) : NotFound | Invalid(file: Path, message: Str)

pure check(text: Str, file: Path) -> Result[Unit, ConfigError] {
  if text == "" {
    return Err(ConfigError.Invalid(file:, message: "empty"))
  }
}
```

Constructors are qualified by family (and by module namespace when imported).
Every error has `.message`. Exact variant patterns expose payload fields;
`is Facet` matches any variant that implements a facet. Programs branch on
variants and facets, never on string kinds; family and variant names appear in
diagnostics only.

`error.fail(message)` builds a `Result[Unit, Error]` validation failure; it is
the shortest way to report an expected failure from a fallible function.

`Err(outer, cause: inner)` translates one error into another and keeps the
original as diagnostic cause. The outer error's family, payload, and Result
type are unchanged, and matching inspects only the outer error. Tracebacks
render the cause chain (bounded). Resources reachable from an error payload or
cause move with the error like any other value.

`ProcessError` is the family returned by process forms. Every variant carries
`message` and `status: Status?`.

| Variants | Facet |
|---|---|
| `NotFound`, `PermissionDenied`, `NonzeroExit`, `Signal`, `Timeout`, `Canceled`, `CaptureLimit` | same name as the variant |
| `InvalidUtf8`, `InvalidTarget` (NUL in a target) | `InvalidData` |
| `Io`, `Redirection` | `HostIo` |
| `ExecFailure`, `Spawn`, `PipelineFailure`, `UnexpectedExit` | `ProcessFailure` |
| `Unknown` (an invalid or already consumed handle) | none |

```xsh
match process.run(command) {
  Ok(status) => print ${status.ok}
  Err(ProcessError.Timeout { message }) => print $message
  Err(is PermissionDenied) => print "permission denied"
  Err(error) => return Err(error)
}
```

A failed `assert` produces `AssertionError.Failed(message: Str)`.

### 4.11 Runtime-owned values

- `Status` is a completed process state, obtained only from process forms.
  It has `ok` and `success: Bool`, `kind` (`"exit"` or `"signal"`), and the
  methods `exited()`, `signaled()`, `exited_with(code)`,
  `exit_code() -> Result[Int]`, and `signal_number() -> Result[Int]`. `!status`
  is `!status.ok`.
- `ProcessHandle` is a live child started by `spawn` (see 11.7).
- `Command` is a typed process plan from `process.command { ... }` or
  `process.command_argv(...)`. It is not a block literal and runs only when
  passed to `spawn`, `process.run`, or `process.spawn`.
- `NetJob` is a live network transfer from `net.start` (see 11.8).
- `FsRoot` is an open directory capability; see the standard modules.
- `Regex` and `Digest` are immutable values with methods.

Handles are values that alias one live host resource; see 11.8 for ownership.

## 5. Typing

### 5.1 Inference and annotations

Inference is local and predictable. A `let` or `var` without an annotation
takes its initializer's type. An annotation supplies the expected type, and the
initializer must fit it. Annotations never convert or validate data: they only
state what the checker must prove. Callers never supply types to a function's
body, and a function's body never supplies types to its callers' arguments.

An unannotated empty list (`[]`) or `map.empty()` has an unresolved element
type that later writes, arguments, and returns in the same function resolve,
including writes in branches and loops. It must be resolved before a concrete
operation uses it; otherwise annotate it. A `var` initialized to `null`
becomes `T?` once a non-null assignment determines `T`; `let x = null` stays
`Null`. An unannotated `{}` is an empty record, or an empty map where a `Map`
is expected. Incompatible contributions are errors; inference never widens to
`Any` or invents a union.

### 5.2 Assignability

- Identical types match. Every concrete value fits `Any`.
- `List`, `Map`, and `Stream` are invariant in their parameters: `List[Str]` is
  not `List[Any]`, and `Stream[Int]` is not `Stream[UInt]`.
- `null` and `T` both fit `T?`.
- A record fits a schema when it has at least the schema's fields with fitting
  types. Erased `Record` accepts any record but cannot satisfy a named schema;
  `{}` is an exact empty record.
- Enums, error families, and other nominal types match only themselves (or,
  for errors, `Error` and the facets they implement).
- `Result[T, E]` fits `Result[T, F]` when `E` fits `F`.
- Equality may compare `T` with `T?` in either order; it yields `Bool` and does
  not narrow or validate.

### 5.3 `Any` and dynamic boundaries

`Any` is the type of data whose shape is unknown: decoded JSON, untyped host
records, results of dynamic callables. A concrete value can always become
`Any`, but `Any` never becomes concrete implicitly. Field access, indexing, and
method calls on `Any` are checked at runtime and produce `Any`. Using an `Any`
where a concrete type is required is `check.dynamic-boundary`:

```xsh
let doc = json.decode(text)?
let n: Int = doc.count                      # error: check.dynamic-boundary
let count = doc.count.require(Int)?         # Int
```

Two forms establish a concrete type:

- `value.require(T)` checks a value against a type and returns `Result[T]`.
  Records are checked recursively, including inside lists, maps, and optional
  slots, and the result is published only if the whole value passes. Extra
  fields are kept. A `T?` field must still be present (it may be `null`).
  Defaults are never filled in, and Str-backed enum slots are converted from
  their wire strings.
- A type pattern `name is T` in a `match`, or a test `value is T`, checks the
  runtime value and narrows the binding (see 6.10).

`.require()` with no argument takes its target from an independent expectation:
an annotated binding, an annotated return or tail, or a uniquely selected
parameter type. Each `?` peels one `Result` layer from that expectation. If no
target is known, the checker reports `check.require-target`.

Module contracts (4.9) are the dynamic boundary for runtime-loaded modules.

### 5.4 Narrowing

Narrowing is local and lexical. A fact holds inside the branch, guarded
continuation, loop body, or match arm where a condition proved it.

- `x != null` (or `x == null` on the false side) narrows `T?` to `T`.
- `x is Pattern` narrows a stable binding to the pattern's type.
- `"field" in record` proves the field exists.
- `!`, `and`, and `or` combine facts in the obvious way; an immutable `Bool`
  binding carries the facts of the condition it holds.
- `guard cond else { ... }`, `assert cond`, and an exiting guarded statement
  (`return x when cond`) make the success facts hold for the following
  statements.

Facts may refer to a binding or a statically known field path. Assigning to the
binding or to an overlapping path invalidates them; writes to disjoint sibling
fields do not. Calls that may mutate captured `var`s invalidate facts about
them. Deferred blocks and callables check mutable captures again, because they
run later. Narrowing never infers numeric ranges, filesystem state, or
arbitrary implications.

During `xsht check`, `reveal_type(expr)` reports the inferred type of `expr` as
a note. It is rejected everywhere else.

## 6. Expressions

The complete expression grammar is in Appendix A.

### 6.1 Precedence and grouping

From tightest to loosest:

| Level | Operators |
|---|---|
| postfix | `.name`, `?.name`, `[i]`, `?[i]`, `[a..b]`, calls, `?` |
| unary | `!`, `-` |
| multiplicative | `*`, `/`, `%` |
| additive | `+`, `-` |
| ordering and membership | `<`, `<=`, `>`, `>=`, `in`, `not in` |
| equality and pattern tests | `==`, `!=`, `is` |
| logical | `and`, `or` |
| fallback | `??` (right-associative) |

Some combinations must be grouped explicitly, because a reader cannot tell the
intended meaning at a glance:

- ordering or membership mixed with equality or `is`: write
  `(a < b) == expected`;
- `and` mixed with `or`: write `(a and b) or c`;
- `??` mixed with `and` or `or`: write `(flag ?? false) and ready`.

Parentheses that change nothing are errors (`parse.redundant-parens`).
Parentheses are meaningful when they override precedence, provide a required
grouping above, form a typed command argument `(expr)` or splice `@(expr)`,
mark a value block `{ (value) }`, group a run-valued payload
(`return (run.status make) when ready`), or group pattern alternatives.

Ordering operators chain: `0 <= offset < limit` compares adjacent pairs left
to right, evaluates each reached operand once, and stops at the first false
pair. `(a < b) < c` compares a `Bool` instead.

### 6.2 Operators

| Operator | Operands | Result |
|---|---|---|
| `+` | `Int`, `UInt`, `Float`, `Duration`, `Str`, `List[T]` (same type on both sides) | same type |
| `-` | `Int`, `UInt`, `Float`, `Duration` | same type |
| `*` | `Int`, `UInt`, `Float`; `Duration` with `Int` | see 4.2–4.3 |
| `/` | `Int`, `UInt`, `Float`; `Duration` by `Int` or `Duration` | see 4.2–4.3 |
| `%` | `Int`, `UInt` | same type |
| `<` `<=` `>` `>=` | two `Int`, `UInt`, `Float`, `Str`, or `Duration` values | `Bool` |
| `==` `!=` | two values of the same type, or `T` with `T?` | `Bool` |
| `and` `or` | `Bool`; short-circuit | `Bool` |
| `!` | `Bool`, or `Status` (meaning `!status.ok`) | `Bool` |
| `in`, `not in` | see below | `Bool` |
| `??` | `Result[T, E]` or `T?` on the left, `T` on the right | `T` |

`in` tests element membership in a `List`, key presence in a `Map`, field
presence in a `Record` (with a `Str` key), substring containment in `Str`,
byte containment in `Bytes`, display-text containment in a `Path` (not
filesystem ancestry), and entry membership in `env.PATH`. A present key or
field whose value is `null` is still present. Operands evaluate once, left to
right.

`/` on paths is not path joining; build paths with `fp"${root}/child"`. `//`
and `div` do not exist.

### 6.3 Calls

Functions are called with expression syntax: `f(x, y)`, `module.f(x)`,
`value.method(x)`. Arguments may be positional, named (`jobs: 4`), punned
(`jobs:` means `jobs: jobs`), spread from a record (`...opts`), or spliced
from a list (`@xs`, positional only).

```xsh
let opts = {jobs: 4, verbose: true}
build(root, ...opts)
build(root, jobs:, verbose: false)
main(@args)
```

A record spread supplies exactly the fields visible in its checked record
type; `Any`, open `Record`, `Map`, `Optional`, and `Result` operands must be
narrowed or unwrapped first. A `null` field is supplied (it does not select the
parameter's default). Unknown names and parameters supplied twice are errors.
The receiver evaluates first, then each argument once in written order. Return
types never select an overload.

### 6.4 List, record, and map literals

```xsh
let argv = ["cc", @flags, "-o", output]
let entry = {name: "core", path: root, "content-type": "text/plain"}
let counts = {[ext]: 1}
let merged = {...defaults, ...overrides}
```

- **Lists.** Elements must share one type (from context when present). `@xs`
  splices a `List[T]` in place; without `@` a list is one nested element.
  Results and streams must be unwrapped or collected before splicing.
  Elements evaluate once in order, and a failure stops construction.
- **Records.** Fields may be labeled, quoted (a quoted key containing a dot is
  one key), punned (`{name}`), or spread from a statically known record.
- **Maps.** A computed key `[expr]: value` makes a brace literal a map, as does
  a non-empty literal in a context expecting `Map`. Computed keys share one key
  domain; constant labels are `Str` keys. Later entries and spreads replace
  earlier ones, though every entry still evaluates, key before value.
- **Functional update.** `{...config, build.jobs: 8, build.flags.debug: true}`
  copies `config` and replaces existing nested fields. It requires exactly one
  leading spread of a known record, and every path must name an existing field
  of the same type. The base evaluates once; replacements evaluate in order and
  publish nothing on failure.

### 6.5 Comprehensions

```xsh
let objects = [fp"${src}.o" for src in sources if src.ext == "c"]
let sizes = {[e.path]: e.size for e in entries}
let pairs = [f"${a}-${b}" for a in left for b in right if a != b]
```

Clauses run like nested `for` loops with `if` filters: each inner iterable is
evaluated anew per outer binding, a false filter skips the rest, and the
projection runs once per surviving combination. Sources may be lists, streams
(pulled lazily), maps (yielding `{key, value}` items), `Str` (scalars), or
`Bytes` (byte values); a `Result`-wrapped source propagates its failure. Filters
are `Bool` values, never assertions. A failed comprehension exposes no partial
result.

### 6.6 Indexing and slicing

`list[i]` requires `0 <= i < len` and otherwise fails with
`index-out-of-range`. `map[key]` and `record["field"]` fail when the key or
field is missing; use `.get(...)` for a `Result`.

`value[start..end]` slices a `List[T]`, `Str`, or `Bytes` and returns the same
type. Either bound may be omitted. Negative bounds count from the end, and
both bounds are clamped into `[0, len]`; an end before the start gives an
empty value. `Str` slices count Unicode scalar values, and `Bytes` slices count
bytes: `"aé🦀"[1..3]` is `"é🦀"` and `b"abcdef"[2..5]` is `b"cde"`. `Str` has
no single-element `[i]` indexing; slice one scalar or iterate. The method
`.slice(offset, length)` is distinct: it rejects negative or out-of-range
offsets instead of clamping.

### 6.7 Optional chaining

`a?.field`, `a?.method(...)`, `a?[i]`, and `a?[x..y]` guard one nullable hop.
The receiver evaluates once. When it is `null` the whole operation is `null`
and its arguments are not evaluated; otherwise the ordinary operation runs and
its result is made optional. Guard each hop: `config?.server?.host?.trim()`.
On a `Result` receiver, `?.` and `?[` propagate the outer `Err` first. An
optional method that returns a `Result` produces `Result[T, E]?`; handle the
layers separately, as in `(text?.parse_int() ?? Ok(0))?`.

### 6.8 Conditional and match expressions

`if` and `match` produce values when used in value position. A value `if`
needs an `else`; a value `match` must be exhaustive without relying on guards.
Branches may contain statements followed by a tail value, and all reachable
branch values must have one type. A branch that returns, breaks, or fails
contributes no value.

```xsh
let mode = if release { "release" } else { "debug" }
let label = match level {
  Info => "info"
  Warn => "warn"
  Fault(reason) => f"fault: ${reason}"
}
```

A `match` with no matching arm fails with `match-no-arm`.

### 6.9 Blocks

A bare `{ ... }` is a block when its first entry is a statement and a record or
map literal when its first entry looks like a field (`{}`, `{name}`,
`{name: v}`, `{[k]: v}`, `{...r}`). Write `{ (value) }` for a block whose only
content is a single name. A block introduces a lexical and cleanup scope but
no function, error, or loop boundary. In statement position it runs as
statements; in value position its tail is its value.

### 6.10 Patterns

Patterns appear in `match` arms, `if let`, `while let`, `is` tests, and
`retry ... on (...)`.

| Pattern | Matches |
|---|---|
| `_` | anything |
| `name` | anything, binding it (or a payload-free variant of that name) |
| literal | an equal value |
| `Ok(p)`, `Err(p)`, `Variant(p, ...)` | constructors |
| `Family.Variant { field, .. }` | an error variant, binding payload fields |
| `is Facet` | any error implementing a facet |
| `{field: p, other, ..}` | records with those fields |
| `[a, b]`, `[head, ..tail]`, `[..]` | lists of exact length, or a prefix with a rest |
| `name is T`, `_ is T` | a dynamic value of runtime type `T` |
| `p | q` | either alternative |
| `p as name` | `p`, also binding the whole matched value |

The subject evaluates once and arms are tried in order. Captures become
visible only after the whole pattern matches. Alternatives must bind the same
names with the same types; `as` binds tighter than `|`. A guard `if cond` runs
after the pattern matches, and a false guard moves to the next arm. Guarded
arms never count toward exhaustiveness. List patterns check the length before
touching elements and apply only to `List` values. Type patterns apply only to
`Any` and erased `Record`; for a known shape use `.require(T)?`.

```xsh
match json.decode(input)? {
  i is Int => print ${i.float()}
  f is Float => print ${f}
  _ is Null => print "null"
  _ => print "other"
}
```

`value is Pattern` is a `Bool` test with the same matcher. It cannot bind
names: write `outcome is Ok(_)`. Alternatives need grouping
(`value is (P | Q)`), and negation is `!(value is P)`. Testing a `Result` never
propagates. A true test narrows a stable binding in the selected branch.

## 7. Bindings And Assignment

`let` bindings are immutable and `var` bindings are mutable. Both may
destructure records:

```xsh
let {name, version: v, build: {jobs, ..}, ..} = manifest
for {path, size} in entries {
  print f"${path}: ${size}"
}
```

A field may bind its own name, rename (`version: v`), or nest. `..` ignores
the remaining fields and `_` discards a value. The source evaluates once, and
all fields are selected before any name becomes visible. Destructuring an
`Any` requires a checked schema first. The same targets work in `for`,
comprehensions, and `guard let`. `export let` takes only a simple name, and
function parameters never destructure.

`_` is a discard, not a variable: `let _ = expr` evaluates `expr` and drops it,
and may repeat.

Assignment targets are a mutable local binding, optionally followed by field,
map-key, and list-index selectors: `config.build.jobs = 8`,
`counts[name] = 1`, `rows[i] = row`. List indices must be in range (no
appending or slice assignment), and `Str`/`Bytes` are immutable. Compound
operators `+=`, `-=`, `*=`, `/=`, `%=` follow the binary operator rules, and
the result must keep the target's type. Selectors evaluate once in path order,
then the right side, then the update commits; a failed update leaves the
target unchanged. Assignment produces `Unit`. Earlier aliases keep their old
contents.

Pure functions may assign to their own local `var`s, including their fields
and entries, but not to parameters, captured bindings, or module bindings.

## 8. Statements, Values, And Results

### 8.1 Statement and value position

Every expression is checked either in statement position, where its value is
not used, or in value position, where it is. Value positions are initializers,
arguments, operands, explicit `return`/`yield`/`break` payloads, conditions,
and the tail of a body whose enclosing function, block, callback, or capture
consumes a value. Everything else is statement position: top-level
statements, non-tail statements in a body, and the tail of a body whose
result type is `Unit` or `Result[Unit]`.

In statement position:

- A `Unit` value is discarded.
- A `Result[Unit]` propagates its failure automatically; success continues.
- A plain `run` asserts success (see 11.2).
- A `Bool` is an error (`check.bool-statement`). It is neither an assertion
  nor silently dropped; write `assert cond` or `let _ = cond`.
- Any other value-producing expression, including `Result[T]` for non-`Unit`
  `T`, is an error. Bind it, return it, use it as the tail, or discard it with
  `let _ = ...`.

Every callable tail in value position is data. A `Bool` tail returns its value,
including `false`; a `Result` tail is returned as a value, not unwrapped.

### 8.2 `assert`

```xsh
assert actual == expected
assert entries.len() > 0, f"no entries under ${root}"
```

`assert condition[, message]` is the only assertion form. It is a `Unit`
statement; the condition must be a concrete `Bool` (not `Any`, `Status`,
optional, or `Result`) and the message a `Str`. The condition runs once. When
it is false, the message (if any) runs, and the assertion propagates
`AssertionError.Failed(message)` like any other `Err`: it obeys `try` and
`retry`, runs cleanup, and requires the `error` effect in a restricted proc.
The failure report includes the condition text, the left and right operands of
a failed comparison, the failing pair of an ordering chain, and the operands
of `and`/`or` conditions (without evaluating skipped ones). Assertions are
always enabled and need no test context. A successful assertion narrows like a
guard (5.4).

### 8.3 Propagation with `?`

`expr?` requires `expr` to be a `Result`. On `Ok(v)` it yields `v`. On
`Err(e)` it leaves the nearest propagation boundary:

| Context | Effect of `?` on `Err` |
|---|---|
| inside `try { ... }` | the `try` produces `Err(e)` |
| inside a `retry` attempt | that attempt fails |
| a function returning `Result[T, F]` | the function returns `Err(e)`; `E` must fit `F` |
| a proc returning a non-`Result` type | the call itself fails; the failure keeps unwinding through callers until a `try`, `retry`, or the top level captures it |
| top level | the script prints a traceback and exits `3` |

A pure function whose return type is not a `Result` cannot use `?`
(`check.try-context`). Error handler blocks, `if`, `match`, loops, and bare
blocks are not boundaries. In a restricted proc, `?` that can leave the proc
requires the `error` effect.

`expr ?` with a space is the same operator in expression context. In a command
argument, a separated `?` belongs to the whole command or run form: write
`expr?` or `(expr?)` to propagate inside one argument.

### 8.4 Fallback with `??`

`left ?? fallback` yields the `Ok` payload of a `Result`, or a non-null
optional value; otherwise it evaluates and yields `fallback`, which must have
the success type. It is lazy and right-associative.

An error handler block receives the error:

```xsh
let config = load_config(path) ?? { |failure|
  eprint f"using defaults: ${failure.message}"
  default_config()
}
```

The parameter is required (use `_` to ignore it), immutable, and has the
Result's exact error type. The tail must have the success type; a `Bool` tail
is a value. The handler creates no boundary: `return`, `break`, `continue`,
and `?` inside it target the enclosing function, loop, or capture. Optional
values have no error and cannot use a handler parameter.

### 8.5 Results as values

`Ok(value)` and `Err(error)` construct results; `Ok()` is `Ok(Unit)`. A
function declared `-> Result[T]` may return or tail-produce either a
`Result[T]` or a plain `T`, which is wrapped in `Ok`. A `Result[Unit]` body may
simply finish. Ignoring a value-producing `Result` is an error; `let _ =`
discards one deliberately.

`result.context(kind, message)` returns `Ok` unchanged and adds a diagnostic
context frame to an `Err`.

### 8.6 Control flow

Conditions are `Bool` or `Status` (true when `status.ok`). There is no
truthiness. Each branch is its own scope.

`for` iterates a `List`, `Stream`, `Map` (items `{key, value}` in key order),
`Str` (one-scalar strings), or `Bytes` (byte values as `Int`), including a
`Result`-wrapped source whose `Err` propagates. The source evaluates once, and
the loop sees a snapshot: reassigning the source inside the loop does not
change the iteration. A pipeline as the source is consumed item by item (see
13.2). The loop binding is immutable.

`loop { ... }` repeats until `break`; `break value` makes the loop an
expression with that value. `break` and `continue` target the nearest loop and
are not allowed inside stream stage blocks.

`return`, `break`, `continue`, and `yield` accept a postfix guard:

```xsh
return cached when cached != null
continue unless entry.kind == "file"
yield row when row.size > 0
```

The condition runs first and the payload runs only if selected. A guarded
statement can fall through, so it does not end a block, but when its payload
leaves the block the following statements may rely on the opposite of the
condition. Group a run payload: `return (run.status make) when ready`; without
parentheses `when ready` would become argv words.

`guard cond else { ... }` continues when `cond` holds and otherwise runs the
block, which must leave the enclosing continuation on every path (by `return`,
`break`, `continue`, or a terminating call such as `abort`). A fallible call is
not termination. The block takes no parameter and creates no boundary.

`guard let target = expr else { |failure| ... }` binds the `Ok` payload of a
`Result` (with an optional type annotation) and otherwise runs the block with
the error. Inside a loop the block may `break` or `continue`.

`if let pattern = subject` and `while let pattern = subject` test a pattern.
They do not unwrap `Result` or optional values implicitly: write
`if let Ok(value) = result`. Captures are immutable and visible only in the
selected branch or iteration. Irrefutable patterns are rejected.

`with` binds several fallible values with one error handler:

```xsh
with config = read_config()?, db = connect(config)? {
  serve(db)
} else { |failure|
  eprint f"setup failed: ${failure.message}"
}
```

Each binding sees the earlier ones. If any initializer fails, the `else` block
runs with the error (the common nominal type, or `Error` for mixed families).
Bindings are not visible in the handler.

Parameterized blocks always put the parameters inside the brace:
`{ |name| ... }`. Plain conditional branches, `guard` failure blocks, and
deferred blocks take no parameters.

### 8.7 `defer`

`defer action` or `defer { ... }` registers cleanup for the enclosing block.
Registration runs nothing. Actions run in last-in, first-out order when
control leaves the block for any reason: normal completion, propagation,
runtime failure, `return`, `break`, `continue`, or cancellation.

A deferred block resolves names at registration but reads their values when it
runs; snapshot an earlier value with `let`. Its statements are in statement
position, so a failing `Result[Unit]` stops that action. Other actions still
run. The original failure stays primary; if there was none, the first cleanup
failure becomes primary, and later cleanup failures are reported with their
locations. A deferred block cannot `return`, `yield`, `break`, or `continue`
out of its body. Owned process handles and network jobs are cleaned up before
the block's defers run (11.8). `abort(status, force: true)` skips cleanup.

### 8.8 `try`, `retry`, and `ctx`

`try { ... }` evaluates a block once and captures its failures as data:

```xsh
let parsed = try {
  let n = text.parse_int()?
  n * 2
}
```

Normal completion yields `Ok(tail)`; an empty body yields `Ok(Unit)`. A
`Result` tail stays nested (`try { op() }` is `Result[Result[T]]`; write
`try { op()? }`). Explicit `?`, statement-position `Result[Unit]` failures,
failed assertions, and failed plain `run` statements inside the block become
`Err`. `return`, `break`, and `continue` keep their ordinary targets, so
`return Err(e)` leaves the function while `Err(e)?` stops at the `try`. Runtime
type failures, `abort`, and cancellation are not captured. The tail value is
computed before the block's defers run; a cleanup failure becomes the `Err`
when the body succeeded. The error type is the narrowest common family, or
`Error`. A block that can only fail needs a `Result` annotation for its success
type.

`retry [delays] { ... }` re-runs a block on failure:

```xsh
let index = retry [1s, 2s, 4s] on (is Timeout) {
  fetch_index()?
}?
```

The delay list (each a `Duration`) evaluates once before the first attempt.
The block runs, then again after each delay while attempts fail, so an empty
list means exactly one attempt. Inside an attempt, `?` fails the attempt
rather than the enclosing function and needs no `error` effect; `return`,
`break`, and `continue` keep their ordinary targets. Each attempt is a scope
whose defers run before the next attempt or the final result. The optional
`on (pattern)` selects which errors to retry, using the non-binding pattern
rules; any other failure returns at once without consuming a delay. The result
is `Ok(value)` from the first successful attempt or `Err` from the last failed
one. A non-empty delay list requires the `time` effect. Each attempt emits a
`retry.attempt` trace event.

`ctx description { ... }` labels failures that leave its body:

```xsh
ctx f"installing ${package.name}" {
  fs.copy(source, dest)?
}
```

The `Str` description evaluates once on entry. When a failure propagates out
of the body, after the body's own cleanup, it gains one context frame (shown
in tracebacks) with the description and the region's span. The error's
identity, payload, and earlier contexts are unchanged. Values returned as data
are not annotated. The body has the ordinary statement or value-block meaning,
and `ctx` remains an ordinary identifier outside this form. Use `ctx` to add
context to the same failure, and `Err(new, cause: old)` to translate into a
different one.

### 8.9 `abort`

`abort(status: Int, force: Bool = false)` ends the script with `status` as a
deliberate exit. It is not an error: no traceback is printed and `try` does not
capture it. Deferred cleanup runs while unwinding unless `force` is true.

## 9. Functions

### 9.1 Definitions

```xsh
pure object_name(src: Path) -> Path { src.with_ext("o") }

proc compile(src: Path, out: Path) [process, error] {
  run cc -c $src -o $out
}

stream lines_of(paths: List[Path]) [fs, error] -> Stream[Str] {
  for file in paths {
    yield @(file.lines()?)
  }
}
```

- `pure` functions are deterministic and effect-free. They may call other pure
  functions and pure standard APIs and mutate their own locals, but cannot run
  processes, call procs or core commands, read or write ambient state, or glob.
- `proc` functions may perform effects and are called with expression syntax:
  `compile(src, out)?`. Command-style calls (`compile src out`) are not
  accepted, and an unresolved command name is an error; it never falls back to
  `PATH`.
- `stream` producers are lazy (9.5).

Calls to value-returning procs remain effectful in expressions. First-class
`Proc` values have `.call(...) -> Result[Any]` and `Pure` values
`.call(...) -> Any`; their arguments are checked at runtime.

### 9.2 Parameters

Parameters are `name: Type`, `name: Type = default`, `name = default`, or a
final rest parameter `...name: List[T]`. A defaulted parameter may omit its type
when the default alone determines one concrete type; `null` and empty
containers need annotations.

Defaults are evaluated in the callee's declaration scope, cannot see other
parameters, and run once per call for each omitted argument, in parameter
order, after the supplied arguments. A stream producer evaluates its defaults on
its first pull. Overloads (standard APIs only) are chosen by argument names and
types.

### 9.3 Return types

A function body's tail is its result: the final expression, or a final
command statement's value. A final `Result[Unit]` call in a `Unit` or
`Result[Unit]` body propagates as a statement.

Return types may be omitted only on private functions:

- A private `pure` function infers its return type from its explicit returns
  and reachable tails. All paths must agree on one concrete type.
- A private `proc` with value tails infers `Result[T]`. A `Result` tail keeps
  its shape, and a plain `T` is wrapped in `Ok`. If every completion is
  Unit-like (no tail value, an `if` without `else`, a bare `return`, or a
  failure-only `Err(...)`), the proc returns `Result[Unit]`. Value tails that
  disagree are `check.type-mismatch`; there is no fallback to `Result[Unit]`.
  Failure-only completions (`Err(...)` tails and early `return Err(...)`) and
  `?` propagations join into the error type: variants of one family join to
  the family, anything else to `Error`.
- Exported functions, `proc main`, and functions in a recursive group need
  explicit return types (`check.required-return`). Empty collections, error-only
  bodies, and dynamic shapes cannot be inferred (`check.infer-return`).

A proc with no return type and a statement body returns `Result[Unit]`. A
caller that ignores an inferred `Result[T]` must handle it like any other
value-producing result.

### 9.4 Callable aliases

An unannotated `let` that names a checked function or a module export keeps
that function's full signature (labels, defaults, return type, and effects):

```xsh
let build = toolchain.build
build(root, jobs: 4)?
```

A `var`, a conditional selection, or an explicit `Pure`/`Proc` annotation gives
a dynamic callable instead. An exported alias must still state its own return
and effect contract.

### 9.5 Stream producers

Calling a `stream` producer returns a `Stream[T]` without running its body.
The body runs as the stream is consumed by a `for` loop, a pipeline, or
`.collect()`. Each `yield value` emits one item, and `yield @source` emits every
item of a `List[T]` or `Stream[T]`, pulling a delegated stream on demand. The
source evaluates once when reached; handle results explicitly
(`yield @(load_rows()?)`). `return` without a value ends the stream; `return
value` is rejected. Defers run when the producer finishes, fails, or its
consumer stops early (a delegated child is closed before its parent). Streams
are one-pass, and aliases share one cursor. Producers use proc-style effect
clauses.

### 9.6 Effects

A proc may declare an effect clause between its parameters and its return type:

```xsh
proc read_config(file: Path) [fs, error] -> Result[Config] {
  json.read(file)?.require(Config)
}
```

| Effect | Covers |
|---|---|
| `fs` | filesystem APIs (`fs`, `archive`, `diff`, `patch`, `user`, `group`, `module`, path I/O methods) |
| `net` | `net` and `dns` |
| `process` | `run`, `spawn`, `wait`, handle cancellation, effectful `process` APIs, `unix`, `linux`, `applet` |
| `env` | `env`, `cd`, `system` |
| `time` | `time` APIs and delayed `retry` |
| `io` | stdin/stdout APIs; also implies `fs`, `net`, `process`, and `env` |
| `error` | propagating an `Err` out of the proc |

A clause, including `[]`, is a checked upper bound: the body and every callee
must stay within it, or the checker reports `check.effect-violation` with the
call chain. `print` and `eprint` need no effect. Pure functions satisfy any
bound.

A private proc without a clause has its effects inferred from its body and
callees (recursion included). Callers see the inferred set. Exported procs,
module-contract entries, `cli main`, `proc main`, native tests, and streams
without a clause are unrestricted, so a restricted caller cannot call them.
Opaque callables and unresolved dependencies have unknown effects, which a
restricted caller cannot use either. Local `try` and `retry` capture removes
only the outward `error` requirement.

## 10. Commands And Scopes

### 10.1 Command statements

A command statement is a core command, a `run` form, or a fully qualified
standard-module call written in command style:

```xsh
print "building" $target
fs.mkdir build
fs.remove dist --missing-ok
json.write out.json (metadata)
```

Command style is available only for effectful standard APIs that return
`Result[Unit]`, and the statement propagates failure. A defaulted `Bool`
parameter may be passed as a flag, mapping kebab case to snake case
(`--missing-ok` means `missing_ok: true`). Everything else uses expression
calls. User procs are never called in command style. Core command names
(`print`, `eprint`, `cd`, `env`) cannot be redefined.

A statement that begins with an identifier followed by `|>` is a pipeline
expression, not a command.

### 10.2 Command arguments

Each command argument is one of:

- a **word** made of adjacent bare text, quoted strings, `${expr}`, and
  `$name`/`$name.field` parts. Adjacent parts with no whitespace form one
  argument. Words are never split, globbed, tilde-expanded, or brace-expanded.
- a **splice** `@name`, `@(expr)`, or `@g"glob"`, inserting each list element
  as its own argument.
- a **typed argument**: `(expr)`, an `f"..."`, `p"..."`, or `fp"..."` literal,
  or an unspaced expression chain containing a call or index, such as
  `input.display()` or `rows[0]`. Plain `record.field` is a word unless written
  `$record.field`, `${record.field}`, or `(record.field)`.

A standalone interpolation that evaluates to a `List` splices its elements,
as `@` does. Interpolation inside a larger word uses display conversion and
always contributes to that one argument (`-j${jobs}`, `"--out=$dir"`).

### 10.3 Output

`print` writes its arguments separated by single spaces plus a newline to
stdout, and `eprint` does the same on stderr. They accept displayable scalars
(`Str`, `Int`, `UInt`, `Float`, `Duration`, `Bool`, `Path`) and need no effect.
`Path` displays its text without normalizing or resolving. `--flush` as the
first argument writes straight to the inherited stream instead of the buffered
script output. `xsh` writes buffered output out before it starts or waits on
a child that shares the script's stdout or stderr, so script and child output
keep program order even when stdout is a pipe or file.

Script stdout and stderr are byte streams. Text APIs write UTF-8.
`io.write_stdout(text)` writes without a newline, and
`io.write_stdout_bytes(data)` writes bytes exactly, with no UTF-8 requirement.

### 10.4 `cd` and `env` scopes

```xsh
cd p"build" {
  run make
}

env CC=clang CFLAGS="-O2 -pipe" {
  run make
}

let version = cd (repo) { run.text git describe ? }?
let report = env (overlay) { collect_report()? }?
```

`cd path { ... }` runs its body with a different evaluator working directory.
`env NAME=value ... { ... }` and `env (overlay) { ... }` run it with
environment overrides; the overlay is a `Record` or `Map[Str, V]` whose values
use argv conversion (11.4), and `null` is rejected rather than meaning "unset".
Names and values must be valid environment entries without NUL.

In statement position a scope returns `Result[Unit]`. The parenthesized forms
`cd (path) { ... }` and `env (overlay) { ... }` may be used in value position
and return `Result[T]` of the body's tail. A `Result` tail stays nested.

The previous directory and environment are restored when the body ends for any
reason (completion, propagation, `return`, loop control, cancellation, or
failure), after the body's defers run. The scope's `Result` reports only
entering and restoring. Failures inside the body propagate to their ordinary
destination, so use `try` to capture the whole operation. Scopes never change
the host process's own cwd or environment. A live stream or handle cannot
escape a scope as its value, through an assignment, a `return`, or a `yield`;
consume it inside. A producer that yields from inside a scope keeps its own
directory and environment between pulls.

### 10.5 Environment access

- `env.Str.NAME -> Result[Str]`, `env.Path.NAME -> Result[Path]`, and
  `env.PathList.NAME -> Result[List[Path]]` read variables with static names.
  `env.get(name)`, `env.get_or(name, fallback)`, `env.int`, `env.bool`,
  `env.path`, and `env.path_list` read computed names.
- A `Str` lookup fails when the variable is missing or not valid UTF-8; bytes
  are never decoded lossily. Child processes still inherit non-UTF-8 values
  unchanged.
- `env.PATH` is a scoped mutable view with `prepend(path)`, `append(path)`,
  `pop()`, and `in`/`not in`. Its operands must be `Path` values.

## 11. Processes

### 11.1 Run forms

External programs run only through `run`. A bare `make -j4` is not a process
call and never searches `PATH`.

| Form | Value position | Statement position |
|---|---|---|
| `run cmd ...` | `Status` | asserts success; propagates `ProcessError` |
| `run.status cmd ...` | `Status`, never fails on exit status | status discarded |
| `run.text cmd ...` | `Result[Str, ProcessError]` (stdout) | |
| `run.bytes cmd ...` | `Result[Bytes, ProcessError]` (stdout) | |
| `run.capture --text cmd ...` | `Result[{status, stdout: Str, stderr: Str}, ProcessError]` | |
| `run.capture --bytes cmd ...` | `Result[{status, stdout: Bytes, stderr: Bytes}, ProcessError]` | |
| `run.stream --text cmd ...` | `Result[Stream[Str], ProcessError]` (stdout lines) | |
| `run.stream --bytes cmd ...` | `Result[Stream[Bytes], ProcessError]` | |

A trailing `?` applies to the whole run form: `run.text git rev-parse HEAD ?`.
The target is resolved as follows: a bare word with no `/` is looked up in
`PATH`; a target containing `/` is a relative or absolute path; a `Path` value
uses its native bytes; a `Str` value is UTF-8 and may not contain NUL. Failure
to find, access, or execute the target is a distinct `ProcessError` variant.

A grouped body may span lines when `(` is followed by a newline:

```xsh
run (
  make
  "ARCH=arm64"
  "-j${jobs}"
  Image
) ?
```

### 11.2 Success and failure

Plain `run` in statement position, and every byte pipeline in statement
position, fails with `ProcessError` on a nonzero exit, a signal death, a setup
failure, or a failed pipeline segment. In value position plain `run` and
`run.status` yield the `Status` as data. `run.text`, `run.bytes`, and
`run.stream` fail on an unsuccessful exit. `run.capture` returns `Ok(record)`
even for a nonzero exit, and fails only on setup, timeout, cancellation,
capture-limit, and decoding errors.

Diagnostics for failed commands include the working directory and a quoted
rendering of argv. Environment overlays are not shown.

### 11.3 Run options

Options follow the run form, before environment overlays:

```xsh
run --timeout=30s --cpumax=80 make check
let status = run.status --accept=[0, 1] grep -q pattern file
```

- `--timeout=DURATION` measures from spawn. When it expires, the child is
  terminated and the form fails with `Timeout`.
- `--cpumax=N` requests a CPU quota of `N` percent of one core (values above
  100 are allowed). Linux enforces it with cgroups v2 and fails with a
  `ProcessError` if that is unavailable. macOS ignores it, and other platforms
  reject it. In a pipeline it is allowed only on the first segment and covers
  the whole pipeline.
- `--accept=CODES` takes a non-empty `List[Int]` of unique codes in `0..=255`,
  evaluated once before spawning. An ordinary exit counts as success exactly
  when its code is in the list, including rejecting `0` if `0` is absent. The
  `Status` and captured records still report the actual code. A signal
  death is never accepted. A rejected exit is `Err(UnexpectedExit)` for
  `Result` forms and a propagated failure for status forms. A malformed
  dynamic list is a runtime error before any child starts. In a pipeline each
  segment applies its own list. The option requires the `error` effect.

### 11.4 Argv conversion

Every argv item is a byte string without NUL. Conversions:

| Value | Argv bytes |
|---|---|
| `Str` | UTF-8 |
| `Path` | native bytes |
| `Int`, `UInt` | decimal |
| `Bool` | `true` / `false` |
| `List[T]` | only via `@` or a standalone interpolation, one item per element |

`Null`, `Bytes`, records, maps, `Result`, `Status`, errors, handles, callables,
and `Unit` are rejected at check time where the type is known and at runtime
otherwise. Convert explicitly (`.display()`, `.utf8()?`, `.pid`). There is no
word splitting at any point: `run rm $file` passes exactly one argument
whatever `file` contains.

### 11.5 Capture

`run.text`, `run.bytes`, and `run.stream` capture stdout and inherit stdin and
stderr. `run.capture` captures stdout and stderr. Output is exact, with no
trailing newline stripped. `--text` requires valid UTF-8, while `--bytes`
decodes nothing. Each captured stream is limited to 16 MiB; exceeding the
limit terminates the child and fails with `CaptureLimit`. Captures never
deadlock on full pipes. A process stream yields lines as the child produces
them. Its completion check (exit status, `--accept`, decoding) can fail after
rows were consumed, and a consumer that stops early cancels and reaps the
child.

### 11.6 Pipelines, redirections, and environment

```xsh
run tar cf - src | run gzip -9 > $tarball
run make > $log 2> $errlog
run sort < $input > $output
run tool >& 2
run patch -p1 < (patch_bytes)
run CC=cc CFLAGS="-O2 -pipe" ./configure --prefix=/usr
```

Byte pipelines connect stdout to stdin; each segment must be its own `run`.
Redirection targets are typed path values or non-negative file descriptors
(`>& 2`). `>`/`2>` truncate and `>>`/`2>>` append. Stdin `<` also accepts
`Bytes`, sent exactly with no temporary file (empty bytes closes stdin at
once); a `Str` is a file path. `NAME=value` words before the target set the
child's environment only. Traces record argv, environment overlays, cwd,
segments, and redirections as structure, never as reconstructed shell
strings.

### 11.7 `spawn`, `wait`, and `cancel`

```xsh
let build = spawn run make all ?
let tests = spawn run make test ?
let statuses = wait [build, tests]?

let server = spawn run /srv/app/server --port 8080 ?
server.cancel(signal: "TERM", kill_after: 2s)?
```

- `spawn run ...` starts exactly one child immediately and returns
  `Result[ProcessHandle, ProcessError]`. It accepts the arguments, options,
  overlays, and redirections of plain `run`, but not pipelines, captures, or
  streams. `spawn command` starts a `Command` plan the same way.
- `wait handle` returns `Result[Status, ProcessError]`; `wait [h1, h2]` waits
  for every distinct handle in order and returns `Result[List[Status],
  ProcessError]`. After an error it keeps draining the remaining handles,
  then returns the first error without partial statuses.
- `handle.cancel(signal: "TERM", kill_after: 2s)` signals the child's process
  group, sends `SIGKILL` after `kill_after` if needed, reaps it, and returns
  `Result[Unit, ProcessError]`.
- Nonzero exits and signal deaths are `Status` data. Setup failures, timeouts,
  cancellation, and invalid handles are `ProcessError`. A trailing `?` applies
  to the whole `spawn`, `wait`, or `cancel` expression.
- A handle exposes `pid`, `command`, `argv`, and `detached`, which stay
  readable after the child is gone. The first `wait` or `cancel` consumes the
  child, and later use of any alias fails with `ProcessError.Unknown`.

### 11.8 Ownership of live resources

Process handles, network jobs (`NetJob`), and open streams are owned by the
lexical scope that created them. Returning a value that contains one, storing
it into an outer binding, or breaking it out of a loop transfers ownership
outward. When a scope exits, its owned non-detached handles are cancelled and
reaped and its owned network jobs are cancelled and drained, before the scope's
defers run. Detached handles are released to a background reaper instead. A
`NetJob` is consumed by its first `wait()` or `cancel()`, like a process
handle. These are process and transfer fan-out, not an async runtime: there
are no futures, callbacks, channels, `await`, or wait-any.

### 11.9 Command plans and builder blocks

```xsh
let plan = process.command {
  cwd = p"/srv/app"
  env = {RUST_LOG: "info"}
  timeout = 30s
  run /srv/app/server --port 8080
}
let status = process.run(plan)?
```

A builder block is accepted only by an API whose signature declares one.
Inside it, `name = expr` sets a builder field (not a variable), `let` and `var`
declare locals, and entries such as `run` are interpreted by the API. Unknown,
duplicate, missing, or invalid fields are check-time errors located in the
block. `process.command` accepts `cwd`, `env`, `stdin` (`Path` or `Bytes`),
`stdout`, `stderr`, `stdout_append`, `stderr_append`, `timeout`, `cpu_max`,
`accept`, `detach`, `new_session`, `ignore_hup`, and exactly one plain `run`
entry. `process.command_argv(target, argv)` builds the same plan from data; its
`argv` includes `argv[0]`. `process.run(plan)` returns `Ok(Status)` for any
completed process and `Err` for setup, timeout, or cancellation failures.
Pipelines, captures, and redirection syntax are not plan inputs.

## 12. Signals, Cancellation, And Exit Status

### 12.1 Cancellation

Every `run` command gets its own process group, and a byte pipeline shares one
group. When XSH receives `SIGINT` or `SIGTERM` with no matching hook, it
forwards the same signal to active child groups, waits a short grace period,
kills the remaining children with `SIGKILL`, cancels live handles and network
jobs, runs cleanup, and fails with `Canceled`; the script exits `3`. XSH does
not manage descendants that move to another process group or session.

OS signal handlers only record that a signal arrived. XSH code runs only at
checkpoints: between statements, at loop iterations, around deferred actions,
while waiting on processes, pipelines, and network jobs, during `time.sleep`,
and while scheduling or collecting parallel stream work. CPU-bound expression
evaluation and blocking host calls observe a signal at the next checkpoint.

### 12.2 Signal hooks

An entry script may declare one hook per signal at its top level:

```xsh
on SIGINT --pre-cancel=150ms [fs, process, error] {
  p"/tmp/build.interrupted".write("interrupted\n")?
  abort(130)
}
```

- Names may omit `SIG` and are case-insensitive: `HUP`, `INT`, `QUIT`, `TERM`,
  `USR1`, `USR2`, `ALRM`, `XCPU`, and `XFSZ` (where the platform has them).
  Numbers, `KILL`, `STOP`, `PIPE`, and job-control signals are rejected, and
  `TERM` and `SIGTERM` conflict.
- The effect list is required (`[]` for none). Hooks are not allowed in
  imported or loaded modules, cannot be exported, and are unsupported in
  `xshi`.
- A hook is armed when its statement executes. It sees procs, pure functions,
  and top-level values evaluated before it. A signal that arrived before
  arming is treated as having no hook.
- The first handled signal starts shutdown and runs its hook at most once,
  with its own defers. A second handled signal escalates: no hook re-entry,
  immediate `SIGKILL` of owned process groups, and remaining cleanup may be
  skipped.
- `--pre-cancel` (default `150ms`) is how long the hook may delay forwarding
  the signal to already running children. The signal is forwarded when the
  hook finishes or when the hook reaches a checkpoint after the budget. Work
  the hook itself starts is not sent the primary signal, so a hook can run
  orderly handoff commands, but it is killed on escalation.

Hook exit status: `abort(status)` in the hook commits `status`, still
cancelling owned children (`force: true` also skips defers). A hook that
finishes normally exits `3` for `INT` and `TERM` and `128 + signal` for other
signals. A hook that fails exits `3` with a traceback.

### 12.3 Exit statuses

| Status | Meaning |
|---|---|
| `0` | success |
| script-chosen `0..=255` | a final top-level `Int`/`UInt`, or `abort(status)` |
| `1` | `xsht lint` findings, `xsht fmt --check` mismatch, or `xsht test` failure |
| `2` | usage error, or a source, parse, or check failure (including invalid arguments to `cli main`) |
| `3` | runtime failure, top-level propagated `Err`, cancellation, or a failed hook |
| `4` | internal implementation error |
| `128 + signal` | tooling interrupted by a signal, or a non-`INT`/`TERM` hook that finished normally |

Tool and runtime failures take precedence over script-chosen statuses.
`xshi` statuses are defined in `docs/SPEC-INTERACTIVE.md`.

## 13. Structured Streams

### 13.1 Pipelines

A structured pipeline passes typed items through stages: `source |> stage |>
... |> stage`. It is unrelated to byte pipelines (`|`), and nothing in it is
text-split or globbed.

```xsh
let sources = fs.files(p"src", exts: ["c"])?
  |> where .size > 0
  |> map .path
  |> sort

let total = fs.files(p"src")? |> map .size |> sum()
```

The result of a pipeline depends on its last stage:

- a transformation stage yields `List[T]`, collected at the pipeline boundary;
- `collect()` yields `List[T]` explicitly;
- a terminal stage yields its own value (see 13.3).

Stage blocks see the current item as `.` (`where .kind == "file"`,
`map { .path.name }`) or bind it explicitly with `{ |item| ... }`. They may
contain statements followed by a tail.

A stage that is not a stream stage is a value call: a bare method name uses
the previous value as its receiver (`text |> split(",")` is `text.split(",")`),
and a qualified function receives it as the first argument. One whole argument
may be `_` to place the value explicitly: `data |> render(template, data: _)`.
A trailing `?` propagates that call's `Result`.

### 13.2 Laziness

Live sources produce items on demand: `fs.walk`, `fs.files`, `fs.dirs`,
`fs.children`, `Path.lines()`, `Path.bytes_lines()`, `run.stream`, `range(n)`
and `range(start, n)`, and `stream` producers. `Str.lines()` and
`Bytes.lines()` split an existing buffer.

`where`, `map`, `flat-map`, `tee`, `enumerate`, `take`, and `drop` process each
live item before the next is pulled. So do the folding terminals (`count`,
`sum`, `min`, `max`, `last`, `fold`, `reduce`, `reduce-by`, `each`, keyed
`count`, `group-by`, `unique-by`), which close the source at the first error.
`take`, `first`, `any`, and `all` stop pulling as soon as the answer is known
and close the source, so a producer's defers run. `sort`, `sort-by`,
`shuffle`, `batch`, `zip`, `par-map`, `table.print`, `collect()`, and binding a
pipeline with `let` materialize their input.

`for item in pipeline { ... }` consumes serial stages item by item without
building a list. `break`, `continue`, errors, and `take` close the source.

### 13.3 Stages

| Stage | Result |
|---|---|
| `where`, `map`, `flat-map`, `tee`, `enumerate`, `take(n)`, `drop(n)`, `unique-by`, `sort`, `sort-by`, `batch`, `zip(other)`, `repeat(n)`, `range`, `par-map` | stream of items (collected as `List`) |
| `collect()` | `List[T]` |
| `count()`, `count { key }` | `Int`; or `Map[Int]` of counts keyed by the key's text |
| `sum()` | `Int` |
| `min()`, `max()`, `first()`, `last()` | `Result[T]` (empty input is `Err`) |
| `any`, `all` | `Bool` |
| `fold(init) { |acc, item| ... }`, `reduce { |acc, item| ... }` | accumulator |
| `reduce-by(sum:/min:/max: true) { {key, value} }` | `Map` of reduced values |
| `group-by { key }` | list of `{key, items}` groups in first-seen order |
| `shuffle(seed)` | shuffled `List[T]` |
| `each { ... }`, `table.print(...)` | `Unit` |

The adapters `text.lines()`, `bytes.chunks(size)`, `json.lines()`, and
`json.stream()` are valid only as the first stage.

### 13.4 Callbacks and errors

- `where`, `any`, and `all` require a `Bool` callback result, `sort-by` a
  sortable key, and keyed `count` a `Str`, `Int`, or `Bool` key. A
  `Result` there is a type error: add `?` to propagate.
- `map` and `par-map` keep whatever the callback returns. Without `?`, a
  fallible callback yields `List[Result[T, E]]` and every item runs (collect
  all). With `?`, the first failure stops the stage and propagates
  (short-circuit).
- `each` and `tee` callbacks are statement bodies: a `Result[Unit]` failure
  propagates.
- `flat-map` accepts a `List` or `Stream` per item (a live stream is drained
  before the next item). A `Result` of a list is unwrapped, and `Err` fails the
  stage.
- `fold` and `reduce` callbacks must return exactly the accumulator's type and
  run serially. A `Result[T]` accumulator keeps `Err` values as data.
- `reduce-by` keeps one accumulator per key, and its callback must return the
  `{key, value}` record directly. `sum` adds numbers, or records field by
  field. Exactly one of `sum`, `min`, `max` must be true.
- `group-by` and `unique-by` compare whole key values (including `Result`s).
  `unique-by` keeps the first item per key.

`map`, `where`, `flat-map`, `each`, `tee`, `sort-by`, `group-by`, `unique-by`,
`any`, and `all` also accept a statically known function in place of a block:
`map(normalize)`, `sort-by(size_key, desc: true)`. It means exactly
`{ |item| normalize(item) }`. Dynamic callables and bound methods need an
explicit block.

### 13.5 Configuration and parallelism

Stage options are ordinary named arguments: `par-map(jobs: 8)`,
`sort-by(desc: true) .size`, `batch(count: 100, max_bytes: 65536)`,
`take(10)`. They evaluate once, in written order, when the stage starts, never
per item. `jobs`, `batch` limits, and chunk sizes must be positive; `take`,
`drop`, and `repeat` accept zero. `batch` needs at least one limit, closes a
batch at the first limit reached, and keeps a final short batch.
`batch(max_argv: true)` sizes batches to fit an argv.

`par-map` maps items on a bounded pool of workers (by default about one per
CPU) and keeps input order. When a worker's callback fails with `?`, no new
work is scheduled. Cancellation stops running workers through the ordinary
process cancellation rules. Every other stage runs serially. `each`,
`group-by`, and `count` reject `jobs:`.

`sort` and `sort-by` are stable. They order `Int`, `Str`, `Bool`, and `Path`
values, and records whose fields are themselves orderable (compared field by
field in field-name order). `desc: true` reverses. Keys of type `Any` are
ordered by their runtime value and fail if it is not orderable; other key
types are check-time errors.

### 13.6 Guidance

- Recursive walks follow filesystem order, which is not sorted. Add
  `|> sort-by .path` when order matters, including before `take` or `first`.
- For per-key counts or sums, use `reduce-by` or `count { key }`. `group-by`
  keeps every item.
- `par-map` pays for coordination; use it for heavy independent items such as
  hashing or running processes. Plain `map` is faster for cheap work.
- Binding a pipeline with `let` materializes it. Laziness and early stopping
  apply only when the pipeline is consumed in place by `for` or a terminal.
- Put `take` before `par-map` when a producer must stop early.
- Prefer builtin methods inside stage blocks to small helper functions; each
  user call has a cost per item.

## 14. JSON

JSON is a boundary format. Decode it at the edge, check the shape the program
relies on, and keep the rest of the program in typed values.

```xsh
type Package = {name: Str, version: Str, files: List[Str]}

let package = json.read(manifest_path)?.require(Package)?
for file in package.files {
  print f"${package.name}-${package.version}: ${file}"
}
```

`json.read` and `json.decode` return `Any`, because parsing proves only that
the text was JSON. `.require(Package)?` is the trust boundary (5.3). Integers
that fit decode as `Int` and other finite numbers as `Float`.

JSON-compatible values are `Null`, `Bool`, representable `Int`, finite
`Float`, `Str`, lists, `Str`-keyed maps and records of compatible values,
optional values (absent as `null`), and Str-backed enums (as their wire
strings). Everything else needs explicit conversion: `Path` (`.display()`),
`Bytes` (`.base64()`), `Digest` (`.hex()`), `Duration`, `Status`, `Result`,
errors, handles, command plans, ordinary enums, non-`Str`-keyed maps, and
non-finite floats.

`json.encode` and `json.write` emit ordinary JSON with record keys in sorted
order (`pretty: true` indents deterministically). `json.encode_lines` and
`json.write_lines` emit one compact value per line, each followed by a newline.

Not every temporary value needs a schema. A record literal is already typed:

```xsh
json.write(log_path, {service: "worker", event: "done", ok: status.ok})?
```

Programs that operate on unknown JSON (formatters, filters, validators) branch
on runtime shape with type patterns:

```xsh
error JsonShape = NotScalar(message: Str)

pure scalar_label(v: Any) -> Result[Str, JsonShape] {
  match v {
    _ is Null => "null"
    b is Bool => if b { "true" } else { "false" }
    i is Int => f"integer ${i}"
    f is Float => f"float ${f}"
    s is Str => f"string of ${s.count_chars()} characters"
    _ => Err(JsonShape.NotScalar(message: "expected a scalar"))
  }
}
```

Path helpers address nested values with segment lists of `Str` keys and
non-negative `Int` indices; `[]` is the root. `json.get(value, path)` returns a
`Result`, and `json.get(value, path, fallback)` returns the fallback when the
path is missing. `json.set` updates or inserts at an existing parent, and
`json.remove` fails when the target is absent. Path errors use `json-path`.

Treat `Any` as a short-lived boundary value. Use `.require(T)?` when the
program knows the shape, and keep dynamic matching inside small helpers.

## 15. Standard Modules

Standard modules are always in scope. Operations on a value are methods on its
type; modules hold factories and operations with no natural receiver. The
complete, generated index is `docs/reference/stdlib.md`, and
`xsht api module:NAME`, `xsht api method:Type.name`, and
`xsht api search:TERMS` give full signatures and details.

| Module | Role |
|---|---|
| `fs` | filesystem walks, metadata, reads and writes, atomic writes, copies, installs, locks, temp files, and `FsRoot` confined directory capabilities |
| `path` | absolute-path computation; most path operations are `Path` methods |
| `env` | environment variables and the scoped `env.PATH` view |
| `process` | process listing, `Command` plans, `process.run`/`process.spawn`, `which` |
| `io` | script stdin and unbuffered stdout |
| `json`, `ini` | data formats |
| `text`, `bytes` | stream adapters and byte-level helpers; text operations are `Str` methods |
| `regex` | runtime regex compilation; `Regex` methods match, find, capture, and replace |
| `hash` | MD5, SHA-1, SHA-256, and SHA-512 digests of bytes and files |
| `archive` | tar, cpio, and zip listing, extraction, creation, and compression |
| `diff`, `patch` | unified diffs and confined patch application |
| `net`, `dns` | HTTP(S) requests, downloads, uploads, batches, pooled clients, `NetJob`; DNS lookups |
| `time` | clock, `sleep`, measurement, duration helpers |
| `cli` | argument parsing beyond `cli main` |
| `module` | runtime module loading |
| `error` | `error.fail` validation failures |
| `map`, `set` | empty-map factory; `Str` sets as `Map[Bool]` |
| `system`, `cpu`, `user`, `group` | host identity and resources |
| `unix`, `linux`, `elf` | privileged and platform-specific host operations, ELF inspection |
| `mime`, `shlex`, `tui`, `utils` | MIME lookup, shell quoting for display, terminal styling, process-scoped cache |
| `template` | text templates |
| `test` | native test support (17) |
| `applet` | internal host support for the shipped core utilities; not a stable user API |

Contracts worth knowing without consulting the reference:

- Standard API failures are structured errors located at the call site.
- `fs.walk`, `fs.files`, and `fs.dirs` skip hidden entries and honor
  `.gitignore` by default (`hidden: true`, `gitignore: false` change that).
  With `stat: false`, metadata fields are unavailable and reading one fails
  with `metadata-unavailable` instead of returning a placeholder.
- `FsRoot` methods resolve relative paths against an open directory handle
  and refuse absolute paths, escaping `..`, and escaping symlinks. They confine
  path resolution, not the process.
- Archive extraction and `patch.apply` reject absolute paths, parent
  traversal, symlink escapes, and overwrites unless asked.
- `time` has no civil-time formatter; run `date` for locale-aware output.
- Regex syntax is the common Rust regex surface without Unicode property
  classes. Match offsets are byte offsets.

### 15.1 `template`

The `template` module renders text templates in the style of Go's
`text/template` from typed data. Its contract is published with the module;
see `xsht api module:template`.

## 16. Tools

### 16.1 Commands

| Command | Purpose |
|---|---|
| `xsh SCRIPT [--] ARGS...`, `xsh -- SCRIPT ARGS...` | run a script (the second form suits shebang lines) |
| `xshi` | interactive shell (`docs/SPEC-INTERACTIVE.md`) |
| `xsht check [--summary] [--annotate[=CLASSES]] [PATH...]` | parse, type-check, and validate scripts |
| `xsht fmt [--check] [FILE...]` | format |
| `xsht lint [--fix] [--only RULE,...] [--runless] [FILE...]` | quality checks and safe fixes |
| `xsht test [OPTIONS] [FILTER]` | run native tests (17) |
| `xsht trace [--raw] [--trace-format text\|jsonl\|flamegraph] [--trace-file PATH] SCRIPT ARGS...` | run with tracing |
| `xsht api [QUERY...]` | query language and standard-library reference data |
| `xsht ast SCRIPT` | print the parse tree |
| `xsht grep PATTERN [FILE...]`, `xsht refactor PATTERN REPLACEMENT [FILE...]` | structural search and rewrite |

`xsht help [COMMAND]` and `COMMAND --help` print generated usage. `xsh` is a
plain runner and rejects tracing flags. Runtime stdout and stderr are never
decorated; diagnostics go to stderr.

Without paths, `check`, `lint`, `grep`, and `refactor` process every `.xsh`
file under the current directory plus `include` entries from the nearest
`xsht-config.ini`, filtered by its `exclude` patterns. Each file uses the
nearest config among its ancestors (for `module_path`, `format.line-width`
(default 120), lint options, and `check.annotate`).

`xsht check` runs exactly the checks that execution runs before evaluating
anything, plus a lowering check that also needs no execution. Dynamic
boundaries are always enforced; there is no permissive mode.
`--annotate` rewrites requested scripts in place with safe inferred
annotations (defaulted parameters, inferred returns, exported bindings, and
optionally local bindings), only when checking reports no diagnostics.

`xsht lint` reports `lint.*` findings and the checker findings that carry fixes
(such as `check.bool-statement`). `--fix` applies only fixes that preserve
behavior and comments; a rewritten file must parse and check with no new
diagnostics. `--only RULE,...` limits reporting and fixing to the named codes,
so `xsht lint --fix --only check.bool-statement` inserts `assert` where Bool
statements appear. Each finding names its rule, and a fix is withheld (with an
explanation) whenever equivalence cannot be proved.

`xsht grep` patterns are XSH expressions where uppercase identifiers are
metavariables (`X.push(ITEM)`, `ARGS..` for zero or more arguments). Matching
respects expression boundaries and ignores whitespace and comments.
`xsht refactor` substitutes the captured text into a replacement template;
`--dry-run` prints a diff.

### 16.2 Traces and tracebacks

Tracing records the runtime graph. Events have an id, a parent id, a depth, a
kind, a source span when available, a name, and timing. Event kinds cover
script, proc, and pure entry and exit, core and module calls, `run`
start/end, spawn/wait/cancel (with handle ids), cwd and env scopes, stream
stages and parallel jobs, retries, signal receipt, hooks, forwarding and
escalation, result propagation, and runtime errors. Argv, environment
overlays, and statuses are structured fields, never reconstructed shell
strings. Network events carry only ids, timing, status, sizes, and error
kinds, never bodies, headers, queries, or credentials. Trace output is
separate from script stdout and goes to stderr unless `--trace-file` is given.
Without `--raw`, `xsht trace` prints a summary of call counts, duration
percentiles, and the slowest operations.

A runtime failure or top-level `Err` prints a traceback with the failing span,
the operation, the error variant and message, context frames, the cause chain,
the user call stack with call sites, and, for process failures, the executable,
argv, working directory, and status.

## 17. Native Tests

`xsht test` discovers tests in `tests/**/*.xsh` and `showcase/tests/**/*.xsh`
under the current directory. Missing roots mean zero tests. Test ids have the
form `tests/file.xsh::name`.

```xsh
use version

test parses_semver { |ctx|
  let v = version.parse("1.2.3")?
  assert v.major == 1
  assert v.minor == 2, f"minor of ${ctx.name}"
}

test runs_script [fs, process, error] { |ctx|
  let result = test.run_script(ctx, "print \"hi\"")?
  assert result.stdout == "hi\n"
}
```

A test file is shaped like a module. Its top level may contain `use`, `const`,
`let`, `type`, `enum`, `error`, `proc`, `pure`, `stream`, `test`, and `export`
declarations; commands, mutation, and control flow are rejected. Imports and
module bindings are initialized before each test.

`test NAME [effects] { |ctx| ... }` declares a test whose body is checked as a
`Result[Unit]` proc body. The block parameter is an immutable `TestContext`
(with `name`, `file`, and `temp_root` fields); it may be `_` or omitted. Without
an effect clause a test is unrestricted. Test names share the top-level
namespace but are not callable, exportable, or nestable, and importing or
running a file never executes them. A test fails when its body propagates an
`Err`, including a failed `assert`.

Each test runs in a fresh evaluator with its own captured stdout and stderr,
working directory and environment, mock registry, call log, and temporary
root. The `test` module provides skip and fail helpers, temporary paths,
files, and directories, whole-script runners (`test.run_script`,
`test.run_xsh`, `test.run_xsht_trace`, which return status and captured
output), and mocks for `dns.*` and `net.*` operations matched by partial
argument records. When an operation has mocks and none matches, the call
fails with an unmatched-mock error; operations without mocks use the real
host.

## 18. Not In XSH

These are deliberately absent:

- shell-string execution, `eval`, implicit word splitting, implicit globbing,
  tilde and brace expansion;
- truthiness and implicit conversions between strings, numbers, and Booleans;
- exceptions and `try`/`catch` (failures are `Result` data);
- futures, `await`, callbacks, event loops, and wait-any;
- generic functions and expression-level type arguments;
- first-class command blocks and script-level job control;
- tagged JSON.

## Appendix A. Grammar

Terminals in capitals: `IDENT` is an expression identifier, `PROC_IDENT` may
also contain `-`, `FIELD_LABEL` is an identifier or keyword used as a label,
`NEWLINE` is a line break that ends a statement (2.5), and the literal tokens
are as described in 2.6. Operator restrictions from 6.1 (required grouping and
redundant parentheses) apply on top of this grammar.

### A.1 Programs and declarations

```ebnf
program        = statement* EOF ;
terminator     = NEWLINE | ";" ;

statement      = use_stmt | export_stmt | const_stmt | let_stmt | var_stmt
               | assign_stmt | proc_def | pure_def | stream_def | cli_main
               | test_def | type_def | enum_def | error_def | signal_hook
               | return_stmt | yield_stmt | break_stmt | continue_stmt
               | if_stmt | while_stmt | for_stmt | loop_stmt | match_stmt
               | guard_stmt | guard_let_stmt | with_stmt | defer_stmt
               | assert_stmt | command_stmt | expr_stmt ;

use_stmt       = "use" module_path ( "as" IDENT )? terminator ;
module_path    = module_segment ( "." module_segment )* ;
module_segment = IDENT | PROC_IDENT ;
export_stmt    = "export" ( const_stmt | let_stmt | proc_def | pure_def
               | stream_def | type_def | enum_def | error_def ) ;

const_stmt     = "const" IDENT ( ":" type_expr )? "=" expr terminator ;
let_stmt       = "let" binding_target ( ":" type_expr )? "=" expr_or_run terminator ;
var_stmt       = "var" binding_target ( ":" type_expr )? "=" expr_or_run terminator ;
binding_target = IDENT | "{" ( destructure_field ( "," destructure_field )* ","? )? "}" ;
destructure_field = FIELD_LABEL ":" binding_target | IDENT | ".." ;
assign_stmt    = assign_target assign_op expr_or_run terminator ;
assign_target  = IDENT ( "." FIELD_LABEL | "[" expr "]" )* ;
assign_op      = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;

type_def       = "type" IDENT type_params? "=" type_body terminator ;
type_params    = "[" IDENT ( "," IDENT )* "]" ;
type_body      = type_expr | record_schema | module_contract ;
record_schema  = "{" schema_field ( "," schema_field )* ","? "}" ;
schema_field   = FIELD_LABEL ":" type_expr ( "=" expr )? ;
module_contract = "module" "{" contract_entry* "}" ;
contract_entry = "export" "optional"? contract_kind terminator? ","? ;
contract_kind  = "let"? IDENT ":" type_expr
               | "proc" IDENT "(" param_list? ")" effect_list? "->" type_expr
               | "pure" IDENT "(" param_list? ")" "->" type_expr ;

enum_def       = "enum" IDENT ( ":" "Str" )? "{" enum_variant ( "," enum_variant )* ","? "}" terminator ;
enum_variant   = IDENT ( "(" ( type_expr ( "," type_expr )* ","? )? ")" )? ( "=" expr )? ;

error_def      = "error" IDENT "=" error_variant ( "|" error_variant )* terminator ;
error_variant  = IDENT "(" ( schema_field ( "," schema_field )* ","? )? ")" ( ":" IDENT ( "," IDENT )* )? ;

proc_def       = "proc" PROC_IDENT "(" param_list? ")" effect_list? ( "->" type_expr )? block ;
pure_def       = "pure" IDENT "(" param_list? ")" ( "->" type_expr )? block ;
stream_def     = "stream" IDENT "(" param_list? ")" effect_list? "->" "Stream" "[" type_expr "]" block ;
cli_main       = "cli" "main" "(" param_list? ")" effect_list? ( "->" type_expr )? block ;
test_def       = "test" IDENT effect_list? block ;
param_list     = param ( "," param )* ","? ;
param          = IDENT ( ":" type_expr ( "=" expr )? | "=" expr )
               | "..." IDENT ":" type_expr ;
effect_list    = "[" ( IDENT ( "," IDENT )* )? "]" ;

signal_hook    = "on" IDENT hook_option* effect_list block ;
hook_option    = "--pre-cancel=" DURATION ;
```

### A.2 Statements

```ebnf
block          = "{" block_params? statement* "}" ;
block_params   = "|" ( IDENT | "_" ) ( "," ( IDENT | "_" ) )* "|" ;

if_stmt        = "if" condition block ( "else" "if" condition block )* ( "else" block )? ;
condition      = expr | "let" pattern "=" expr ;
while_stmt     = "while" condition block ;
for_stmt       = "for" binding_target "in" expr block ;
loop_stmt      = "loop" block ;
match_stmt     = "match" expr "{" match_arm* "}" ;
match_arm      = pattern ( "if" expr )? "=>" ( statement | block ) ","? ;

guard_stmt     = "guard" expr "else" block ;
guard_let_stmt = "guard" "let" binding_target ( ":" type_expr )? "=" expr_or_run "else" block ;
with_stmt      = "with" with_binding ( "," with_binding )* ","? block "else" block ;
with_binding   = IDENT "=" expr ;

postfix_guard  = ( "when" | "unless" ) expr ;
return_stmt    = "return" expr_or_run? postfix_guard? terminator ;
yield_stmt     = "yield" ( expr_or_run | "@" expr ) postfix_guard? terminator ;
break_stmt     = "break" expr? postfix_guard? terminator ;
continue_stmt  = "continue" postfix_guard? terminator ;

defer_stmt     = "defer" ( block | expr_or_run ) terminator ;
assert_stmt    = "assert" expr ( "," expr )? terminator ;
expr_stmt      = expr_or_run terminator ;
```

### A.3 Expressions

```ebnf
expr_or_run    = expr | run_form "?"? ;
expr           = fallback ;
fallback       = logical ( "??" fallback )? ;
logical        = equality ( ( "and" | "or" ) equality )* ;
equality       = ordering ( ( "==" | "!=" ) ordering | "is" pattern )* ;
ordering       = additive ( ( "<" | "<=" | ">" | ">=" | "in" | "not" "in" ) additive )* ;
additive       = multiplicative ( ( "+" | "-" ) multiplicative )* ;
multiplicative = unary ( ( "*" | "/" | "%" ) unary )* ;
unary          = ( "!" | "-" ) unary | postfix ;
postfix        = primary postfix_op* ;
postfix_op     = "." FIELD_LABEL | "?." FIELD_LABEL
               | "." "require" "(" type_expr? ")"
               | "[" expr "]" | "?[" expr "]"
               | "[" expr? ".." expr? "]" | "?[" expr? ".." expr? "]"
               | call_args | "?" ;
call_args      = "(" ( arg ( "," arg )* ","? )? ")" ;
arg            = expr | FIELD_LABEL ":" expr | IDENT ":" | "..." expr | "@" expr ;

primary        = literal | IDENT | list_lit | record_lit | map_lit | map_comp
               | if_expr | match_expr | try_expr | retry_expr | ctx_expr
               | scope_expr | spawn_expr | wait_expr | handler_expr | value_block
               | "(" expr ")" ;
literal        = "null" | "true" | "false" | INT | FLOAT | DURATION | STRING
               | RAW_STRING | FMT_STRING | BYTES | REGEX | PATH | PATH_FMT | GLOB ;

list_lit       = "[" ( list_item ( "," list_item )* ","? )? "]"
               | "[" expr comp_clauses "]" ;
list_item      = expr | "@" expr ;
record_lit     = "{" ( record_field ( "," record_field )* ","? )? "}" ;
record_field   = FIELD_LABEL ":" expr | STRING ":" expr | IDENT
               | field_path ":" expr | "..." expr ;
map_lit        = "{" map_entry ( "," map_entry )* ","? "}" ;
map_entry      = "[" expr "]" ":" expr | FIELD_LABEL ":" expr | "..." expr ;
map_comp       = "{" ( field_path | "[" expr "]" ) ":" expr comp_clauses "}" ;
comp_clauses   = "for" binding_target "in" expr ( "for" binding_target "in" expr | "if" expr )* ;
field_path     = FIELD_LABEL ( "." FIELD_LABEL )* ;
value_block    = "{" statement+ "}" ;

if_expr        = "if" condition "{" block_body "}" ( "else" "if" condition "{" block_body "}" )*
                 "else" "{" block_body "}" ;
match_expr     = "match" expr "{" ( pattern ( "if" expr )? "=>" expr ","? )* "}" ;
block_body     = statement* ;
try_expr       = "try" block ;
retry_expr     = "retry" "[" ( expr ( "," expr )* ","? )? "]" ( "on" "(" pattern ")" )? block ;
ctx_expr       = "ctx" expr block ;
scope_expr     = ( "cd" | "env" ) "(" expr ")" block ;
handler_expr   = postfix "??" "{" "|" ( IDENT | "_" ) "|" statement* "}" ;
spawn_expr     = "spawn" ( run_form | expr ) ;
wait_expr      = "wait" expr ;
```

### A.4 Patterns

```ebnf
pattern        = alias_pattern ( "|" alias_pattern )* ;
alias_pattern  = primary_pattern ( "as" IDENT )* ;
primary_pattern = "_" | IDENT | literal | type_pattern | facet_pattern
               | constructor_pattern | variant_pattern | record_pattern
               | list_pattern | "(" pattern ")" ;
type_pattern   = ( "_" | IDENT ) "is" type_expr ;
facet_pattern  = "is" IDENT ;
constructor_pattern = qualified_name "(" ( pattern ( "," pattern )* )? ")" ;
variant_pattern = qualified_name record_pattern ;
record_pattern = "{" ( record_pattern_field ( "," record_pattern_field )* ","? )? "}" ;
record_pattern_field = FIELD_LABEL ":" pattern | IDENT | ".." ;
list_pattern   = "[" ( pattern ( "," pattern )* ( "," list_rest )? | list_rest )? ","? "]" ;
list_rest      = ".." IDENT? ;
qualified_name = IDENT ( "." IDENT )* ;
```

### A.5 Types

```ebnf
type_expr      = qualified_name type_args?
               | type_expr "?" ;
type_args      = "[" type_expr ( "," type_expr )* "]" ;
```

### A.6 Commands and processes

```ebnf
command_stmt   = command "?"? terminator ;
command        = module_command | core_command | run_form ;
module_command = IDENT "." IDENT command_arg* ;
core_command   = ( "print" | "eprint" ) command_arg*
               | "cd" command_arg block
               | "env" env_assignment* block
               | "env" "(" expr ")" block ;
env_assignment = IDENT "=" command_arg ;

command_arg    = word | splice | typed_arg ;
word           = word_part+ ;
word_part      = BARE_TEXT | STRING | "${" expr "}" | "$" IDENT ( "." FIELD_LABEL )* ;
splice         = "@" ( IDENT | "(" expr ")" | GLOB ) ;
typed_arg      = "(" expr ")" | FMT_STRING | PATH | PATH_FMT | expr_chain ;

run_form       = run_head run_option* env_assignment* run_body ( "|" run_form )? redirection* ;
run_head       = "run" | "run.status" | "run.text" | "run.bytes"
               | "run.capture" capture_mode | "run.stream" capture_mode ;
capture_mode   = "--text" | "--bytes" ;
run_option     = "--timeout=" command_arg | "--cpumax=" command_arg | "--accept=" command_arg ;
run_body       = command_arg command_arg*
               | "(" NEWLINE ( command_arg | NEWLINE )+ ")" ;
redirection    = ( "<" | ">" | ">>" | "2>" | "2>>" ) command_arg | ">&" INT ;
```

`expr_chain` is an unspaced expression containing a call or index, and
`BARE_TEXT` is a run of characters that are not whitespace, quotes, `$`, `@`,
`(`, `)`, `{`, `}`, `;`, `|`, `<`, or `>`.
