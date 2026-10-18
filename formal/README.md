# XSH formal project

This maintenance project uses Lean 4.35.0, pinned in [`lean-toolchain`](lean-toolchain).
Cargo builds and XSH products do not depend on Lean. [`Xsh.lean`](Xsh.lean)
imports mathematical models of selected XSH behavior. A `SPEC_PROVED` theorem
proves a property of its Lean definitions; it does not establish correspondence
with XSH source, indexed IR, Rust runtime, or host behavior.

[`coverage.json`](coverage.json) is the authority for provisional coverage:
`source_identity` is null, every universal obligation remains planned, and the
finite Rust-conformance ledger has no cases. The fixed source/IR corpus,
semantic scenario suite, and replay runner have not been introduced. Private
bridge fixtures are not conformance cases.

[`check-inventory.py`](scripts/check-inventory.py) and its
[tests](tests/inventory/test_check_inventory.py) validate inventory structure,
owner coverage, root reachability, and evidence records. They do not test Lean
theorem behavior or Rust conformance. [`xsh-lean.md`](../xsh-lean.md) is the
paused, owner-managed plan and is parsed by the inventory checker; preserve its
G0–G10 structure when updating it.

## Build and check

The [Lake config](lakefile.toml) keeps build and package output under the
repository's `.work/` directory. Before creating it, ensure `.work/` is ignored
through this checkout's Git exclude file. From the repository root:

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

Use the pinned toolchain on `PATH`. From the repository root, check the
inventory:

```sh
python3 formal/scripts/check-inventory.py
python3 formal/tests/inventory/test_check_inventory.py
```

From `formal/`, run the [project axiom audit](scripts/audit-axioms.sh), its
self-test, and the [fresh paranoid check](scripts/check-fresh.py):

```sh
scripts/audit-axioms.sh
scripts/audit-axioms.sh --self-test
python3 scripts/check-fresh.py
python3 -B tests/test_check_fresh.py
```

The audit rejects unsafe declarations, project axioms, `native_decide`, and
`sorry`; its permitted dependency list is defined in the linked script. The
paranoid kernel check is separate. It requires `bubblewrap` on `PATH` and
enabled user namespaces or a working setuid installation, and checks a
temporary workspace under `.work/tmp/`.
