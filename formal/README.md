# XSH formal project

This is the Lean foundation for the XSH soundness campaign. It is pinned to
Lean 4.34.1 and uses only Lean's bundled libraries; no XSH runtime or product
build depends on it. The current `Xsh` root imports every module under
`formal/Xsh/`. These modules define and prove fragments for exact scalar and
word representations; lexical environments, activations, and captured cells;
checked integer outcomes and millisecond-duration operations over typed scalar
values; finite map-presence and primitive key-order laws; checked call argument
ordering; checked record updates and finite Map literal evaluation; JSON-tree
paths, bounded JSON compatibility projection, and compact JSON text encoding;
byte-lexical path components, POSIX lexical path fields, and lexical path
normalization; Str-backed enum wire conversion; List index and slice bounds,
bounded List-comprehension execution, eager finite loop snapshots, and finite
loop-local control traces;
Unicode scalar text-slice laws and UTF-8 byte search; Result `?` propagation
boundaries and returned-Result data; finite
`.require(Set[T])` validation and array conversion; validated
boundaries and atomic typed-store writes; and effect lattices, recursive
summaries, lexical bounds, and delayed-work origins. Failed typed writes
preserve the old store and produce an uncatchable type error; `.require(UInt)`
remains recoverable. Optional-access traces cover lazy field, method, index, and
range hops plus Result propagation, while byte access covers `Str` byte lookup
and explicit-length UTF-8 slices.
The effect fragments also prove that an abstract finite pull prefix preserves
creation-site charge order and its corresponding remainder, that each charge
retains its creation and body sites, and that a supplied creation-context
permission premise implies permission for every pulled charge. Five observation
theorems show that positional pairs preserve supplied emission order, retain
the creation site, body site, and required effect, and carry creation-site
permission under explicit obligation alignment; they also equate aligned
observation charges with a finite pull sequence and discharge a listed checked
consumer obligation at its own context. These results use caller-supplied
emissions and classified obligations. Their `Expr` site labels are structural
and cannot distinguish identical source occurrences. The
`DelayedWork`/`pull` model does not execute producers or establish Rust checker
classification or diagnostics, stream aliases, delegation, close semantics,
full latent-effect semantics, or conformance. The closed `ExpressionBridge`
fragment checks classified producer bodies at each call's lexical context,
retains the checked producer and cursor through aliases, records delegated
origins and body sites, and keeps both conditional alternatives tagged. Its
witnesses accept an outside-created producer consumed inside `without net`,
reject creation inside that region, reject a missing alias, an unknown source
form, and a blocked conditional arm, and show that a restricted consumer does
not move the charge site. These are checker facts for the represented finite
source forms only; they do not establish Rust source classification, full
stream semantics, or conformance. `Xsh.Effects.Capture` models removal of the
outward `error` fact while preserving other named effects and unresolved work.
It proves idempotence and commutation with the represented effect closure,
permutation and duplicate invariance for finite effect clauses, and concrete
witnesses that dropping named effects or unresolved work changes restricted-call
acceptance. A captured missing-contract edge retains its explicit unresolved
fact in the finite solver graph. These laws and witnesses concern represented
summaries and graph edges only; they do not establish Rust checker
classification or propagation, complete solver behavior, full `EFF-CAPTURE`,
`EFF-LATENT`, or `EFF-SEMANTIC` semantics, or conformance. Together, the effect
fragments close neither `EFF-LATENT` nor `EFF-SEMANTIC`. The raw IR layer models typed pool IDs,
optional IDs, ranges, tables, and row decoding. Its decoder fragments prove
successful sequential read length, successful payload length and table bounds,
and rejection of one concrete out-of-bounds payload range. These independent
fragments do not close G7a, establish full decoder soundness, or prove Rust
verifier conformance. Transport modules define closed bridge codecs and prove
re-encoding laws for accepted hexadecimal values and bridge IDs. These are
independent Lean results about their stated definitions, not Rust conformance
or full-language proofs.

