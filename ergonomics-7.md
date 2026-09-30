## 1. Infer the target of an explicit schema-validation operation

```xsh
# Before
proc read_manifest(path: Path) [fs, error] -> Result[Manifest] {
  json.read(path)?.require(Manifest)
}

# After
proc read_manifest(path: Path) [fs, error] -> Result[Manifest] {
  json.read(path)?.require()
}
```

Permit omission of the schema argument in the existing special `.require(Type)` operation when an independently established expected type uniquely determines it. The validation call remains explicit and still executes the same runtime schema check or schema-directed conversion, returning the same Result shape. An annotation alone must never perform validation.

Propagate expected types through typed bindings, explicit returns, function tails, branch/value blocks, known parameter slots, Result construction, and explicit `?` so a target is not lost merely because a value passes through an ordinary expression. Reuse shared bidirectional expression checking rather than a collection of textual recognizers for the example.

Infer only the schema parameter, not a new error family or propagation destination. For example, an expected `Manifest` after `.require()?` determines Manifest, and an expected `Result[Manifest, Error]` for `.require()` determines the same schema. A declared narrower error type still must accept the operation's actual errors. Preserve nested Result distinctions and assertion/value contexts.

The target must be grounded in an explicit annotation or uniquely resolved callable/return contract. Do not invent a schema from subsequent field access, fallbacks, runtime values, arbitrary mutable writes, or an overload whose only selecting evidence would itself be the omitted target. Ambiguous or unconstrained calls require the existing explicit `.require(Type)` form. Do not infer Any or an erased empty Record as a successful shortcut.

Preserve the explicit form. Known typed boundaries can use either spelling; no runtime schema lookup, reparsing, or double validation is added. Expected typing must not change implicit Ok rules, error capture, host effects, or expression evaluation.

Autofix redundant explicit schema arguments only after checking that the independently expected target is exactly equivalent, including resolved generic substitutions and schema conversion behavior. Do not delete validation, its Result handling, or a schema annotation that is the only source of the target. Add underconstrained/no-fix and ambiguous-overload witnesses, not just the easy return case.

## 2. Solve local empty and nullable initializers from consistent constraints

```xsh
# Before
var objects: List[Path] = []
for source in sources {
  objects += [object_path(source)]
}

# After
var objects = []
for source in sources {
  objects += [object_path(source)]
}
```

Introduce proper local inference variables for unannotated empty collections and mutable null-seeded accumulators. Solve them from checked element writes, assignments, and unambiguous parameter/return expectations in the enclosing function. A nullable accumulator initialized with null and assigned a Path can have one fixed inferred type `Path?`; it is not a variable whose declared type changes during execution.

Infer across all relevant branches and loop bodies before finalizing the binding. A zero-iteration loop still contributes static constraints. Different branch traversal order must not select different types. All writes and reads must fit the solved type; incompatible concrete contributions are errors, not unions or Any. Existing permitted contextual conversions remain explicit checker decisions, not permission to invent numeric widening.

Use existing collection constraints: `[]` establishes List with an unresolved element; an explicit empty-map constructor establishes Map with unresolved parameters, including K/V where applicable. **Do not reinterpret an unannotated `{}` record literal as a map because of later use.** An ordinary immutable `let value = null` remains a Null value unless existing explicit contextual rules apply; this feature is not global optional-type guessing.

A type hole may flow through local immutable aliases only as the same monomorphic unknown. It must be solved before publishing a concrete signature, preparing indexed execution, or crossing an operation that needs it. Do not instantiate one empty variable differently at separate uses. A function's private result inference may consume the solved local type, but do not infer exported or required parameter signatures from callers or introduce whole-program inference.

Require an annotation when a material type remains unconstrained or overloaded uses admit multiple valid solutions. Preserve discard-binding behavior; do not create artificial type requirements for a wholly discarded inert literal. Separate inference diagnostics from recovery after an earlier error.

No runtime values are inspected and no code is executed to infer a type. Ownership, effects, stream consumption, Result handling, and value semantics are unchanged. Provide diagnostics showing the initializer and the conflicting contributions. Autofix a collection/nullable-local annotation only when rechecking the whole affected binding establishes the same type and preserves annotation-driven conversions.

## 3. Infer defaulted-parameter types semantically, not in the parser

```xsh
const defaults = {jobs: 4, timeout: 30s}

# Before
proc build(jobs: Int = defaults.jobs, timeout: Duration = defaults.timeout) {
  run_build(jobs:, timeout:)?
}

# After
proc build(jobs = defaults.jobs, timeout = defaults.timeout) {
  run_build(jobs:, timeout:)?
}
```

