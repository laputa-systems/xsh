# Performance adapter reference

`check.py` dispatches to existing measurement and acceptance owners. Python 3.11
or newer is required. It adds no production dependency. The planned `baseline`, `inference`, `measure`, `optimize`, `deployment`, and
`close` modes remain unavailable and exit nonzero. Phase attribution is not
implemented.

Run policy controls without Docker:

```sh
python3 -B dev/performance/check.py --config CONFIG self-test
```

Add `--adapter` to qualify existing release products and execute one exact-output
control in the pinned offline Linux image. This also checks rejection of wrong
output and a real malformed cleanup cidfile. It does not measure performance.
Readiness controls exercise before and after failures without a real clock.

## Configuration

The JSON configuration requires exactly these fields. All paths are explicit,
absolute, and free of symlinks. Unknown and duplicate fields are rejected.

| Field | Value |
|---|---|
| `schema` | Integer `1` |
| `source_root` | Clean XSH Git checkout root |
| `commit` | Exact 40-character lowercase source commit |
| `tree` | Exact 40-character lowercase source tree |
| `laputa_root` | Explicit isolated Laputa checkout containing `pm/` |
| `product_receipt` | Actual `canonical_gate_batch_replay` receipt, or completed `profile-cost.py` receipt |
| `xsh`, `xsht` | Exact product paths bound by the selected existing owner |
| `scratch` | Existing directory under the adapter checkout's `.work/tmp/` |
| `functional` | `null`, or the functional input object below |

Product qualification checks the clean commit and tree, executable paths and
hashes, successful owner result, and resolved cleanup. The profile provider
requires opt3. The canonical provider verifies bootstrap, resource and log
bindings before qualifying verification products. It issues no new product receipt. An exact-source checkout can differ from the receipt's original
build location. There is no normal-checkout fallback.

The functional object requires exactly `canonical`, `supplemental`, `skip_review`,
`structural`, `review_evidence`, and `output`. Each value is an absolute path.
The output must be under the source checkout's `.work/` and must not exist.
Functional validation requires `product_receipt` to equal `canonical`; it does
not require a prior release profile or functional-close record.

## Functional validation

```sh
python3 -B dev/performance/check.py --config CONFIG functional --check
```

The command validates the actual `final_canonical_replay.py` receipt, its pinned
command and image, resource record, bootstrap build, product files, logs, source
checks, and cleanup. It validates required product builds, harness builds, warning-deny commands,
all five current Linux contexts, product stability, harness stability, and log bindings.

The command runs `metrics.xsh check-current --review-evidence` in the pinned image.
The existing metrics owner measures current source, validates the structural
report, and checks all nine functional semantic review rows. The report retains
all ten canonical rows. Functional evidence excludes exactly `timings`; final
review still requires `timings`. Neither a bare boolean nor launcher success can
close functional acceptance.

The adapter's `skip_review` input records a human review against exact receipts.
It requires exactly these fields:

| Field | Value |
|---|---|
| `source_commit`, `source_tree` | Exact configured source identities |
| `canonical_sha256`, `supplemental_sha256` | Hashes of the reviewed receipts |
| `reviewer` | Nonempty reviewer identity |
| `contexts` | Object containing `canonical` and every supplemental `linux-special-*` step |

Each context requires exactly `result: "review_verified"` and a nonempty `reason`
that states the skip disposition. Review every mandatory skip in the bound logs;
an unresolved mandatory skip prevents a verified context. This input records a
review, not an automatically established privilege result.

After all checks pass, the command publishes the existing functional-close shape
with the exact commit, tree, reviewer, products, scope, and evidence bindings.
Publication refuses overwrites and exposes only a complete JSON record. It
rechecks source and products before publication. No full successful close has
been established by the adapter foundation tests.

## Canonical bootstrap ownership

`final_canonical_replay.py::validate_bootstrap_receipt` owns the bootstrap schema.
Its `BOOTSTRAP_CARGO_ARGV` requires a verification `xsh` build with
`--message-format=json-render-diagnostics`. Its Cargo artifact record must name
`/work/target/x86_64-unknown-linux-musl/verification/xsh`. The receipt binds the
clean source, pinned image, exact Rust flags, Cargo command, successful Cargo JSON
stream, and executable bytes. The JSON stream and receipt live under the selected
source checkout's `.work/tmp/`.

The supplemental launcher's Cargo JSON product build and `parse_binary_artifacts`
are the existing producer boundary. Its broader product command and target mount
do not satisfy the canonical bootstrap contract. The acceptance owner must adapt
that producer to the canonical contract before supplying a fresh canonical run.
This adapter neither builds a second product schema nor invents bootstrap credit.

## Limits

Policy tests cover config rejection, product identity, output comparison, failed
builds, cleanup, readiness ordering, canonical rejection, and required supplemental
contexts. They do not replace full acceptance, resource qualification, workload
measurement, phase-observer validation, or final timing gates.

Unresolved owned-container cleanup retains its cidfile under `scratch` and fails.
The adapter uses the existing owned-container cleanup implementation. It does not
create a new container supervisor or benchmark runner.
