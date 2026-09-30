# XSH: the final coherence pass

The objective is fewer repeated proofs, wrappers, serialization adapters, and API special cases. Add no standalone keywords, symbolic operators, general callback/trait system, user-defined hashing, scheduler, dynamic eval, or general-purpose compile-time interpreter. Extend existing concepts rather than rebuilding preceding work. Examples name illustrative user types/helpers; they are not new standard APIs.

## 1. Preserve proofs through record projections and immutable Boolean aliases

```xsh
let available = report.firmware.vendor != null
guard available else { return }
print $report.firmware.vendor
```

Extend existing flow-sensitive narrowing from simple bindings to statically known record-field paths, and retain proof provenance through immutable Boolean aliases. In this example assume `report.firmware` itself is a nonoptional known record. The final field has type Str after the guard, not Str?, without another null check, fallback, or schema conversion.

Recognize existing null, type/pattern, and field-presence tests and their supported `!`, `and`, and `or` combinations; do not add new predicates or stronger schema assumptions than those tests establish. An immutable alias such as `let available = expr` may retain a bounded reference to these facts. Analyze alias chains with shared identities, not exponentially expanded expressions. Evaluate no expression during analysis and emit no additional runtime checks.

Facts refer to binding identity/version and field paths, not repeated text. Invalidate a fact when its subject or any path prefix can change, even if the Boolean alias itself is still true. Follow value semantics: a copied immutable record is a snapshot, not a live alias to a reassigned variable. Retain facts across unrelated sibling updates only when disjointness is proved. Unknown calls must conservatively invalidate facts about mutable state they could affect; effects alone are not a mutation/alias proof.

Carry facts through established branch/guard/normal-assertion continuations. At joins retain only facts true on every reaching path. A caught failed assertion must not establish its success facts after a recovery join. Do not infer filesystem existence, resource liveness, numeric ranges, array bounds, or arbitrary relationships among fields. Scope this extension to names and known record paths, not effectful getters or general dynamic indexing. Missing-field membership establishes only the field/type information that the existing membership rule permits.

Autofix redundant `??` fallbacks or repeated null-check scaffolding only where the checked facts prove them unreachable/redundant and evaluation/conversion behavior is unchanged. Preserve useful names and messages. Add paired positive/negative tests for alias invalidation, parent replacement, shadowing, short-circuiting, early exits, recovery joins, and mutable captures. The win is checker precision, not hidden casts.

## 2. Infer effects of private procs rather than requiring handwritten forwarding summaries

```xsh
# Before
proc read_manifest(file: Path) [fs, error] -> Result[Manifest] {
  json.read(file)?.require(Manifest)?
}

# After: still checked as requiring fs and error.
proc read_manifest(file: Path) -> Result[Manifest] {
  json.read(file)?.require(Manifest)?
}
```

For ordinary non-exported user procs without an effect clause, infer an effective effect set from checked bodies and resolved callees. This is semantic inference, not a linter that re-inserts annotations and not permission to treat the proc as pure. Keep explicit effect clauses as checked upper bounds, including explicit `[]`. Pure/proc separation and return typing remain unchanged.

Preserve existing behavior at exported API boundaries, dynamically loaded module contracts, CLI/test entry declarations, and stream declarations; do not silently infer a public contract from its current implementation. An exported alias in feature 9 must preserve an explicitly established public callable contract, rather than accidentally making a private inferred implementation the only API promise.

Use the finite existing effect domain. Compute transitive summaries over the resolved call graph with a least fixed point for recursive components, independent of source declaration order. Reuse checker/linter effect knowledge instead of maintaining rival analyses. Include method effects, executed stage bodies, implicit propagation/assertions, and special host forms. Distinguish executing a function from merely referencing it. Local try/retry capture affects outward propagation exactly as its contract prescribes; host effects are not erased by capture.

An opaque callable, unrestricted external callee, or unresolved dependency is unknown/unrestricted, not an empty set or an invented all-inclusive `io` set. Such a dependency cannot satisfy a restricted caller unless an existing checked contract justifies it. Emit an actionable call-chain diagnostic. Do not add effect-polymorphic parameters, effect aliases, or annotation overrides that permit an unchecked body.

