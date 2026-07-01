# XSH Low-Level Optimization Plan

This note tracks the current string-overhead reduction plan for the low-level
language core. The goal is not to make source text disappear: diagnostics,
formatting, docs, and user-visible ordering still need text. The goal is that
hot evaluator, checker, and lowered-IR paths compare compact identities rather
than repeatedly allocating, hashing, or scanning UTF-8 strings.

## Direction

XSH source stays greppable and text-shaped. Internally, names should become
binary identities as soon as they cross a syntax boundary where the token is
known to be an identifier-like name.

Prefer:

- `Name` for single identifiers.
- `QualifiedName` for `namespace.member` identities.
- compact wrappers such as `FunctionName` only when a value can be either
  simple or qualified.
- shape-local indexes for repeated record-field access.

Avoid in hot paths:

- `Name::intern(format!("{namespace}.{member}"))`
- string keys for field lookup when the field already came from AST syntax
- `as_str()` comparisons except at display, diagnostics, docs, or true text
  boundaries
- generic string sorting unless the result is user-visible

## Current State

Implemented:

- `src/symbol.rs` provides interned `Name`, compact `Symbol`, and
  `QualifiedName`.
- AST, checker, sema, runtime bindings, function parameters, and many internal
  maps now use `Name`.
- Runtime direct module calls use qualified proc/pure maps keyed by
  `QualifiedName`.
- Lowered IR function calls use `LoweredFunctionKey::{Name, Qualified}`, so
  qualified lowered calls no longer require flattened dotted names.
- Runtime type validation and lowered type conversion use
  `qualified_type_defs: FxHashMap<QualifiedName, TypeDef>` for qualified type
  definitions, including module import contexts, function argument/return
  checks, rest-parameter item checks, and type patterns.
- First-class function values use compact `FunctionName` handles that can carry
  either a simple `Name` or a `QualifiedName`. Module records carry direct
  qualified handles for exported proc/pure values, and `.call(...)` dispatches
  through the matching simple or qualified registry.
- Aliased/default module imports and loaded-module records no longer register
  unnecessary flattened `module.member` compatibility names for qualified type
  definitions or first-class exported proc/pure values.
- Lowered type/tag lookup no longer flattens qualified type names or qualified
  tag probes; qualified type definitions are resolved through `QualifiedName`.
- Shaped record field lookup can use interned `Name` via `RecordMap::get_name`.
- `clippy::single_call_fn` is globally allowed in `Cargo.toml`.

Measured result after the lowered qualified-call and qualified type/function
passes:

- Allocation calls dropped across the perf scenarios versus the preceding pass.
- Interpreter benches recovered strongly in pure/result/mixed paths, with large
  wins in pure call chains and result validation.
- Frontend timing is still mixed; some lower-heavy allocation counts improved,
  but import/lowering cases still need attention.
- Targeted qualified-helper allocation measurements improved after removing
  stale flattened module registrations:
  `perf/interpreter/qualified-pure-ir-glue-5k.xsh` dropped from 65,589 to
  65,514 allocation calls and from 3,498,021 to 3,488,098 allocation bytes
  against a fresh clean `HEAD` control on 2026-06-15. The broader
  `perf/run.xsh` scenarios are not targeted at first-class qualified function
  values or qualified runtime type lookup and remain noisy.

## Remaining Work

### 1. Qualified Error Families And Variants

Error family lookup and some lowered error construction still use recursive
flattening for names like `module.ErrorFamily`.

Introduce binary identities for error families and variants:

- `QualifiedName` for family identity.
- Possibly a compact `ErrorVariantKey { family, variant }`.
- Keep display formatting only for diagnostics and serialized error values.

Use these in:

- error constructors
- lowered error expressions
- pattern/tag matching where applicable
- error-family registration during module import

### 2. Record Shape Field Indexing

`RecordShape` now stores field `Name`s, but `index_of_name` still does a linear
scan. That is fine for tiny records, but a complete version should avoid repeated
linear search on larger shapes.

Use a size-sensitive layout:

- keep the current array scan for small records
- add a shape-local `FxHashMap<Name, usize>` for larger record shapes
- consider generated index constants for standard records with fixed schemas

Do not pessimize tiny standard records with unnecessary map allocations.

### 3. Parser And AST Boundary

Many syntax fields are already interned, but some command-word and parser
surfaces still create owned strings and intern later.

Push `Name` closer to parse/desugar where safe:

- identifier tokens
- record field names
- named call arguments
- method names that are syntactically identifiers
- module/member names in qualified expressions

Keep raw strings for true text surfaces: string literals, command words, paths,
format text, diagnostics, and comments.

### 4. Static Symbol Preloading

The interner currently handles builtins plus lazy dynamic names. More hot names
can be known ahead of time.

Generate or maintain a static symbol table for:

- standard module names
- standard method names
- standard record fields
- builtin type names
- standard error family and variant names
- common constructor names

The benefits are deterministic IDs for hot builtins and less startup churn.
Generated tables should be checked in only if they stay small and auditable.

### 5. Text Boundary Audit

Audit `Name::as_str()`, `Display`, `Ord`, and `format!` usage.

Classify each use as:

- required user-visible text
- diagnostics or docs
- map key compatibility
- hot-path comparison that should become binary
- temporary compatibility shim

This is mostly cleanup, but it prevents new string work from creeping back into
the core.

## Recommended Order

1. Record shape indexing.
2. Qualified error families and variants.
3. Parser/AST boundary tightening.
4. Static symbol preloading.
5. Text boundary audit.

Qualified type definitions, first-class function values, and lowered type/tag
lookup cleanup are complete for the runtime/lowered-IR paths covered by this
plan. Error-family work remains the main intentional dotted-name compatibility
area in the evaluator.

## Verification

For each pass:

- Run `cargo check`.
- Run the relevant targeted test suite, then `cargo test`.
- Rebuild the release perf binary when collecting perf:

```sh
cargo build --release --features perf-metrics --bin xsh
target/release/xsh perf/run.xsh
```

- Compare allocations against the previous pass:

```sh
target/release/xsh perf/allocation-compare.xsh -- \
  target/perf/OLD/allocation.json \
  target/perf/NEW/allocation.json
```

- For interpreter-facing changes, run:

```sh
cargo bench --bench bench frontend -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
cargo bench --bench bench interpreter -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
```

The target signal is not just fewer allocations. A pass should either improve
interpreter timing or clearly set up the next pass that does.
