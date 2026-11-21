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

Handling policy, fixed before any scored call: the generated service bindings declare no
facets. Both languages are told which kinds are transient and match variants.
