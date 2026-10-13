# XSH formal project

This is the Lean foundation for the XSH soundness campaign. It is pinned to
Lean 4.34.1 and uses only Lean's bundled libraries; no XSH runtime or product
build depends on it. The current foundation proves exact round trips for
32-bit words, binder IDs, name IDs, and signed 64-bit values. The scalar model
also defines bounded UInt and Duration values, raw Float bits with IEEE
equality and total ordering, and NUL-free native paths. These are independent
value-domain proofs, without a Rust conformance or whole-language soundness
claim.

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