Expose effective summaries consistently to call checking, private signature displays, linting, lowering, and static aliases. Avoid an annotation lint oscillating with inference. Offer removal only for redundant private annotations whose absence rechecks equivalently; keep deliberate documentation/upper bounds when useful. Test transitive calls, recursion, captured mutation versus host effects, explicit violations, unknown dynamic calls, and restrictions through aliases. No runtime scheduling or effect checks are introduced.

## 3. Parametric record schemas, not a general generics framework

```xsh
type Observation[T] = {
  state: ObservationState,
  value: T?,
}
type NameObservation = Observation[Str]
type CountObservation = Observation[Int]
```

Allow type parameters on user record schemas and aliases, and applications in ordinary type positions, including qualified names such as `model.Observation[Int]`. This abstracts repeated data shape, not executable behavior. Keep generic functions/procs, generic error families, generic enums, trait bounds, typeclasses, higher-kinded types, variadic parameters, and runtime Type objects out of scope.

Parameters live in the declaration's lexical type scope. Enforce arity, duplicate/reserved names, fully supplied arguments, and existing variance/assignability rules after substitution. Do not default unsupplied parameters to Any or confuse parameters with ordinary bindings. Resolve imported definitions and their private dependencies in the declaring module.

Instantiate through the existing checked schema/type machinery and cache canonical instances within the checked program. Reuse the existing representation for a fully substituted record; do not specialize executable function bodies or add per-access runtime generic dispatch. Preserve current recursion restrictions and diagnose nonterminating/expanding type applications instead of endlessly constructing schemas.

Defaults must be valid for every allowed substitution. For example, null for T? and an empty List[T] can be checked structurally; a literal Int is not a valid universal T default. Use expected-type record literals and existing constructors for concrete named aliases. Do not introduce expression-level generic-call/index ambiguities just to construct an instance: `let sample: Observation[Int] = {...}` and `CountObservation(...)` suffice in this pass.

Integrate instances with field access, destructuring, nested containers, `.require`, constant preparation, schema diagnostics, and wire enums. Retain readable qualified type applications in tooling instead of exposing compiler expansion identifiers.

Migrate only actually isomorphic record shapes. Records with different provenance, raw-data fields, optionality, or error contracts must not be forced into one generic abstraction. Keep meaningful concrete aliases at public boundaries. Tests must demonstrate exact field-type preservation and rejection of a wrong specialization, not merely that several declarations parse.

## 4. Explicit string-backed enums at wire boundaries

```xsh
enum ObservationState: Str {
  Observed = "observed",
  Absent = "absent",
  Unsupported = "unsupported",
}
```

Add an explicitly Str-backed, payload-free enum form. Every variant must supply a unique constant string; permit intentional empty strings, but reject duplicates, missing mappings, payload fields, and mixed backing types. No casing conventions, automatic numbering, fallback/unknown variant, numeric discriminants, general serialization attributes, or tagged-JSON framework.

Inside XSH these remain nominal enum values with ordinary constructors, matching, equality, and exhaustiveness. A raw Str is not implicitly an enum, and identically spelled variants from different enums remain distinct. Do not change existing ordinary enums or implicitly encode their names.

At explicit `.require(Schema)` boundaries, a string-backed enum slot accepts an already correctly typed value or validates/converts an exactly matching Str into that enum. This is an explicit schema-directed decoding extension, not ambient assignment coercion. The compiled recursive schema walker handles nested records, lists, optional slots, and supported map values atomically: publish no partially converted trusted value if a later field fails. A type-pattern test checks the actual runtime type; it does not convert a string into an enum.

`json.encode`/write operations serialize these enum values using their declared strings, including nested occurrences. Raw `json.decode` stays raw/untyped until explicitly required. Reject unknown strings and mistyped values with field/index-aware errors. Defaults do not fill missing JSON fields. Keep custom schema-version adaptation, missing-field policy, redaction, and protocol migrations in their existing owners. Do not extend CLI/env/argv coercions or non-string map-key serialization implicitly.

Prepare each mapping once with the declaring schema and preserve namespace/type identity through imports and generic records. Preserve caller-visible wire strings exactly. Existing Result.context/ctx annotations and error translation stay available where callers need a domain error rather than the primitive schema error.

Replace hand-written enum-to-string/string-to-enum ladders only when the mapping, rejection policy, and expected error/wire behavior truly match. Preserve version-specific decoders and richer payload enums. Use golden wire fixtures, nested rejection tests, round-trips, same-string/different-enum tests, and ordinary-enum negative tests. The success criterion is deletion of redundant codecs without changing the protocol.

