# XSH formal project

This maintenance project uses Lean 4.35.0, pinned in [`lean-toolchain`](lean-toolchain).
Cargo builds and XSH products do not depend on Lean. [`Xsh.lean`](Xsh.lean)
imports mathematical models of selected XSH behavior. A `SPEC_PROVED` theorem
proves a property of its Lean definitions; it does not establish correspondence
with XSH source, indexed IR, Rust runtime, or host behavior.

[`coverage.json`](coverage.json) is the authority for coverage. It records
the frozen source commit and tree. A universal obligation is planned or
closed. A row closes only when its theorem covers the whole frozen core for
that obligation; it then names a public theorem in a module reachable from
`Xsh` that is not a fragment root, and the axiom audit rejects a closed root
that is absent, not a theorem, or outside the project modules. No row is closed
yet. The finite Rust-conformance ledger holds only the named tests that ran
against binaries built from that commit. Each case is evidence for its own test
only. The `artifact_certificates` list holds `ARTIFACT_CERTIFIED` records, one
per kernel-checked certificate: the artifact kind, the Lean definitions that
hold the artifact's bytes, the certificate theorem, its supporting and
rejection theorems, the proposition, and what it does not show. A certificate
proves a closed proposition about those bytes; it is not a fragment root and
closes no universal obligation. The fixed source/IR corpus, semantic scenario
suite, and replay runner have not been introduced. The ledger's bridge cases are
the fourteen named arithmetic and facts tests; other private bridge fixtures
are not conformance cases.

The experimental foreign-IR bridge also decodes candidate instruction rows,
binary-operation tables, locations, and string pools before passing a
reconstructed program to `FullVerifier`. Other semantic tables still come from
an in-process program. Source-to-IR validation and portable symbol ownership
remain open, so this bridge is not a certificate for foreign programs. The
Lean raw-row reader checks tag names against their ordinals; it does not yet
relate the decoded rows to source programs or executions.

The connected model represents nested value blocks with a Unit value and a
separate function-return context. Its contextual validators prove source and
node typing for checked blocks, including qualified instances. Block scope
restoration preserves writes to outer cells. Mutable bindings retain their
selected storage type, and assignments use structural compatibility while
preserving the stored value. The model checks qualified storage requirements
and relates them to runtime compatibility; source-derived unique rows and
runtime-relevant call-instance closure remain open. Source admission, lexical
effect bounds, translation validation, and whole-machine correspondence remain open.
The connected machine has conditional initialization, progress, step and
execution preservation proofs. Their premises include grounded function bodies,
storage and function-table certificates; deriving those premises from admitted
source and published Rust facts remains open in general. For the scoped
mutable-local fixture, its actual published Int call grounds the checked body
and closes its storage tag; full machine initialization remains open.
A scoped fact certificate checks one Rust-exported generic mutable-local example
inside `without`, including source sites, storage requirements, call bindings,
and ten altered decoded facts. Its call check currently supports direct,
unshadowed top-level calls in equality assertions with literal arguments;
other call sites are reported as unsupported. Other effect regions and full
source-to-IR correspondence remain open; the older fact validator still reports
regions as unsupported.

Every claim here starts after parsing unless planned source-admission evidence
covers the source bytes. Rust is the only implementation compared so far. The
plan states the contract a second implementation must meet; no such
implementation or evidence about one exists.

[`check-inventory.py`](scripts/check-inventory.py) and its
[tests](tests/inventory/test_check_inventory.py) validate inventory structure,
owner coverage, root reachability, and evidence records. They do not test Lean
theorem behavior or Rust conformance. [`xsh-lean.md`](../xsh-lean.md) is the
owner-managed plan and is parsed by the inventory checker; preserve its
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
temporary workspace under `.work/tmp/`. Its sandbox passes the Lake thread
limit to con-leche's `--jobs` option as well as to Lean.
