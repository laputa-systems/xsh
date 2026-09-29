# XSH: canonical membership and boolean assertion statements

The host is macOS aarch64. Linux, Docker, cross-compilation, release packaging, and CI-platform changes are out of scope. Read `AGENTS.md`, `docs/CHAPTER-01-why-xsh.md`, and the relevant contracts in `docs/SPEC.md`, `docs/SPEC-TYPING.md`, `docs/ARCHITECTURE.md`, `docs/XSHT.md`, and `docs/TEST-MAP.md`.

## Goal

Make `in` / `not in` the canonical membership spelling, remove redundant standard membership APIs, and make boolean expression statements implicit assertions:

```xsh
# Before
 test.contains(output.stderr, "attempt 3")?
 test.eq(actual, expected)?

# After
 "attempt 3" in output.stderr
 actual == expected
```

This is a breaking language/API cleanup, not an additional preferred spelling alongside indefinitely supported aliases. Do not implement unrelated syntax proposals, a second interpreter, a new test framework, or a general-purpose assertion DSL.

## 1. Specify expression statements versus values first

The rule is: **a statically typed `Bool` used as an expression statement asserts; a boolean used as a value does not.** This is statement-use classification, not an unused-variable/dataflow optimization.

- In discard/statement position, evaluate a `Bool` expression once. `true` completes with `Unit`; `false` propagates a structured assertion failure.
- A final boolean expression in a `Unit` or `Result[Unit]` function body is also an assertion. This includes ordinary annotation-free test procs, whose existing default return type is `Result[Unit]`.
- A genuine value-producing tail remains a value. In particular, `Bool`, `Result[Bool]`, `Any`, and `Result[Any]` return contexts may consume a boolean without asserting it. A boolean tail in a function requiring an incompatible value, such as `Int`, remains a type error; do not silently turn it into an assertion and hide a missing return.
- Initializers, assignments, arguments, explicit `return`, `yield`, conditions, guards, expression branches, and value-producing callback tails are value contexts. A false `where`/`any`/`all` predicate must not become an assertion failure. For inferred value-producing blocks, infer their result before classifying statement assertions. In particular, a value-producing retry tail of `false` remains `Ok(false)`.
- Boolean expression statements inside ordinary statement blocks assert, including non-tail statements inside value-producing functions. Preserve the existing narrow function-tail contract: this task does not make statement-form `if`/`match`/loops produce values.
- In ordinary script top-level statement execution, a bare boolean asserts, including the last statement. Preserve final top-level integer exit statuses. Preserve imported modules' declaration-only top-level restriction.
- `let _ = predicate()` explicitly discards the boolean without asserting. Never autofix an intentional discard or an unused boolean binding into an assertion.
- Require a checked `Bool`, including normally resolved aliases/narrowing. Do not decide assertion semantics by inspecting an `Any` value at runtime. Do not add truthiness for `Status`, integers, optionals, or results. `path.exists()?` can assert its unwrapped boolean; `path.exists()` still returns a value-producing `Result[Bool]` that needs handling. Do not add implicit `?` for these values.

These rules apply to general XSH, not just test filenames or `proc test_*`. Preserve `xshi`'s shell-subset frontend contract. Its XSH execution must use the same core rules; any existing explicit expression-value consumer remains a value consumer, not a reason to introduce an alternate evaluator.

Use one authoritative statement-use classification shared through checker facts/lowering and tooling. Cover semicolon/newline forms consistently; do not introduce an undocumented Rust-like semicolon distinction.

## 2. Integrate failure with XSH's existing model

Introduce a core `AssertionError` with a `Failed(message: Str)` variant through the existing nominal `Error`/`Result` machinery. It must be available without the `native-tests` feature. This is not a new exception subsystem or a Rust panic.

An assertion statement behaves like propagation of a pure `Result[Unit, AssertionError]`: use the existing pure-function, result, and error-family legality checks. Do not silently widen a declared error type. In restricted procs, assertion propagation outside a retry attempt requires `error`; update effect checking and inference accordingly. Unrestricted procs remain unrestricted. Inside retry attempts, follow the existing attempt-local propagation rules.

Failures must work through ordinary callers, result handlers, retries, and existing unwinding. Defers run in the established order, and cleanup failures must not hide the primary assertion failure. Use the existing CLI runtime-failure status, source diagnostics, tracebacks, and native-test failure reporting. Do not implement direct process exit as the assertion mechanism.

Use the same core failure construction for retained boolean assertion helpers, preserving their public result-valued and custom-message capabilities. Treat standardizing assertion failure identity as an explicit part of this migration and update directly affected fixtures. Keep skips and harness/setup failures distinct.

Assertions remain enabled in all build profiles. A passing assertion produces no output and requires no test context. Do not strip assertions, duplicate evaluation, or discard side effects during optimization.

## 3. Consolidate membership, without changing unrelated semantics

`in` and `not in` already exist. Preserve their precedence, source-order operand evaluation, and current domains:

