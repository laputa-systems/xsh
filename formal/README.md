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

The shared data foundation in [`Core/Types.lean`](Xsh/Core/Types.lean) and
[`Core/Values.lean`](Xsh/Core/Values.lean) represents the non-recovery type
families and raw value candidates. It reuses the existing scalar and integer
validation domains. Its kinded substitutions preserve callable metadata and
module contracts. Values retain raw Float bits, typed semantic references,
recursive error causes and nominal identity. The data laws do not establish
admissibility, complete type membership, execution safety or Rust conformance;
the fragment machines have not yet migrated to this foundation.

[`Core/Membership.lean`](Xsh/Core/Membership.lean) defines membership for the
represented value-type families, with row, export and union judgments. It
retains the environment-path list view, exact and open exports, and semantic
nominal provenance. [`Core/MembershipDecision.lean`](Xsh/Core/MembershipDecision.lean)
constructs a terminating decision and proves its result agrees with those
judgments. Retained stream and callable contracts are explicit predicates with
caller-supplied decisions. The model controls cover acceptance and rejection;
they are not Rust conformance evidence. Type admissibility, valid semantic
worlds, establishing and preserving the retained contracts, and integration
with source and execution remain open. TYPE-MEMBER is still planned.

[`Core/SymbolicEffects.lean`](Xsh/Core/SymbolicEffects.lean) represents finite
symbolic effect expressions with an arbitrary number of parameters and a mask
for each parameter. Exact inclusion and permission coverage are proved
equivalent to their judgments for every assignment. Only direct permissions
receive `io` closure; variable masks retain exact membership. Substitution agrees
with replacing assignments and preserves both relations for known replacements.
Unknown contracts propagate through each live substituted dependency and remain
distinct from a known empty contract. An erased error-only dependency gives a
counterexample to unconditional capture/substitution commutation with unknown
replacements. These are laws of the model: a false comparison witnesses an
assignment that fails, not an immediate Rust predicate rejection. Source
extraction, the Rust three-valued predicate decision, source effect instantiation,
inference integration and execution correspondence remain open. All universal
effect obligations remain planned.

Shared type terms now carry an effect-parameter scope separately from type
variables. Callable policies retain symbolic effect bounds and keep pure,
procedure and producer kinds distinct. Type substitution preserves that scope;
effect substitution can change it. One traversal implements both operations.
Their general commutation law transforms the type replacements when applying
the effect substitution. Known effect substitution agrees with interpreting
the bound under the substituted assignment; unknown replacements propagate
through live dependencies. The existing membership decision uses zero effect
parameters and no type variables. Model controls check nested callable policies,
defaulted labels, error capture and unknown replacements. Type admissibility,
source binding, retained-contract authentication and machine integration remain
open. These data and substitution laws close no full-core obligation.

The experimental foreign-IR bridge also decodes candidate instruction rows,
binary-operation tables, locations, and string pools before passing a
reconstructed program to `FullVerifier`. Other semantic tables still come from
an in-process program. Source-to-IR validation and portable symbol ownership
remain open, so this bridge is not a certificate for foreign programs. The
instruction exchange also retains assignment operations and decodes assignment
path steps. Lean relates an assignment's decoded StoreValue ordinal and source
statement site to scoped checker facts.
An extended comparison reconstructs the replacement expression from raw rows
and from the checked node. It supports scalar literals, reads, binary operations,
records, projection, and conditional expressions, and value blocks that bind
temporaries outside the mapped slots before one value, as scheme lowering emits
for assignment right-hand sides. It rejects different same-type reads and
integer payloads. Its general theorem composes this comparison with
the scoped store typing theorem for every sorted solution. Name spellings come
from the admitted source's name table. The binder-to-slot map is derived rather
than supplied: parameters take their positional slots, and each source local
takes the slot of the StmtLet row that a test-only Rust export claims for its
declaration site. Lean accepts a claim only when the row lies in the function's
own statement blocks, its exported fact has the local's mutability (a user `var`
is thus told from a compiler temporary), the initializer resolved through the
function's single-assignment temporaries equals the source initializer, and
binders and slots are injective. Corrupting a claim, a fact, a row slot, or a
store target is rejected for the exported scoped body, and swapped claims for the
exported two-local body. The claim table, parameter order, fact kinds, and the
invariant that slots are never reused remain assumptions of the Rust lowerer;
locals with equal initializers are told apart only by the uses a check covers.
Calls, Result forms, annotated or destructuring declarations, other value blocks,
the older projected-row fixtures that still pass a map, and operational
correspondence remain open.
Assignment transfer exports retain the checker’s optional value-type override.
For the scoped fixture, Lean relates the emitted assignment fact to the outer
binding and checked projected row; altered transfer facts are rejected.
Resource-marked candidates remain unsupported by the Rust decoder; complete
source-to-IR and execution correspondence remain open.

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

[`Core/ContractSubstitution.lean`](Xsh/Core/ContractSubstitution.lean) characterizes
substitution of known and unknown effect contracts across arbitrary parameter
scopes. Given an initial exact inclusion or permission-coverage judgment, a
substitution preserves that judgment exactly when a resulting unknown contract
on the left also makes the right unknown. For known source expressions,
preservation under every substitution is equivalent to inclusion of their live
parameter dependencies. A parameter hidden by direct effects can therefore
break an otherwise valid inclusion when replaced by an unknown contract.
Source typing must establish the condition before using a substitution law.
Restricted-call permission keeps its separate known-bound rule.
The proof covers the shared effect domain, not source admission or Rust
conformance. EFF-LATTICE and all other universal obligations remain planned.

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
/bin/bash scripts/audit-axioms.sh
/bin/bash scripts/audit-axioms.sh --self-test
python3 scripts/check-fresh.py
python3 -B tests/test_check_fresh.py
```

The audit rejects unsafe declarations, project axioms, `native_decide`, and
`sorry`; its permitted dependency list is defined in the linked script. The
paranoid kernel check is separate. It requires `bubblewrap` on `PATH` and
enabled user namespaces or a working setuid installation, and checks a
temporary workspace under `.work/tmp/`. Its sandbox passes the Lake thread
limit to con-leche's `--jobs` option as well as to Lean. Con-ron uses one worker
to fit its per-worker stack and scratch storage alongside the complete export
within the same 8 GiB address-space limit.
The fresh build limits [glibc malloc arenas](https://sourceware.org/glibc/manual/latest/html_node/Malloc-Tunable-Parameters.html)
to the configured worker count. The official prebuilt compiler otherwise fails
to create a thread under that limit on the qualification host. The same module
compiles with bounded arenas and the unchanged address-space cap.
If a build or checker exceeds its deadline, the command prints its captured
progress and terminates the owned process group. A timeout does not qualify a check.