## 5. Make FsRoot the receiver of its own filesystem operations

```xsh
# Before
let root = fs.open_root(directory)?
defer fs.close_root(root)
let text = fs.root_read_text(root, p"etc/app.conf")?

# After
let root = fs.open_root(directory)?
defer root.close()
let text = root.read_text(p"etc/app.conf")?
```

Move the existing single-root operations to FsRoot methods, directly bound to the same host operations, and remove their redundant public module spellings. This adds no new filesystem ability and no layer of XSH wrapper functions.

Use explicit mappings: `fs.close_root(root)` -> `root.close()`, `fs.root_path(root)` -> `root.host_path()`, `fs.root(root, path)` -> `root.open_root(path)`, and `fs.root_read(root, path)` -> `root.read_bytes(path)`. For the remaining genuine single-root `fs.root_SUFFIX(root, ...)` operations, use `root.SUFFIX(...)`, preserving parameter names/order/defaults and return types. This includes bounded observation-returning calls; do not merge them into throwing read methods or rename absence into failure.

Retain module factories (`fs.open_root`, `fs.tempdir`, project/user roots, etc.). Keep genuinely multi-capability operations such as `fs.root_install_file` as module operations rather than arbitrarily picking one receiver. The explicit host_path escape remains fallible and is never inserted implicitly.

Resolve methods from the checked FsRoot type and the existing validated runtime handle. Do not structurally treat every `{id: Int}` record as a newly trusted capability. Preserve root confinement, native bytes, symlink/path validation, child-root ownership, closure behavior, errors, mocks, tracing, and effects. Never implement a root method by extracting a host Path and calling an ambient Path method. Add no resource protocol, automatic destructor, implicit working directory, or weakened platform adapter.

Reuse the registry's operation IDs and host dispatch; update signatures, effects, documentation, examples, completion, and coverage inventories. Keep removed names recognizable by migration tooling only, not executable aliases. Autofix resolved calls where receiver promotion retains argument evaluation order; reordered named/effectful arguments may require safe temporaries or no fix. User-defined similarly named calls are untouched.

Verify native temporary-root behavior, closed handles, independent child roots, escaping and confined symlinks, non-UTF-8 paths, bounded observation states, failure identity, and mocked operation IDs on macOS. Do not claim Linux confinement verification.

## 6. Retain typed causes when translating errors

```xsh
match compile(source) {
  Ok(value) => value
  Err(failure) => return Err(
    BuildError.CompileFailed(package: name),
    cause: failure,
  )
}
```

Extend the existing Err constructor with one optional named `cause: Error` argument. The first error remains the caller-visible nominal error/facets/payload; the supplied error is retained as its immediate diagnostic cause, including that cause's own chain, original span, contexts, and process status where applicable. Keep the normal Result success/error type inference and explicit propagation rules. A cause does not union the inner type into the outer error type or make outer pattern matching match inner facets.

This complements ctx: ctx describes the same failure in a broader operation, whereas a cause records deliberate translation into a different domain error. Do not add catch syntax, another exception mechanism, a throw/fail keyword, or a family of error-conversion methods.

Both arguments are evaluated once under existing source-order/named-argument rules. Construction returns Result data; it does not propagate. Attach metadata to an immutable copy/wrapper of the outer error, leaving input aliases unchanged. Explicit cause input sets the immediate cause on that new value; preserve the complete supplied cause chain. One-argument Err preserves its existing behavior and metadata.

Keep cause metadata separate from declared payload fields: do not reserve a new user payload name or add a script-level introspection API in this pass. Human diagnostics and existing structured error/trace output expose the causal chain with bounded, escaped rendering and actual error identities. Reuse the current error representations rather than flattening messages or converting ProcessError into generic text. Avoid deep copying entire chains at every propagation; handle rendering/traversal iteratively or with existing depth bounds. Hidden abort/cancellation control transfers cannot be captured and repackaged through this ordinary value constructor.

Do not automatically replace meaningful custom messages with a generic wrapper. Offer a targeted diagnostic when a handler creates a new error by flattening `failure.message` and otherwise loses the original; a cause addition is a deliberate diagnostic enhancement, not byte-for-byte error equality. Preserve custom payload fields and outer error contracts. Test nested translation, nominal outer matching, inner process metadata, contexts, aliases, long chains, and construction-versus-propagation boundaries.