A defaulted parameter without a type annotation takes the checked type of its default expression, not merely a type recognized from a short list of literal AST tags. Resolve constants, imported constants, field projections, checked primitive expressions, and already permitted calls through ordinary semantic checking.

Remove literal-only type synthesis from parameter parsing. The parser records that the type was omitted; declaration/signature analysis establishes its type. Do not introduce another user-visible inferred-type token or expose recovery types in metadata. Retain the normal annotations/default requirements for non-defaulted and rest parameters.

This is type inference, not constant evaluation. Preserve which default expressions are legal, where names resolve, when supplied/omitted defaults evaluate, and how often their effects occur. Do not start evaluating defaults at declaration time or permit new parameter-to-parameter references just because the type is knowable. Preparation-only defaults in a CLI declaration retain that stricter value contract.

A type must be established from the default and already checked declaration dependencies, not from callers or the function body. Null or an otherwise unconstrained empty collection still needs an annotation. An annotation supplying a named-schema constraint, conversion, optional domain, or overload context is not redundant merely because the example call works without it.

Resolve signature dependencies deterministically and diagnose cycles that need an explicit anchor. Do not guess a type to break recursion. Preserve explicit exported return/effect contracts. The concrete inferred parameter type must reach overload resolution, callable aliases, named spreads, API output, and lowering just like an explicit type.

Autofix only exactly redundant defaulted-parameter annotations and preserve comments. Test imported constants, defaults with ordinary runtime evaluation, null/empty ambiguity, dependency cycles, and identical call behavior before/after removal.

## 4. Infer parametric record-constructor arguments from fields and context

```xsh
type Observation[T] = {state: ObservationState, value: T?}

# Before
let measured: Observation[Int] = {state: Observed, value: 12}

# After
let measured = Observation(state: Observed, value: 12)
```

Permit use of a parametric record schema's existing constructor name without expression-level type arguments. Infer its parameters from supplied named field values and a concrete expected instance, then instantiate the existing constructor/schema plan. Do not add generic-call syntax, generic functions, traits, implicit conversions, or runtime type objects.

Use one fresh monomorphic set of type variables per constructor occurrence. Named arguments, punning, statically checked record spreads, defaults, duplicate-field checks, and source-order evaluation retain ordinary constructor semantics. Instantiate field expectations before checking nested literals where context is available. Preserve K/V and nested Optional/List/Map/schema parameters without introducing Any when values disagree.

Nullable fields make a non-null supplied value evidence for the inner parameter. Null and empty default values do not independently select that inner type. For example, `Observation(state: Absent, value: null)` requires an expected `Observation[T]` or a concrete schema alias. Give an actionable diagnostic pointing to the unresolved parameter rather than guessing Any, Null, or a recovery type.

Multiple fields constraining the same parameter must agree under the established type relation. Constructor inference may not choose a broader unrequested type to erase a conflict. Explicit constraints and concrete aliases remain authoritative. Do not use future field accesses or runtime values as evidence.

Reuse the existing parametric-schema substitution cache and record-construction representation. No per-call runtime generic dispatch or extra allocation is warranted. Check defaults against the resolved instance under their existing declaration rules; do not invent parameter-dependent default execution.

Preserve construction versus validation: inferring Observation[Int] does not validate arbitrary Any input or apply defaults to JSON/schema validation. Keep untyped external values behind `.require` or existing type-pattern boundaries.

Autofix a schema-annotated record literal or concrete-alias construction only when the inferred instance, field/default evaluation, named constraints, and resulting type are unchanged. Null-only, empty-only, conversion-dependent, and conflicting-field cases must retain their type context.

## 5. Preserve field types through constant-key access

```xsh
# config has a checked schema containing workers: Int.

# Before
let workers = config.get("workers")?.require(Int)?

# After
let workers = config.get("workers")?

const field = "workers"
let same_workers = config[field]
```

For known record fields and module exports selected by a literal or compile-time-known Str key, retain the selected field/export type. A fallible `.get(key)` returns `Result[FieldType, ExistingErrorType]`; ordinary indexing has the same field type under its existing indexing/error contract. It must not become Any merely because access used a key instead of dot syntax.

Resolve keys through the shared constant facts, not arbitrary execution or a string singleton-type language. Dynamic keys and keys with no proven visible field retain their genuinely dynamic result and existing validation requirements. A width-compatible record type does not prove hidden runtime fields absent or grant access to their types.