- `Str in Str`: literal substring containment.
- `Bytes in Bytes`: contiguous byte-subsequence containment, including the existing empty-needle behavior.
- Element in `List`: existing value equality and typing rules.
- Existing `Path` membership and `env.PATH` membership retain their distinct current semantics. Path membership currently uses display-text containment; it must not be reinterpreted as filesystem ancestry or a security boundary. `env.PATH` retains exact entry membership and existing effects.

Extend membership to `Str` keys in `Map[T]` and `Str` field names in `Record`. Test key/field presence, not contained values. A present key whose value is null is still present. Use direct lookup, not allocation of a key list. Preserve evaluation even when a record's known shape permits an optimized lookup.

Remove the callable/discoverable standard surfaces `Str.contains`, `Bytes.contains`, `List.contains`, `Map.has`, `Record.has`, `test.contains`, and `test.not_contains`. Inventory and remove any additional actual standard module aliases that are exact duplicates of these operations. Do not guess APIs from their names.

Keep distinct operations such as prefix/suffix tests, regex matching, filesystem existence, lookup/get, and stream predicates. Do not add regex matching or implicit consumption of streams to `in`. Do not introduce numeric-byte membership or implicit text/byte conversion.

The scope is XSH's public APIs and XSH call sites. Rust's own `.contains` calls and useful private membership primitives are not removal targets. Arbitrary user-defined fields/functions named `contains` or `has` must not be rewritten.

## 4. Implement through the current architecture

Inspect the actual owners before editing. Relevant starting points are:

- `src/syntax/` and `src/sema/check/{expr,stmt,call,method,stream}.rs`.
- `src/runtime/eval/lower.rs`, `indexed/full.rs`, `lowered_ops.rs`, and `lowered_run/indexed_run.rs` with its `explicit_run.rs` child.
- `src/runtime/eval/modules.rs`, which currently owns test assertion support.
- `crates/xsh-registry/src/signature/{methods,modules,docs}.rs`, plus `errors.rs`, `runtime_op.rs`, `reference.rs`, and associated registry metadata/tests.
- `crates/xsht/src/{lint,format,edit}.rs`, `cli/lint.rs`, and the existing tooling integration suites.

Carry assertion classification into verified indexed execution; do not rediscover it using dynamic value tags or test-runner-only source rewriting. Update verifier invariants and every affected execution path. Retain the existing efficient string/byte/view/list membership implementations; ensure the canonical operator reaches appropriate fast paths rather than replacing specialized membership with avoidable materialization.

Diagnostics are part of the feature. Report the original expression, file, line/column, and useful evaluated values. For equality/inequality, ordering, and membership assertions, retain enough information to explain what failed rather than merely printing `false`. Preserve useful equality-diff behavior where applicable. For compound predicates, show only evaluated information; never evaluate a skipped operand to produce diagnostics.

Use source spans and compact diagnostic metadata prepared during checking/lowering. Do not parse source expressions at execution time. Render details lazily on failure, using existing bounded rendering conventions; do not clone whole strings/collections or allocate formatted diagnostics on every successful assertion.

## 5. Add safe, source-preserving autofixes

Add stable lint rules such as `lint.prefer-in` and `lint.prefer-bare-assertion`, following the repository's existing naming/diagnostic conventions. Use checked types and resolved standard-call identity with CST-backed edits, not regex rewriting.

Support these canonical transformations where semantics permit:

```xsh
container.contains(item)             # -> item in container
! container.contains(item)           # -> item not in container
mapping.has(key)                    # -> key in mapping
record.has(field)                   # -> field in record

test.ok(condition)?                 # -> condition
test.eq(actual, expected)?          # -> actual == expected
test.ne(actual, unexpected)?        # -> actual != unexpected
test.contains(container, item)?     # -> item in container
test.not_contains(container, item)? # -> item not in container
```

Also handle currently valid statement forms without an explicit trailing `?`. Assertion-helper rewrites to bare expressions are only valid when the replacement is classified as an assertion statement. Keep `test.ok`, `test.eq`, and `test.ne` available for result-valued/custom-diagnostic/dynamic cases; ordinary, safely expressible assertions should no longer need them.

For removed containment helpers with a custom message or whose `Result` is consumed, migrate to an explicit `test.ok(item in container, message: ...)` or negated counterpart when type and evaluation semantics permit. Do not replace a result-valued call with a statement, erase a meaningful message, drop evaluation of a message expression, or lose `.context`, fallback, return, or match behavior.

Important safety constraints:

- Replacing a method call with `in` reverses the textual operand order. Preserve actual evaluation order, evaluation count, failures, mutable reads, and short-circuiting. Purity alone is not proof that reordering is safe. Use a conservative proof for direct fixes. Where a whole-statement rewrite can safely preserve order, introduce hygienic local temporaries at the original evaluation point. Otherwise emit an actionable diagnostic without an unsafe fix; migrate the repository occurrence manually with clear bindings.
- Handle named/reordered arguments, precedence, parentheses, negation, multiline syntax, comments, Unicode spans, and shared imported sources. Do not hoist evaluation out of branches, callback bodies, retries, or short-circuit operands.
- The old test containment helper accepts broad `Any` inputs and returns false for unsupported operand pairs. Do not claim that replacing those cases with a type-rejecting operator is semantics-preserving. Autofix only proved-supported domains; diagnose/manual-migrate intentional legacy-invalid cases.
- Equality helpers likewise require a proof that operator equality has the applicable typing and comparison semantics. Do not broaden core equality merely to obtain more fixes.
- Do not convert general validation branches using `error.fail`, domain-specific errors, or intentional discards into assertions.
- Nested fixes must compose correctly, preserve comments, and reach an idempotent result without conflicting edits or oscillation with existing lints.

Crucially, migration fixes must work after the old APIs have been removed. Do not leave callable compatibility aliases solely so the linter can recognize them. Use narrowly scoped removed-API diagnostics/migration metadata and receiver/argument facts so `xsht lint --fix` can offer/apply justified edits despite those specific old-API errors. Do not suppress unrelated checker failures. Recheck rewritten source through the existing checked-program pipeline before accepting edits. Keep migration-only names out of runtime dispatch, completion, and `xsht api` inventories.

## 6. Migrate the maintained corpus and reference surface

Migrate applicable XSH in `stdlib/`, `core/`, `dev/`, tests, examples, showcase programs, maintained documentation snippets, and relevant embedded XSH fixtures. Scope showcase edits to this language/API migration. Preserve unrelated work and formatting.

Audit existing discarded boolean statements too: code that intentionally ignored a boolean must become an explicit discard rather than accidentally acquiring a new assertion. Do not alter genuine boolean tails or predicates.

Update `docs/SPEC.md` first or alongside implementation, including statements/tails, propagation/effects, membership, testing, and the operator exception to the method-preference policy. Update the closest typing/tooling/architecture contracts and canonical registry language/API examples. Delete stale registry metadata and operation variants only when unused; retain shared primitives needed by `in`. Do not rebuild generated documentation or introduce another authoritative API list.

The final executable corpus must not depend on removed standard APIs. Legacy spellings may remain intentionally in migration/negative fixtures, not as runnable compatibility paths.

## 7. Verify on native macOS aarch64

Prefer native XSH integration coverage for language behavior, with Rust tests at actual CLI/tooling/verifier boundaries. Include an independent negative CLI witness using ordinary Rust assertions: a script containing a failing bare boolean must exit unsuccessfully, report the failing source, and not execute a subsequent marker. Do not validate the new assertion mechanism solely using that same mechanism.

Cover passing/failing comparisons and arbitrary boolean expressions; function and callback value tails; Unit/ResultUnit tails; explicit discards; Any/Status/ResultBool boundaries; typed errors and effects; retry/handler behavior; defer order and cleanup-failure precedence; short-circuiting and single evaluation; every membership domain; map/record present-null fields; string/byte views and Unicode; and feature-disabled core assertions.

Exercise removed-API migration after removal, custom messages, consumed results, operand-order hazards, named arguments, ambiguous/dynamic receivers, comments, precedence, nested fixes, idempotence, and imported-file deduplication. Assert removed names are absent from API/completion inventories. Use deterministic temporary resources, no external network services, and no destructive actions outside test-owned directories.

Confirm the native host/toolchain and follow the pinned Rust toolchain. Build exact debug binaries as needed:

```sh
cargo build -p xsh --bin xsh
cargo build -p xsht --bin xsht
cargo build -p xshi --bin xshi
cargo test --test integration syntax::
cargo test --test integration sema::
cargo test -p xsht --test integration lint::
cargo test --test integration runtime:: -- \
  --skip runtime::coverage:: \
  --skip runtime::examples::example_corpus_is_formatted \
  --skip runtime::examples::example_corpus_lints_without_warnings \
  --test-threads=1
target/debug/xsht test --jobs 1 tests/xsh/stdlib
```

Run the closest new/changed native test files and relevant registry/API/verifier/CLI suites as well. Verify a supported `xsh` configuration without `native-tests` can execute core assertions. Adjust commands to current repository ownership, and report any unavailable/platform-specific cases rather than claiming they ran.

Respect the repository's no-global-formatter/autofixer policy. For this feature, focused formatting/autofix acceptance tests in isolated temporary fixture trees are explicitly in scope; project-wide formatter/lint/autofix runs are not. Migrate the working corpus with reviewed, scoped patches. Do not use broad development workflows that launch release builds, Linux, Docker, or unrelated rewriting.

## Completion

Finish implementation, corpus migration, documentation, and native verification—not just a plan or partially wired syntax. Report the exact removed and retained surfaces, the resolved assertion/value/error contract, any deliberately non-autofixable cases, and the commands/results actually verified. Identify blockers or pre-existing failures precisely. Do not claim Linux verification.
