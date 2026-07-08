# Tickets

Narrow, actionable bugs observed across the xsh runtime and standard modules.
Each entry describes the symptom, a minimal reproduction, the likely area, and
any known workarounds. When a ticket is resolved, delete it.

## Open

### `xsht lint --fix` should remove provably needless local annotations

**Symptom**

Package scripts and applets often carry annotations that the checker can already
infer:

```xsh
export let name: Str = "zlib"
export let deps: List[Str] = ["musl"]
let out: Path = p"build/out"
var argv: List[Str] = ["cc", "-O2"]
```

These are noise in idiomatic xsh. They make package definitions feel more
ceremonial than necessary, and they obscure the cases where annotations are
actually important: dynamic data validation, exported contracts, empty custom
typed accumulators, or current lowering limitations.

**Desired behavior**

Add a lint, likely `lint.needless-annotation`, that reports annotations on
`let`/`var` bindings when removing the annotation preserves the checked type.
When `--fix` is supplied, remove only the annotation and leave the initializer
and surrounding formatting intact:

```xsh
let name: Str = "zlib"
let deps: List[Str] = ["musl", "zlib"]
var argv: List[Str] = ["cc", "-O2"]
let source: Path = p"src/main.c"
```

becomes:

```xsh
let name = "zlib"
let deps = ["musl", "zlib"]
var argv = ["cc", "-O2"]
let source = p"src/main.c"
```

**Autofix criteria**

Be conservative. The fix should be offered only when the checker can prove that
the annotation is redundant.

Safe initial cases:

- Scalar literal initializers: `Str`, `Bool`, `Int`, `Float`, `Duration`,
  `Path`, `Bytes` where the initializer syntax already fixes the type.
- Non-empty homogeneous list literals, including path literals:
  `List[Str] = ["a"]`, `List[Path] = [p"a"]`.
- Non-empty list literals that intentionally infer to the same widened type the
  annotation names.
- Initializers whose inferred concrete type exactly equals the annotation.
- Exported value bindings are fixable only if the module export type remains
  identical after removing the annotation.

Skip or warn without autofix:

- Dynamic/schema boundaries, e.g. `let name: Str = record.get("name")?` or
  `let rows: List[Record] = json.read(path)?`.
- Empty collection initializers unless a later-flow/type-fact pass proves the
  annotation is not needed: `var rows: List[Package] = []`.
- Empty custom typed accumulators such as `var records: List[PnpRecord] = []`;
  these often document the intended shape and may be required to avoid unused
  type declarations.
- Function parameters, return types, type declarations, module contracts, and
  `proc main(...argv: List[Str])`.
- Cases where the annotation keeps the program on a currently supported compact
  lowering path. A known example is annotation removal in
  `packages/repo/m4/files/m4.xsh`, which can make `main` require compact
  lowering through an unsupported helper body.
- Any case where removing the annotation introduces diagnostics, changes the
  exported module signature, changes strict-dynamic warnings, or changes lowered
  execution behavior.

**Implementation notes**

- Reuse checker facts rather than string heuristics. For each annotated local
  binding, compute the initializer type as if the annotation were absent and
  compare it with the annotation type.
- For exported bindings, compare the resulting module export type before/after
  the proposed removal.
- For `--fix`, use source spans for only the `: Type` portion of the binding.
  Do not rewrite the initializer.
- The first implementation can skip empty `[]`/`{}` entirely. A later pass can
  use assignment/use facts to prove empty accumulators are safe to fix.
- The lint should explain why a nearby annotation was not fixable when useful
  during development, but normal output should avoid noisy notes for skipped
  cases.

**Minimal tests**

Fixable:

```xsh
let name: Str = "pkg"
let ok: Bool = true
let source: Path = p"src/main.c"
let deps: List[Str] = ["musl"]
var argv: List[Str] = ["cc", "-O2"]
export let rel: Str = "1"
```

Not fixable:

```xsh
let name: Str = metadata.get("name")?
let rows: List[Record] = json.read(index)?
type Entry = {name: Str}
var entries: List[Entry] = []
proc main(...argv: List[Str]) [error] {}
```

Regression coverage should include both `xsht lint` diagnostics and
`xsht lint --fix` output, plus a check that the fixed source still passes
`xsht check` and has the same exported module contract where exports are
involved.

### `in` should either support `Path` or reject it at check time

**Symptom**

`in` works as expected for list membership and string substring checks:

```xsh
print ("a" in ["a", "b"])
print ("musl" in "ld-musl-aarch64")
```

But `Path in Path` currently typechecks and then fails at runtime:

```xsh
proc main() [error] {
  print (p"usr/lib" in p"usr/lib/libz.so.1")
}

main()?
```

Observed failure:

```text
err[runtime.error]: invalid lowered binary operation
  /tmp/xsh-in-check.xsh:2:10
    print (p"usr/lib" in p"usr/lib/libz.so.1")
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: type-error: invalid lowered binary operation
```

**Desired behavior**

Either:

- support `Path in Path` with clearly documented semantics, likely substring
  containment on display-normalized path text, or
- reject the expression during checking with a precise diagnostic.

Given xsh's path ergonomics goals, supporting it is preferable if the semantics
are not surprising.

**Minimal tests**

Add checker/runtime coverage for:

```xsh
test.ok("lib" in "usr/lib/libz.so")?
test.ok("libz.so" in [ "libz.so", "libc.so" ])?
test.ok(p"usr/lib" in p"usr/lib/libz.so")?
test.eq(p"bin" in p"usr/lib/libz.so", false)?
```