The argv fragment proves nine laws and witnesses about expanding already
classified command words: ordinary words remain single items, splices preserve
their items and order, and a space-splitting mutation changes the result. It
does not model parsing, interpolation, path conversion, globbing, NUL
validation, process creation, or host behavior. The primitive-classification
fragment proves nine results for five named host operations under the
represented lexical effect rules, including two classification mutation
witnesses. It does not classify arbitrary source
syntax, establish registry or Rust correspondence, or close G3. The ordered-set
fragment proves eleven laws and witnesses for finite sets over ordinal keys,
including extensional membership, increasing iteration, and an insertion-order
mutation. The finite map-presence fragment proves twelve laws for lookup,
`.get`, presence, setting, removal, and a null-as-absence mutation witness over
finite ordinal keys. It distinguishes a present `null` from an absent key. It
does not model key validation, comparators, ordering, iteration, JSON, diagnostic
tags, XSH runtime behavior, or Rust conformance.

Finite private Rust bridge fixtures exercise selected JSON limits, table
bounds, and ghost-reference rejection. They are empirical tests of that
private bridge; they are separate from the Lean theorems and do not establish
general Rust conformance. Consolidation acceptance and the post-consolidation
G0b source freeze remain pending. The provisional inventory has 508 fragment
roots across 89 core owner sets and still marks all 72 universal theorem
obligations planned. Integer operation fragments prove
checked conversion's exact signed-range split and total outcomes for addition,
subtraction, multiplication, division, and remainder. Concrete mutation
witnesses distinguish checked overflow from wrapping addition and truncating
remainder from Euclidean remainder. These are results about the Lean integer
model; they do not establish Rust arithmetic conformance or checker integration.

`Xsh.Semantics.ListAccess` has eleven roots for List reads, assignment-index
normalization, and clamped slice bounds. Literal negative reads count from the
end while computed negative indices and negative assignment indices are rejected.
The model does not cover parsing, diagnostic phases, store updates, String or
Bytes slicing, signed overflow, or the Rust evaluator.

`Xsh.Semantics.ListExecution` has fourteen roots for eager execution of a
finite two-source List comprehension. The model records source, filter, and
projection invocations; evaluates the inner source for each outer binding;
preserves filter and loop-nesting order; and publishes only a complete result.
Its mutation witness detects hoisting an inner source that depends on the outer
binding. It uses abstract finite expression functions and does not model
parsing, general expression effects, streams, cleanup, or XSH runtime
correspondence.

`Xsh.Iteration.Snapshot` has twelve roots for finite eager loop sources. It
captures List order, the selected Map key order, Unicode scalar iteration for
Str, unsigned values for Bytes, and Result success or error handling. Body
frames preserve their current item when the loop source is rebound. The model
excludes streams and pipelines, loop control, typechecking, and runtime or Rust
correspondence.

`Xsh.Loops.ControlFlow` has twelve roots for finite loop-local outcomes and
traces over a preselected finite item sequence. It covers current-loop continue
and break, nested loop targets, return and failure propagation, one-time repeat
count evaluation, finite ranges, and exact traces. It excludes checker target
validity, stage callbacks, divergence, effects, cleanup, streams, and runtime
or Rust correspondence.

`Xsh.Paths.Normalize` has twelve roots for native-byte lexical stack
normalization. It removes empty and dot components, cancels ordinary components
before `..`, preserves leading relative parents, clamps parents at an absolute
root, and retains component bytes. Its repeated-normalization result is a
concrete example, not a general idempotence law. Runtime NUL admission, cwd and
filesystem resolution, symlinks and confinement, path fields, construction and
interpolation, and runtime or Rust correspondence are excluded.