For optional module exports, keep the actual missing-export Result behavior while preserving the known success type. Preserve typed callable signatures for known callable exports without exposing private exports or turning dynamic modules into statically trusted namespaces. A present nullable field remains `Ok(null)` when retrieved through get; null and absence are not merged.

Do not remove `.get`'s Result just because the field is usually present. Opaque/host-backed records may still report existing access/metadata errors. Keep receiver/key evaluation order and count, source attribution, field privacy, runtime-only identity checks, and explicit schema-validation boundaries.

Store the resolved projection in checked facts/lowering and reuse ordinary field access where valid; do not inspect record layouts afresh in every consumer. This feature is precision preservation, not automatic reflection or a new operator.

Autofix a now-redundant `.require(Type)` only when the successful get/index value was already guaranteed to have exactly that validated shape, the validation is an identity rather than schema-directed conversion, and all Result/error behavior and evaluations are preserved. Keep checks following genuinely dynamic access. Test keyword labels, imported const keys, nullable values, optional exports, opaque-record failures, and user fields/functions with the same spelling.

## 6. Let constant descriptors retain the same inference as inline descriptors

```xsh
const option_schema = {
  jobs: {kind: "Int", form: "-j --jobs N", default: 4, positive: true},
  root: {kind: "Path", form: "ROOT"},
}

let options = cli.parse(args, option_schema)?
# options.jobs is Int; options.root is Path.
```

Eliminate the inference cliff where a supported static descriptor works inline but loses its result shape after extraction to a const. Resolve descriptor values from the shared checked constant representation, including imported consts, constant field projections, and constant record composition already admitted by the language.

Apply this to existing descriptor-driven CLI surfaces with a defined shape relation, notably cli.parse, cli.applet, and cli.parse_full. The latter's values field must retain the same concrete schema; provenance/warning fields retain their actual contracts. Preserve the existing signature-derived CLI entrypoint path; this work supports advanced descriptor-based interfaces rather than replacing them.

A descriptor's Str field type alone is not proof of its contents. Inspect only established constant values and preserve their declaration-source provenance for diagnostics. Do not execute arbitrary functions, read files/env, evaluate mutable lets, introduce schema objects, or expand the const expression subset merely to infer a descriptor.

Derive argument shape/validation metadata and inferred result types from one normalized descriptor plan shared with the runtime parser, rather than separate partial parsers in the checker and CLI implementation. The plan must preserve required/optional/repeated rules, explicit required:false overrides, defaults, flags, aliases, type names, errors/help behavior, provenance, and existing applet-versus-strict duplicate policy.

Literal and equivalent const descriptors must have identical inferred types and parsing behavior. Unsupported genuinely dynamic descriptors remain dynamic; never pretend they produce a known schema. Invalid known descriptors should receive precise preparation diagnostics; classify any new earlier rejection as an intentional checking change, not a behavior-preserving autofix.

Keep descriptor strings that are part of the retained CLI configuration format. This is not a general runtime type-string feature. Do not allocate or parse a second descriptor plan solely for inference when the same checked data can be reused.

No obligatory hoisting lint is needed. Where existing code added `.require` or duplicate shape declarations solely to repair this lost inference, remove them only after proving exact shape and validation equivalence. Test literal/const/imported-const parity, record spreads, all option-shape rules, dynamic fallbacks, and invalid-descriptor locations.

## 7. One builtin signature-instantiation engine instead of per-method type repairs

Represent polymorphic builtin contracts directly, for example:

```text
List[T].get(index: Int) -> Result[T]
Map[K, V].values() -> List[V]
Map[K, V].set(key: K, value: V) -> Map[K, V]
List[Str].join(separator: Str = "") -> Str
```

These describe existing operations; they are not new public methods. Replace erased placeholder types plus checker switches on method names with canonical internal type templates and shared argument/result instantiation. Keep real dynamic Any distinct from a template parameter.

Use a small fixed vocabulary covering existing relationships: receiver type, element/key/value types, nested containers/Results, concrete constraints, and existing display/JSON/record projections. Reuse the actual registry and call binder. No user-facing generic functions, trait resolution, user code in signature rules, or general signature DSL interpreter.

Each call gets its own fresh template instantiation. Solve from receiver, arguments, and independently available expected result under existing overload policy. Do not disambiguate overloads by return type alone or weaken concrete operands. Integrate local holes and parametric record substitution through shared constraint primitives, not one universal unbounded inference solver.

Ordinary calls, methods, named arguments/spreads, value-pipeline lowering, and statically typed aliases must consume the same signature facts. Preserve parameter/default evaluation order, effect contracts, nominal error types, and type-dependent runtime checks. Keep genuinely semantic special rules for schema validation and constant descriptor/projection facts identified explicitly in registry metadata rather than scattered method-name string checks.

