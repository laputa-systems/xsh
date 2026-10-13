# XSH formal project

This is the Lean foundation for the XSH soundness campaign. It is pinned to
Lean 4.34.1 and uses only Lean's bundled libraries; no XSH runtime or product
build depends on it. The current `Xsh` root imports every module under
`formal/Xsh/`. These modules define and prove fragments for exact scalar and
word representations; lexical environments, activations, and captured cells; a
checked integer evaluator with typed scalar values; validated boundaries and
atomic typed-store writes; and effect lattices, recursive summaries, lexical
bounds, and delayed-work origins. Failed typed writes preserve the old store
and produce an uncatchable type error; `.require(UInt)` remains recoverable.
The effect fragments also prove that an abstract finite pull prefix preserves
creation-site charge order and its corresponding remainder, that each charge
retains its creation and body sites, and that a supplied creation-context
permission premise implies permission for every pulled charge. The
`DelayedWork`/`pull` model uses that premise only in the permission result; it
does not establish Rust checker classification or diagnostics, stream aliases,
delegation, close semantics, or conformance, and it closes neither
`EFF-LATENT` nor `EFF-SEMANTIC`. The raw IR layer models typed pool IDs,
optional IDs, ranges, tables, and row decoding. Its decoder fragments prove
successful sequential read length, successful payload length and table bounds,
and rejection of one concrete out-of-bounds payload range. These independent
fragments do not close G7a, establish full decoder soundness, or prove Rust
verifier conformance. Transport modules define closed bridge codecs and prove
re-encoding laws for accepted hexadecimal values and bridge IDs. These are
independent Lean results about their stated definitions, not Rust conformance
or full-language proofs.

Finite private Rust bridge fixtures exercise selected JSON limits, table
bounds, and ghost-reference rejection. They are empirical tests of that
private bridge; they are separate from the Lean theorems and do not establish
general Rust conformance. Consolidation acceptance and the post-consolidation
G0b source freeze remain pending. The provisional inventory still marks all
72 complete theorem obligations planned. The inventory names 63 rooted Lean
fragment declarations. Its 29 Python tests check inventory shape, root
reachability, and mutation controls; they are not semantic or Rust conformance
tests. No full G1 closure or whole-language soundness claim is made.

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
external assumption. The executable Float comparator
in `Xsh.Values.FloatEquality` is specified against the raw-bit IEEE value
relation, with symmetry, transitivity, and reflexivity exactly for non-NaNs.
Signed zeros compare equal while retaining distinct raw bits, transport
encodings, and total-sort order. `tests/xsh/float-equality.xsh` includes a
constant-folding regression for signed zeros, NaNs, and nested values. These
Lean laws and the source regression do not establish Rust conformance.

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