`Xsh.Calls.ArgumentOrder` has fourteen roots for already-checked finite call
layouts. The model preserves receiver, written argument-source, and omitted
default event order; carries source events through list splices and visible
record spreads; and treats a supplied `null` as present. Its mutation witness
shows parameter-order sorting changes source events from `[21, 20]` to
`[20, 21]`. It assumes resolved parameter IDs, pre-evaluated events, and the
visible fields supplied by a checked spread. It does not model parsing,
expression evaluation or checking, duplicate labels, overload resolution,
function entry or effects, Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Collections.KeyOrder` has eleven roots for separate Int, UInt, Bool,
String, Bytes, Path, and Duration key domains. It proves reflexivity,
antisymmetry, and transitivity for the represented orders, with a concrete
witness that reversing Boolean order changes the result. The fragment does not
prove totality or model map/set operations, key validation, path normalization,
runtime behavior, Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Results.Fallback` has ten roots for modeled `??` selection over Result and
optional sources. It records only selected fallback-tail calls, preserves the
original error in a Result handler context, models a right-associated chain,
and detects an eager-call mutation on a successful Result. The tail is an
opaque callback; the fragment does not model general expression or operator
evaluation, `?`, cleanup, static effects, concrete runtime errors, Rust
correspondence, or full G1/G2/G3 closure.

`Xsh.Results.Propagation` has twelve roots for finite nearest-boundary behavior
of `?`. It models success, the nearest try or retry boundary, compatible error
conversion at a Result function, failure unwinding from a plain procedure, and
the distinction between returned Result data and propagation. The compatible
error embedding is an explicit premise. The fragment does not prove source
typing, cleanup, effects, concrete runtime behavior, Rust correspondence, or
full G1/G2/G3 closure.

`Xsh.Json.Path` has eighteen roots for finite JSON-tree traversal, nested reads,
updates, and removals using string keys and nonnegative indices. Its Float leaves
exclude NaN and infinities; Records and Maps remain distinct. An append-at-length
mutation witness detects extension beyond an existing list index. The model
does not cover non-String Map-key validation, key uniqueness policy, source
integer-width bounds, parsing, encoding, diagnostics, JSON validation, runtime
or Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Json.Compatibility` has fourteen roots for recursive projection of a
bounded value fragment into a JSON tree. It preserves scalar and list values,
sorts record and Map fields, requires string Map keys and finite Float leaves,
uses prepared enum wire spellings, requires an explicit conversion for paths,
and does not publish partial output after failure. Record and Map entries are
assumed unique. The Set theorem is only an ordered ordinal-key abstraction that
emits ordinals as a JSON integer array; it does not model general `Set[T]`
serialization. The fragment excludes JSON text encoding/parsing, source-width
validation, diagnostics, runtime, and Rust correspondence.

`Xsh.Json.Encoding` has twelve roots for compact encoding of a bounded finite
JSON tree. It preserves array order, sorts object fields by the shared string
key order, escapes quotes, backslashes, and control characters, and preserves
printable Unicode. Float leaves and exhausted fuel produce no text. The
bounded fragment does not establish runtime or Rust correspondence; Float
formatting, pretty output, parsing, and I/O are excluded.

`Xsh.Records.Update` has twelve roots for checked functional updates to existing
record paths. The model covers path-prefix overlap checks, one evaluation of the
base, source-order replacement evaluation, failure short-circuiting without
partial publication, updates rooted in the original snapshot, preservation of
untouched nested fields, and record-shape preservation. Its overlap rule mirrors
the current checker: two targets overlap when either path is a prefix of the
other. `docs/SPEC.md` currently omits this record-update overlap rule. Replacement
values are supplied after checking; this fragment does not establish parser,
checker, diagnostic, runtime, or Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Maps.Literal` has twelve roots for checked finite Map literal evaluation.
The model records key-before-value evaluation, source-ordered entries and
spreads, failure short-circuiting without publishing a partial map, later-value
replacement, and present-key iteration in the selected primitive key order. It
assumes one already validated finite key domain with an injective, ordered key
table and abstract expression outcomes. It does not model arbitrary expression
effects, parsing, key validation or coercion, cross-domain conversion, runtime,
or Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Strings.Slice` has eleven roots for text slicing represented as finite
Lean `Char` sequences. The results cover normalized scalar bounds,
end-relative indexing, omitted bounds, source order, and a mutation witness
where normalizing by the example's UTF-8 byte size changes the modeled scalar
slice. The fragment does not model XSH UTF-8 storage, `Bytes` operations, text
search, parsing, runtime evaluation, Rust correspondence, or full G1/G2/G3
closure.

`Xsh.Strings.Search` has eleven roots for first-match search over UTF-8 byte
lists, including negative and past-end starts, empty needles, byte-offset
bounds, a multibyte prefix, and a scalar-index mutation witness. This is an
independent model over Lean's `String.toUTF8` bytes; it does not prove the XSH
storage or runtime implementation, normalization or parsing, raw `Bytes`
search APIs, Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Control.OptionalAccess` has twelve roots for lazy field, method, index,
and range access over an already evaluated nullable receiver, plus Result
propagation before method arguments and a mutation witness for eager argument
evaluation. It does not model checker diagnostics, effects, cleanup, runtime
parity, Rust correspondence, or full G1/G2/G3 closure.