### `xsht lint --fix` should rewrite simple `.contains(...)` membership to `in`

**Symptom**

Older package code often spells membership or substring checks through
`.contains(...)`:

```xsh
if deps.contains("musl") {
  ...
}

if line.contains("=") {
  ...
}

if ["prepare", "build"].contains(mode) {
  ...
}
```

The preferred idiom is the built-in `in` operator:

```xsh
if "musl" in deps {
  ...
}

if "=" in line {
  ...
}

if mode in ["prepare", "build"] {
  ...
}
```

**Desired behavior**

Add a lint, likely `lint.prefer-in`, that reports simple `.contains(...)`
calls on receivers where `in` is semantically equivalent. With `--fix`, rewrite
the expression while preserving grouping where needed.

**Autofix criteria**

Safe initial cases:

- `list.contains(value)` -> `value in list`
- `str.contains(needle)` -> `needle in str`
- literal-list receivers, e.g. `["a", "b"].contains(x)` -> `x in ["a", "b"]`
- negated cases, e.g. `! items.contains(x)` -> `! (x in items)` unless parser
  precedence makes the parentheses unnecessary and unambiguous
- method chains with simple receivers, e.g. `path.display().contains("/")` ->
  `"/" in path.display()`

Skip or warn without autofix:

- Types where `.contains(...)` does not match `in` semantics.
- Receivers or arguments with side effects unless evaluation order is proven
  unchanged.
- Cases where the checker accepts `.contains(...)` but `in` currently lowers
  incorrectly. Known example: `Path in Path` typechecks but fails at runtime;
  see the adjacent `Path` membership ticket.

**Minimal tests**

Lint and autofix coverage should include:

```xsh
if names.contains(name) {}
if ! names.contains(name) {}
if ["a", "b"].contains(name) {}
if text.contains("needle") {}
if path.display().contains("/") {}
```

The fixed output should pass `xsht check` and preserve behavior.

### `xsht fmt` should preserve readable multiline record/type shapes

**Symptom**

Running `xsht fmt` over the package repo produced syntactically valid output but
made several idiomatic package shapes less readable.

Examples from `packages/pm/make.xsh`:

```xsh
export type CMultiTarget = {
  tasks: List[MakeTask],
  groups: Map[CompileTasks],
  outputs: Map[Path],
  deps: List[Str],
}
```

was collapsed to:

```xsh
export type CMultiTarget = {tasks: List[MakeTask], groups: Map[CompileTasks], outputs: Map[Path], deps: List[Str]}
```

Small API type declarations are much easier to scan in multiline form,
especially when they are part of a public helper surface.

The formatter also rewrote package build calls from the compact, natural shape:

```xsh
let tool = make.c_program({
  cc,
  triple,
  cflags,
  defs,
  includes,
  root: p".",
  sources,
  out_dir: p"obj",
  out: p"obj/tool",
  libs: [],
  ldflags: [],
  deps: [],
})
```

to:

```xsh
let tool = make.c_program(
  {
    cc,
    triple,
    cflags,
    defs,
    includes,
    root: p".",
    sources,
    out_dir: p"obj",
    out: p"obj/tool",
    libs: [],
    ldflags: [],
    deps: [],
  },
)
```

That adds vertical noise and makes the PM make call shape feel less natural.

It also broke fluent chains in a hard-to-read way:

```xsh
return src.display().replace("/", "_").replace(".cxx", ext).replace(".cpp", ext).replace(".cc", ext).replace(
  ".c",
  ext,
).replace(".S", ext).replace(".s", ext)
```

And it rewrote a raw string literal in `packages/pm/build.xsh` from
`r"""..."""` to an escaped normal triple string. That may preserve behavior, but
formatting should not change literal flavor unless the source syntax actually
requires it.

**Desired behavior**

Keep existing multiline record/type shapes when they are already multiline.
Prefer multiline formatting for:

- record type declarations with more than a small number of fields
- record type declarations with nested generic types
- public/exported type declarations
- record literals passed as the sole argument to a call, especially
  `make.c_program({ ... })`, `make.c_multi_program({ ... })`, and similar
  builder-style APIs

For builder-style calls with a single record literal argument, preserve or
produce:

```xsh
call({
  field: value,
})
```

not:

```xsh
call(
  {
    field: value,
  },
)
```

For fluent method chains, prefer either one line when short or a consistent
chain layout when long:

```xsh
return src.display()
  .replace("/", "_")
  .replace(".cxx", ext)
  .replace(".cpp", ext)
```

rather than breaking one middle call and then continuing the chain on the same
line.

For string literals, preserve raw/triple/raw-triple literal style when possible.
Escaping a raw literal into a normal literal should be treated as a semantic
rewrite, not ordinary formatting.

**Minimal tests**

Add formatter fixture coverage for:

```xsh
export type CMultiTarget = {
  tasks: List[MakeTask],
  groups: Map[CompileTasks],
  outputs: Map[Path],
  deps: List[Str],
}

let target = make.c_program({
  cc,
  triple,
  cflags,
  defs,
  includes,
  root: p".",
  sources,
  out_dir: p"obj",
  out: p"obj/tool",
  libs: [],
  ldflags: [],
  deps: [],
})

let value = path.display().replace("/", "_").replace(".cxx", ext).replace(".cpp", ext).replace(".cc", ext)

let script = r"""print f"${value}"
"""
```

The fixed-point formatted output should preserve the multiline record type,
preserve the builder-call shape, use a coherent chain layout, and keep the raw
string literal raw.
