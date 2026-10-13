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
The effect fragments model lexical rules but do not establish Rust checker
classification or diagnostics. The raw IR layer models typed pool IDs,
optional IDs, ranges, tables, and row decoding. Transport modules define
closed bridge codecs and prove re-encoding laws for accepted hexadecimal
values and bridge IDs. These are independent Lean results about their stated
definitions, not Rust conformance or full-language proofs.

Finite private Rust bridge fixtures exercise selected JSON limits, table
bounds, and ghost-reference rejection. They are empirical tests of that
private bridge; they are separate from the Lean theorems and do not establish
general Rust conformance. Consolidation acceptance and the post-consolidation
G0b source freeze remain pending. The provisional inventory still marks all
72 complete theorem obligations planned. No full G1 closure or whole-language
soundness claim is made.

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

The fixed source/IR corpus, scenario tests, and replay runner have not been
introduced yet. Direct Lake builds and the axiom controls above are the
available bootstrap checks; later campaign work will add those runners under
`formal/`.