Render API metadata and diagnostics from the same templates, showing meaningful T/K/V relationships instead of internal placeholders. Lower to the same efficient concrete runtime operations; do not replace native dispatch with runtime generic lookup. Do not grow the Rust core by retaining the old repaired signatures alongside a second canonical engine.

Before deleting a special case, pin its intended behavior in tests. Remove accidental unsound widening deliberately under the single-checker contract rather than silently preserving it as generic inference. Require tests that the same operation has identical types/diagnostics through supported call spellings. Report the removed special cases, not merely the new abstraction.

## 8. Retire record.require's parallel string-based type language

```xsh
# Before
let checked = record.require(raw, {name: "Str", jobs: "Int"})?

# After
type BuildConfig = {name: Str, jobs: Int}
let checked = raw.require(BuildConfig)?
```

Remove the public legacy record.require API and its parallel string-contract grammar in favor of ordinary named schemas and the explicit `.require(Type)` boundary. Where an independent expected type exists, section 1 also applies. Remove the record module itself only if it has no other retained public operations.

Migrate maintained static contracts into ordinary schemas, preserving required fields, nested validation, type identity, source/diagnostic context, and extra-field policy as required by the caller. Reuse schema aliases and parametric records when genuinely appropriate; do not generate one redundant declaration for every temporary.

This is a deliberate API removal, not a promise that every old contract is interchangeable with a named schema. In particular, an absent optional key is not automatically the same as a present T? field; string-described callable contracts are not arbitrary record schemas; runtime-selected validation policy is not compile-time type inference. Preserve these semantics with explicit application-owned validation/type-pattern branches or applicable existing module contracts. Do not silently insert null, defaults, casts, or unsupported callable promises.

For truly dynamic schema-processing programs, keep the data dynamic and the validator explicit. Do not build a new runtime type-object/eval/schema framework to recreate the removed DSL. Document intentional unsupported old surface precisely, and do not claim an unsafe legacy use was automatically migrated.

Remove runtime string parsing, checker string-grammar validation, signature/docs entries, and glue used only by the removed API. Audit shared uses before deleting code; retained CLI scalar descriptors are a separate limited configuration format and remain valid. Remove no code merely because its identifier contains the word contract.

Provide narrow removed-API diagnostics and safe migration edits for proved-equivalent literal/constant required-only contracts even after runtime support is gone. Do not suppress unrelated errors or preserve executable compatibility shims. Use ordinary explicit error translation/context where a caller depends on a distinct validation error contract.

## 9. Make checked dynamic boundaries the default, not an opt-in strict mode

```xsh
# Explicit validation stays visible; the target need not be repeated.
let config: BuildConfig = json.read(path)?.require()?

# This must not establish BuildConfig just from the annotation.
let unchecked: BuildConfig = json.read(path)?
```

Consolidate compatibility and strict-dynamic checking into one language checking contract. Ordinary `xsht check` and execution preparation enforce the same type/validation rules. Remove --strict as a semantic mode; update the caller/config/test/documentation surfaces that selected it. Prefer an actionable removed-option diagnostic over an indefinitely accepted no-op or a new --unsafe/--loose replacement.

Permit concrete values to be erased into Any, and allow intentional dynamic operations to keep dynamic results. Reject a flow from Any, erased Record, a container containing unchecked Any, or an erased callable into a concrete type unless the existing explicit schema/type-pattern/contract boundary has established it. An expected type is a constraint, not proof about external data. Preserve runtime checks for genuinely dynamic operations and host boundaries.

Keep exact empty record literals distinct internally from an erased dynamic Record; neither recovery types nor that representation collision may accidentally certify fields. Preserve current width compatibility for genuinely known records, invariant container contracts, nominal tags/errors, and approved schema-directed conversion at explicit `.require` boundaries.

Remove strictness toggles and duplicated conditional checks after migrating the maintained code. Unsupported field access on a known shape is an ordinary error; dynamic `.get` stays available with an honestly dynamic result. Do not globally ban Any, require schemas for throwaway output records, or claim that these static rules eliminate arithmetic/OS/runtime failures.

Implement the inference improvements first so this change does not merely spread annotations. Preserve explicit error/effect contracts and the established distinction between errors and lint/style guidance. Recovery should suppress cascades without making invalid code executable.

A migration tool may remove --strict from maintained invocations where the default now provides that policy. It must not insert `.require()?` everywhere to force compilation: validate at genuine input boundaries, use sound existing proofs, or keep intentional dynamic computation dynamic. Review contracts added at a boundary rather than guessing a schema from field names used later.