`Xsh.Strings.ByteAccess` has twelve roots for `Str` byte lookup and UTF-8
boundary-checked slicing with an explicit byte offset and length. It excludes
the omitted-length form, the `Bytes` API, bracket slicing, parsing, runtime
errors, `UInt8` conversion, runtime evaluation, Rust correspondence, and full
G1/G2/G3 closure.

`Xsh.Values.DurationOps` has eighteen roots for checked millisecond addition,
subtraction, multiplication, division, interval ratios, and time conversions.
Its `time.millis` saturation law uses mathematical `Int`; actual XSH signed-64-bit
`IntValue` inputs cannot reach the unsigned duration maximum. The model does not
cover parsing, evaluation, source spans, compound assignment, or Rust behavior.
`Xsh.Validation.Nominal` models declaration identity as module plus declaration
name, tracks constructor and `.require` introduction provenance, erases nominal
wrappers to their represented values, and checks nominal union membership. Its
ten theorem roots include witnesses for equal representations with distinct
identities and mutations that collapse IDs or erase provenance. The fragment
does not validate record schemas or `.require` inputs, prove union
exhaustiveness, or establish source/Rust conformance or full G2.

`Xsh.Validation.RelPath` models a byte-lexical relative-path boundary with a
subtype that preserves the exact candidate bytes. Its 21 theorem roots cover
lexical depth changes, empty and dot components, escaping parents, NUL
rejection, non-UTF-8 names, and the modeled `require` introduction, erasure,
acceptance, and rejection laws. This independent byte-list model does not
establish Rust checker/runtime correspondence, symlink-aware filesystem
confinement, or full G2 validation closure.

`Xsh.Paths.Components` has fourteen roots for lexical path-component scanning
and component-sequence prefix/suffix queries. It reuses `PathBytes`, slash and
dot bytes, and byte constructors from `Xsh.Validation.RelPath`; its
`splitOn`/`collectNames` scan is an independent Lean model rather than a proof
about a shared Rust scanner. The roots cover absolute-root identity, repeated
and trailing separators, leading and interior dot components, parent and
non-UTF-8 name bytes, complete-component prefix/suffix behavior, and a raw-byte
prefix mutation witness. They do not model filesystem access, normalization,
name or parent queries, RelPath validation, or Rust correspondence.

`Xsh.Paths.Fields` has twelve roots for final-name, POSIX basename and dirname,
lexical parent, and extension queries over native bytes with `/` as the
separator. The model performs no text decoding or component normalization. It
does not match every Rust-backed XSH `Path.name`, `Path.ext`, or `Path.parent`
edge case, including `foo/.` and `foo/..`; it establishes no correspondence
with those XSH APIs. Filesystem resolution and `with_ext` are excluded.

`Xsh.Enums.WireConversion` has seventeen roots for prepared Str-backed enum
mappings layered over the shared recursive `.require` boundary. The model
assumes each mapping is nonempty and has unique declaration names and wire
spellings; it does not validate source declarations. Enum identity uses
`NominalId`. The shared structural projection represents maps as open records
and reports structural rejection through pathless `ValidationFailure`; the
separate conversion pass visits every Str-keyed map value and reports
`BlamePath` for conversion failures. The wrapper exposes a converted value only
after both stages succeed. A mutation witness skips map-value validation. The
fragment excludes implicit coercions, JSON encoding, diagnostic spans, and
Rust correspondence.