## 7. Iterate Str and Bytes directly without materializing adapters

```xsh
# Before
for character in name.split("") {
  character in "abcdefghijklmnopqrstuvwxyz"
}

# After
for character in name {
  character in "abcdefghijklmnopqrstuvwxyz"
}

for octet in payload {
  octet <= 127
}
```

At direct for-loop and existing comprehension iterable positions, make Str produce one-scalar Str values in Unicode scalar order, and Bytes produce Int values in 0..255 in byte order. No Char type, grapheme segmentation, normalization, byte decoding, or new iterator methods. Empty values yield no items.

Use borrowed/owned source cursors and the existing string/byte view machinery. Evaluate the source once, retain its snapshot through iteration, and avoid building a character/byte List. Reassigning the original local does not change the retained value. Preserve immutable loop bindings, index-independent ordering, source spans, checkpoints, loop transfers, and cleanup. No thread or per-source function wrapper is involved.

Support the corresponding outer Result iterable under the same explicit/implicit iterable-propagation rules as existing loops, preserving the error type/effects. Do not redefine ordinary string operations or make a plain string silently mean lines in pipeline stage dispatch. This feature does not broaden the general stream-source, list-splice, or yield-delegation protocols; retain their documented domains.

Autofix direct iteration over `text.split("")` only for a proved Str and equivalent iteration semantics. A variable holding the split list and used elsewhere must remain a list. Replace indexed byte-traversal loops only when the index is unused except for the access, bounds cover precisely the entire stable source, and snapshot/evaluation behavior matches. Keep indexed scanners that need offsets.

Test multibyte scalars, combining sequences (separate scalars), NUL and invalid UTF-8 Bytes, empty values, reassignment of the source binding, nested loops, early exit, comprehensions, and Result-source errors. Use deterministic counters/representation checks to demonstrate no eager element list, not fragile timing assertions.

## 8. Use absence and one fallback mechanism instead of sentinel/default mini-APIs

```xsh
# Before
let position = text.find(":")
if position != -1 { consume(position)? }
let count = counts.get(key, 0)

# After
let position = text.find(":")
if position != null { consume(position)? }
let count = counts.get(key) ?? 0
```

Change Str.find's miss result from -1 to null, with return type Int?. Preserve successful byte offsets, search-start/empty-needle semantics, and any existing non-miss error behavior. Change Str.byte_at and Bytes.byte_at to Int? for an out-of-range access, returning a byte value otherwise, and remove their configurable fallback parameters. Preserve each API's existing indexing domain: do not silently make negative byte indices count from the end.

Remove the two-argument fallback overloads of List.get and Map.get. Keep their one-argument Result-returning forms and use ordinary `??` for caller fallback. Crucially, **do not change List/Map.get into Optional**: Ok(null) in an optional-valued collection must remain distinguishable from a missing entry and must not trigger a fallback. Lookup errors remain typed errors unless the caller explicitly recovers.

This is a bounded normalization of these concrete surfaces, not a rewrite of every numeric sentinel in OS observations, every `_or` API, or every fallible function. Keep exceptions separate from ordinary absence, and do not add split/search helper functions, a new operator, or truthiness. Preserve byte-indexed text semantics, including UTF-8 boundary requirements of subsequent slicing APIs.

Update registry, lowering fast paths, checker facts, docs, and maintained users together. Recognize removed overloads in migration tooling without publishing executable aliases. For legacy code that deliberately computes with the sentinel, an explicit `find(...) ?? -1` retains that policy. Do not make nullable arithmetic legal to avoid migration work.

Old fallback arguments may run eagerly, while `??` runs its fallback only when needed. Direct fixes need inert, non-failing fallback expressions or safe temporaries at the exact old evaluation point; account for receiver/index reads and mutation before performing the lookup. Otherwise decline the fix. Rewrite sentinel comparisons only for values proved to originate from the affected APIs, not arbitrary `-1` checks. Test hits at offset zero, absence, empty needles, invalid starts, present-null collection values, eager fallback effects, and fast-path parity. No runtime object allocation is required merely to represent Int-or-null.

## 9. Signature-preserving aliases instead of forwarding functions

```xsh
# Before
use compiler
export proc compile(source: Path, output: Path) [process, error] {
  compiler.compile(source, output)?
}

# After: callers still use api.compile(...).
use compiler
export let compile = compiler.compile
```