Test unchecked nested containers, null/optional flows, dynamic modules/callables, host-returned data, explicit validation, type patterns, correct erasure into Any, and equality/serialization of deliberately dynamic values. Compare check and execution preparation results; do not retain a hidden permissive runner.

## 10. Delete only demonstrably redundant compatibility vocabulary

Use a finite audited removal list, with these intended canonical spellings:

```text
ARGV                            -> args
run.builtin*                    -> corresponding run* form
fs.ls(...)                      -> fs.children(...)
Str.count_bytes()               -> Str.byte_len()
```

The run migration preserves the rest of the actual form, including status/text/bytes/capture/stream modes, options, argv, and Result/status behavior. ARGV migration targets only the predeclared compatibility binding, not user locals, quoted strings, environment names, external argv, or serialized fields.

Use fs.children as the single ambient direct-child enumeration name only after confirming exact alias semantics in the actual checkout. Do not conflate it with rooted handle operations, fs.walk, or fs.files; their contracts differ. Str byte_len remains explicitly byte-oriented and must not replace character counts or acquire a generic Unicode-ambiguous len spelling.

Verify each pair's signatures, defaults, parameter order, effects, failures, iteration/materialization order, feature gates, and values. A preferred name is not sufficient evidence of equivalence. If a listed pair differs in the current tree, retain the genuinely distinct operation and document the specific difference instead of changing behavior to make removal appear safe. Do not enlarge this into an unbounded vocabulary redesign.

Remove public aliases from registries, runtime dispatch, generated help/API inventories, completion, and maintained examples. Retain migration metadata/diagnostics outside executable dispatch and preserve source attribution to the canonical operation. Historical alias operation names in diagnostics/traces may deliberately become canonical; record that change rather than claiming exact text equivalence.

Use CST-backed, symbol-resolved fixes even when old names no longer typecheck. Keep ordinary user methods with the same spelling untouched. Delete private operation aliases only when no retained path uses them; do not search-and-replace Rust names or remove shared primitives.

Test public inventory absence, supported canonical behavior, recovery/fix idempotence, shadowed names, comments, external command arguments, and real direct-child ordering/error cases with local fixtures. This is a vocabulary cleanup, not a process-execution or filesystem-policy redesign.

## Shared architecture and quality requirements

Inspect current owners under `src/syntax/`, `src/sema/check/`, `src/sema/types.rs`, `src/loader.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, and `crates/xsht/`. The reviewed baseline exposes concrete targets: parser-level default inference; binding types finalized from initializers; record get returning Any; CLI inference restricted to literal Record AST nodes; separate list/map method checking; record-contract string parsing; and compatibility/strict checking branches. Do not assume all remain untouched in the current worktree.

Use a small shared substrate: fresh inference variables distinct from dynamic/recovery types, expected-type flow, canonical substitutions/signature templates, and checked constant facts with source provenance. Sections 1-6 should use that substrate, not six independent solvers. Reuse source spans and resolved symbols through lowering; do not execute user code to infer types or reparse expressions at runtime.

Avoid premature abstraction. Restrict constraint solving to the stated local/declaration scopes and supported type relationships. Preserve runtime semantics unless a section explicitly authorizes a breaking checking/API change. No new implicit collections, dynamic casts, user-defined operators, generic function system, broad numeric promotion, or reflection facility.

Every inferred type must have an explainable origin. On conflict, report the smallest useful set of constraints: the ambiguous seed/declaration, the operation or field that establishes a type, and the incompatible use. Show user-facing schemas/types, not solver IDs or Unknown. Keep messages bounded and avoid a cascade for each later use. The existing reveal_type/check-annotation/API tooling must expose final inferred types consistently; add no debug helper to the runtime language.

For every autofix, preserve comments, Unicode byte spans, named-argument/source order, validation and conversion boundaries, Result handling, effects, cleanup, and shared-source deduplication. Recheck through the normal pipeline. A second pass must be unchanged. Do not remove an annotation merely because current sample executions happen to succeed; compare solved types and required conversions. Keep human-readable explicit annotations where they state a meaningful boundary.

Migrate maintained stdlib/core/dev code, native tests, relevant examples/snippets and scoped showcase cases with reviewed edits. Remove old special cases and entrypoints after tests prove the replacement. Update canonical specs, registry reference metadata, nearest architecture/testing documentation, and affected development commands. Do not regenerate published documentation, create another authoritative language manual, or hide problems by excluding files from checking.