`Xsh.Validation.RecursiveRequire` models `.require` over a finite target
grammar for integers, strings, optional values, lists, and required record
fields, using explicit predecessor fuel for each recursive check. It rejects
duplicate record keys while preserving extra uniquely named fields. Its 11
theorem roots include nested success and failure witnesses, optional `null`,
required optional-valued fields, exact erasure and provenance on success, and
recoverable rejection that leaves the modeled store unchanged. This independent
finite model establishes no Rust correspondence or full G2 validation claim.

`Xsh.Validation.SetRequire` has ten public roots for finite `.require(Set[T])`
validation. Existing sets retain membership only when every member matches the
target predicate; decoded arrays convert only after all members match and keys
are unique, and accepted sets iterate in primitive key order. A duplicate
mutation witness distinguishes rejection from accidental deduplication. The
private `decoded_array_acceptance_iff_typed_nodup` lemma is compiled proof
support, not a coverage root. The model assumes a finite order-preserving key
table and target predicate; it excludes JSON parsing, diagnostic text,
checker/runtime and Rust correspondence, and full G1/G2/G3 closure.

`Xsh.Validation.BoundedRefinements` proves interval inclusion, exact-payload
widening and composition, narrowing rejection, and a mutation witness for a
missing upper-bound check. `Xsh.Validation.NonEmptyFiltering` relates a
nonempty filtered list to a surviving input, preserves the filtered sequence
after successful validation, and rejects an all-false filter. These 14 roots
cover selected operations in the independent validation model; they do not
cover every validated-type operation or establish Rust correspondence or full
G2.

`Xsh.Effects.BoundFailureWitnesses` preserves the source expression, required
effect, and rejection reason for a classified obligation.
`Xsh.Effects.SolverEdgeOrder` proves that edge permutations preserve the finite
recursive solver answer and detects a first-edge-only mutation. These 15 roots
cover bounded failure classification and graph operations; they do not
establish Rust checker or graph-construction correspondence or full G3.

The inventory names 508 rooted Lean fragment declarations across 89 core owner
sets. Its 50 Python tests check inventory shape, root reachability, mutation
controls, and finite-case
evidence boundaries; they are not semantic or Rust conformance tests. No full
G1, G2, or G3 closure or whole-language soundness claim is made.

The base-machine fragment includes a typed `time.now()` request for signed
epoch milliseconds. `Xsh.Semantics.BaseHostTrace` labels internal steps,
requests, and responses; proves that accepted and rejected response edges
preserve the fragment's machine invariant; and defines a finite runner driven
by scripted responses. The generic finite-fuel runner is proved to produce a
valid prefix trace, and valid traces preserve the machine's well-formedness
invariant. A mutation witness shows that omitting a consumed mismatched
response event cannot form a trace from the waiting state to failure. A
concrete script issues two requests in order, returns 2 then 3, and completes
with 5; a Boolean response is labeled and rejected with a type-mismatch
fault. `Xsh.Proofs.BaseHostRunnerLaws` proves that terminal runs ignore extra
fuel and appended responses and that empty scripts leave waiting machines
unchanged. Its finite witnesses show that a valid response resumes a wait and
that changing response order changes the returned value. These results concern
only the finite scripted formal runner. They do
not establish source or whole-language soundness, Rust or OS conformance,
concrete host behavior, or clock accuracy; clock correspondence remains an
external assumption. The Lean comparator in `Xsh.Values.FloatEquality` takes
raw 64-bit Float encodings and computes IEEE value equality. Its specification
proves symmetry, transitivity, and reflexivity exactly for non-NaNs. Signed
zeros compare equal while retaining distinct raw bits, transport encodings,
and total-sort order. This semantic comparison is distinct from bitwise
representation equality used by `FloatValue::eq`. The
`tests/xsh/float-equality.xsh` source regression includes signed zeros, NaNs,
and nested values. These Lean laws and the source regression do not establish
Rust conformance.

