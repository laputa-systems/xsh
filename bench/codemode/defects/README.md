# Product defects found by the experiment

Each defect has a minimal reproducer that runs against the frozen toolchain
(`.work/codemode-runs/toolchain`, built from `c68ac9594`).

## D1: a facet declared in an imported module never matches

```sh
cd bench/codemode/defects/d1-imported-facet
xsht check main.xsh   # passes
xsh main.xsh
```

Observed:

```text
only the variant pattern matched
retry on the facet: -1 after 1 attempts
```

Expected, by SPEC 4.10 ("`is Facet` matches any variant that implements a facet") and
the checker's own suggestion of `lib.Transient` for an unqualified `Transient`:

```text
facet pattern matched
retry on the facet: 3 after 3 attempts
```

`lib.LibError.Down` declares `: Transient`; inside `lib.xsh` the facet matches, but an
importer's `Err(is lib.Transient)` and `retry ... on (is lib.Transient)` never do, so the
retry silently runs once. Variant patterns (`lib.LibError.Down {..}`) do match.

`narrowed/` shows that the error value keeps its facet: wherever the error was built,
`error is Transient` inside `lib.xsh` is true, and `is lib.Transient` in the importer is
false. The fault is in matching a qualified facet pattern, so look first at how the
importer lowers `is lib.Transient` (`compact_qualified_pattern_facet` in
`src/runtime/eval/lower/patterns.rs`) against the facet names an error value carries.

```text
built in lib, tested in main: false
built in main, tested in main: false
built in main, tested in lib: true
built in lib, tested in lib: true
```

Handling policy, fixed before any scored call: the generated service bindings declare no
facets. Both languages are told which kinds are transient and match variants.

## D2: `Path.relative_to` returns a path outside its base unchanged

```sh
cd bench/codemode/defects/d2-relative-to
xsh main.xsh
```

Observed:

```text
inside: logs/a.log
outside: /etc/hosts
```

`xsht api method:Path.relative_to` states "The result is an error when the path is not
below the requested base", but the signature is `Path.relative_to(base: Path) -> Path`
and a path outside the base comes back unchanged, so a caller cannot tell a relative
result from an absolute one. Either the contract text or the behavior is wrong. No
development reference depends on the outside case; the tasks only relativize paths
that a walk of the base produced.

## D3: a top-level `Str +=` statement checks but fails at run time

```sh
cd bench/codemode/defects/d3-top-level-str-append
xsht check main.xsh   # passes
xsh main.xsh          # exits 3
xsh in-loop.xsh       # the same append inside a loop prints ab
```

Observed for `main.xsh` (`var text = "a"`, then `text += "b"` at top level):

```text
err[runtime.error]: += assignment cannot combine Str and Str
```

The same `+=` inside a `for` body or a `proc` works, and `Int +=` works at top level. A
model's program hit it in the revision run (`dev-config-set/xsh-revised/1`, attempt 1);
its repair avoided the append by joining a list that ends with an empty line, and passed. It was found during
scored runs, so its handling is recorded rather than pre-set: the attempt counts as a
runtime failure and is classified `compiler/runtime defect`; no reference is affected.

## D4: `sort-by(desc: true)` reverses items with equal keys

Reported by the held-out author (`tasks/session/session-oncall-report/NOTES.md`) and
reproduced on the frozen toolchain:

```sh
xsh bench/codemode/defects/d4-descending-sort/main.xsh
```

```text
desc: c d b a
asc: a b d c
```

`xsht api language:stream.sort-by` promises a stable sort, so `a b d` (all keyed 1) should
keep their order after `c` under `desc: true`; they come out reversed. The documented
two-pass idiom for a compound order is therefore wrong for a descending primary key.
The authored references sort once by a record key instead; no reference depends on it.

## D5: a user type named like a built-in record gives a misleading second error

Reported by the held-out author (`tasks/held/held-dedupe-tree/NOTES.md`) and reproduced:
`type Group = {name: Str, size: Int}` is rejected with `check.duplicate-name` (correct),
and each use is then checked against the built-in `Group` record, adding a
`check.type-mismatch` that names fields the user never wrote
(`bench/codemode/defects/d5-builtin-type-name/main.xsh`). A diagnostic-quality issue, not
a behavior defect. The author's exact wording ("expected Record, found Record") did not
reproduce; the cascade did.

## Documentation gaps the development runs exposed

Each is a fact a correct program needs that `xsht api` states wrongly or not at all.
They are not product defects; they are recorded for the follow-up list.

- `fs.walk` and `fs.files` obey `.gitignore` files by default, also outside a Git
  checkout, but the signature shows only `gitignore: Bool = default`.
- Many `xsht api` contracts say "Function arguments are positional-only; parameters
  marked `= default` ... cannot be supplied as `name = value`", while `name: value`
  arguments such as `fs.walk(root, gitignore: false, hidden: true)` and
  `archive.create(..., collision: .Replace)` check and run. Some functions even require
  the label: the held-out author found, and the frozen checker confirms, that
  `csv.encode(rows, true, ",", ["sku", "name", "price"])` is `check.named-arg` for
  `headers`, `delimiter` and `columns`.
- `check.redundant-parens` is a hard check error, not a lint, and neither the tour nor
  `xsht api language:core` says so; it caused 8 of the 24 failed development programs.

TODO.md tracks D1 as XSH-158, D2 as XSH-159, D3 as XSH-162, D4 as XSH-163, D5 as
XSH-164, and the documentation gaps as XSH-160 and XSH-161.

