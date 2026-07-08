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