Coverage schema 2 prepares the finite conformance ledger for G0b. The current
inventory remains provisional, has no frozen source identity, contains no
finite conformance cases, and keeps all 72 universal theorem obligations
planned. A frozen identity uses a full Git commit and its exact root tree; the
commit must exist in the checkout, and it may be an ancestor of `HEAD`. Each
named `RUST_CONFORMANCE` case requires a stable `test_id` present as an exact
argv element. The only supported forms are `xsht test --exact <test_id>` and
`cargo test --profile <profile> --target <target> -p <package> --lib
<test_id> -- --exact` or the corresponding `--test <target>` form. Source-test
cases identify both sibling products with separate `xsht` and `xsh` SHA-256
roles. Cargo-test cases identify the Cargo executable and package test
artifact separately with `cargo` and `cargo-test:<package>:<target>` roles.
Every role has its own SHA-256; profile and target are recorded, and Cargo
values must match the command. `expected_result` is a predeclared normalized
oracle with exactly `test_id`, `exit_code`, `passed`, `failed`, `skipped`, and
`filtered`; it requires that named test to pass once with no failures or skips.
For `xsht`, `filtered` is null because its summary does not report that count;
Cargo's filtered-out count is retained.
`actual_result` retains captured command output as `exit_code`, `stdout`, and
`stderr`. The checker parses the runner's result line and summary, ignores
duration text only, maps Cargo's ignored count to `skipped`, and compares the
normalized outcome with the oracle. It requires one named `ok` result and a
summary with exactly one pass and zero failures, skips, or ignored tests. This
rejects the zero-test success result that either exact filter can produce.
The agreement is finite evidence for the named case only. Provisional cases,
missing or mismatched identities, duplicate names, other evidence classes,
trivial or mismatched commands, invalid result records, and failed cases are
rejected. No finite evidence is recorded here.

From this directory, keep Lake's metadata, manifest, cache, build outputs, and
temporary files under the repository's ignored `.work/` directory. Add that
directory to the worktree's Git exclude file before creating it:

```sh
repo_root=$(git rev-parse --show-toplevel)
exclude_path=$(git rev-parse --git-path info/exclude)
if ! rg -qxF '.work/' "$exclude_path"; then
  printf '.work/\n' >>"$exclude_path"
fi
mkdir -p "$repo_root/.work/tmp" "$repo_root/.work/lake-cache"
export TMPDIR="$repo_root/.work/tmp"
export LAKE_CACHE_DIR="$repo_root/.work/lake-cache"
lake --dir="$repo_root/.work" --file="$repo_root/formal/lakefile.toml" build
```

Lake uses `.work/` as its workspace root, so its fixed metadata directory and
generated `lake-manifest.json` stay there. The package config points Lean
sources at `formal/` and build/dependency outputs into `.work/`; the environment
routes Lake's cache and temporary files there as well.

Audit every declaration in the project modules:

```sh
scripts/audit-axioms.sh
scripts/audit-axioms.sh --self-test
```

Run the fresh kernel check through its guarded runner:

```sh
python3 scripts/check-fresh.py
```

The audit discovers every Lean source module under `formal/` outside
`scripts/`, builds and imports those modules, and checks declarations by their
module identity. This catches project declarations whose namespace is outside
`Xsh` and modules not imported by the root. It rejects unsafe declarations,
project axioms, and transitive dependencies beyond `propext`, `Quot.sound`,
and `Classical.choice`. The self-test checks that an axiom in an outside
namespace, `native_decide`, and `sorry` are rejected. Fresh kernel checking
and this allowlist audit are separate checks.

The fresh-check runner enforces a 180-second wall limit and an 8 GiB maximum
address space for Lake and the checker process. If the time limit expires, it
terminates the whole process group.

Check the provisional inventory and its Python tests from the repository root:

```sh
python3 formal/scripts/check-inventory.py
python3 formal/tests/inventory/test_check_inventory.py
```

The fixed source/IR corpus, semantic scenario tests, and replay runner have
not been introduced yet. The inventory tests above validate inventory
structure and root reachability only. Direct Lake builds and the axiom
controls above remain the available proof bootstrap checks.
