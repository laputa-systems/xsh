# XSH formal project

This is the Lean foundation for the XSH soundness campaign. It is pinned to
Lean 4.34.1 and uses only Lean's bundled libraries; no XSH runtime or product
build depends on it. The current `Xsh` root imports every module under
`formal/Xsh/`. These modules define and prove fragments for exact scalar and
word representations; lexical environments, activations, and captured cells;
checked integer outcomes and millisecond-duration operations over typed scalar
values; List index and slice bounds; validated boundaries and atomic typed-store
writes; and effect lattices, recursive summaries, lexical
bounds, and delayed-work origins. Failed typed writes preserve the old store
and produce an uncatchable type error; `.require(UInt)` remains recoverable.
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
mutation. It does not establish XSH checker/runtime behavior or Rust
conformance.

Finite private Rust bridge fixtures exercise selected JSON limits, table
bounds, and ghost-reference rejection. They are empirical tests of that
private bridge; they are separate from the Lean theorems and do not establish
general Rust conformance. Consolidation acceptance and the post-consolidation
G0b source freeze remain pending. The provisional inventory has 232 fragment
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

`Xsh.Validation.RecursiveRequire` models `.require` over a finite target
grammar for integers, strings, optional values, lists, and required record
fields, using explicit predecessor fuel for each recursive check. It rejects
duplicate record keys while preserving extra uniquely named fields. Its 11
theorem roots include nested success and failure witnesses, optional `null`,
required optional-valued fields, exact erasure and provenance on success, and
recoverable rejection that leaves the modeled store unchanged. This independent
finite model establishes no Rust correspondence or full G2 validation claim.

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

The inventory names 232 rooted Lean fragment declarations across 89 core owner
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