Preserve the complete checked callable signature when an immutable let directly aliases a statically resolved user-defined pure/proc, including a qualified imported callable or another such alias. A call through the alias uses ordinary call syntax with the original parameter labels, defaults, overload resolution where already supported, return type, pure/proc kind, and effect contract. It must not degrade into erased `.call(...) -> Any` dispatch.

This is symbol/signature-preserving aliasing, not a new callable type system, bound-method facility, partial application, generic function feature, closure expression, or new reexport syntax. Keep existing dynamic/erased callable behavior for var bindings, conditional/computed function selection, arbitrary callable-producing expressions, and explicitly erased boundaries. Do not make callable values eligible for const preparation.

Resolve aliases through normal lexical/module identities, with cycle diagnostics and no per-call string lookup. Creating the alias does not call the function, evaluate its defaults, or move its captured environment. Preserve ordinary alias-initializer timing and existing capture/resource lifetimes. Runtime calls may use the same callable handle/target; do not generate a forwarding function frame.

An exported alias exposes a callable with that contract, so existing module contracts can recognize its pure/proc signature rather than classifying it as an untyped data export. It exposes only the named item; do not inject names into importing scopes, reexport every implementation symbol, or leak internal stdlib identities. Preserve explicit public effect/return guarantees; diagnose an export whose target provides only an inferred private contract when no explicit public contract exists. Tools must navigate to the original definition while exposing the public alias name.

Autofix transparent forwarders only when the exact parameter list/defaults/effects/returns match, each argument is forwarded unchanged once in order, and no conversions, wrapping policy, control flow, logging, cleanup, or meaningful context is added. Removal of the redundant wrapper traceback frame is intentional and documented. API names must not change. Test aliases across module boundaries, named/punned/spread calls, defaults and overloads, effect violations, native stage callbacks, reachability, dynamic erasure, and aliases of aliases.

## 10. Put accepted process exit codes at the run boundary

```xsh
run --accept=[0,1] diff previous current
let matches = run.text --accept=[0,1] rg pattern source ?
```

Add one run policy option `--accept=EXPR`, checked as a nonempty List[Int] of exit codes 0..255. It explicitly requests completion validation against that set. Reuse existing run-option expression parsing and process policy plumbing; add no new status type, operator, command literal, generic predicate, ignore-errors mode, or success-check helper. Duplicate entries may be rejected with a clear policy diagnostic rather than silently expanding work.

Evaluate/validate the option once at the established run-option position, before spawning that invocation. Preserve other argument/redirect evaluation rules. With no option, every mode retains its existing status/failure contract. With an option, normal completion is accepted only for an ordinary exit whose code belongs to the set. An accepted code remains the child's actual code; `.ok` still means actual zero success, `$?` is not normalized, and capture records preserve the true Status. Never accept signal termination by treating 128+signal as an ordinary exit. Setup errors, timeout, cancellation, capture limits, and decoding/I/O failures remain failures regardless of the set.

Apply the explicit completion contract consistently across ordinary run, text/byte capture, status/capture-record modes, and existing process-stream completion. Result-producing forms return their usual Result shape with Err on rejection; direct Status forms use their established propagation/assertion failure mechanism when this explicit check rejects. A stream may fail at completion after producing rows; do not buffer it to hide that boundary. Explicit status validation contributes the normal error effect even when the unconfigured mode only exposes status as data.

Carry the policy through existing Command/owned-process machinery without inventing new construction/execution syntax. If a policy-bearing invocation is spawned, ordinary wait applies it through Result[Status] and preserves list-wait drain/reaping rules; explicit cancel/scope cancellation remains cancellation, not an invented unexpected-exit failure. In pipelines the set belongs to its own run segment; it must not excuse an unrelated failing segment. Keep preexisting first/primary-error precedence and diagnostic status/source metadata.

Do not broadly autofix custom status handlers: branch-dependent behavior, custom errors, output decisions, and diagnostics are policy. Migrate reviewed cases that only declare accepted completion codes; retain genuine status inspection. Test accepted/nonaccepted exits, actual status metadata, signal/setup failures, capture/decode limits, late stream errors, spawned waits, pipeline siblings, invalid options before spawn, and literal external argv named `--accept` after the executable. Use deterministic self-contained child fixtures, not installed diff/rg versions, for acceptance tests.

