# Final correctness gate input repair

The owner approved this replacement plan on 2026-10-16. The ignored historical
manifest was not recovered, and this lane has not executed the gate.

## Missing manifest

The launcher expected an ignored `manifest.json` pinned by its body hash, but
neither that file nor a tracked source for its original plan is available. Its
original product and warning-deny arrays are also absent. The new tracked
`plan_source.py` defines the approved replacement plan as a new authority. It
preserves the five Linux contexts and three harness builds asserted by
`test_launcher_cleanup.py`; it does not claim to reconstruct the missing
product or warning-deny arrays. The replacement product commands and
workspace-wide all-target, all-feature Clippy command are derived from the
current workflow contract and were approved by the owner. Workspace scope
covers the campaign's XSHT and XSHI changes. The plan is executable only
against the supplied clean commit and tree.

Each harness build qualifies its root `xsh` binary from Cargo's raw artifact
and completion records. The receipt binds the root manifest, non-test binary
target, isolated target/profile path, actual feature set, executable hash,
successful build, and clean before/after source checks. Cargo's `fresh` reuse
is accepted only after that same variant and hash were qualified earlier in
the run. The Linux-privileged feature build is a distinct variant and may have
a different hash from the default verification product.

`generate_manifest.py` deterministically renders the tracked plan and binds its
source, launcher, runner, cleanup test, and generator with one-way hashes. The
runtime manifest remains ignored state generated for the approved source tree.

## Candidate products replace the fallback

The old launcher required three ignored `baseline-bin` hashes. In this gate
they served only as fallback `PATH` entries. The first operation is a direct
Cargo build of the required verification products, and the launcher fails if
any required executable is absent afterward. Harness, Clippy, and native test
commands do not need an interpreter before that build completes. Runtime
native invocations select the newly built verification `xsh` and `xsht`
explicitly; the five supplemental contexts use the built harnesses or
verification `xsht` and set `XSH_BIN` to the candidate. The gate-local fallback
mount and hash pins can therefore be removed without adding a bootstrap build
or accepting unqualified artifacts.

This scope does not change `infra/run-native.sh` or the separate profile-cost
tool. The former belongs to the full-suite runner. The latter owns release
product verification; this plan builds only the verification products its
correctness contexts consume.

## Approval and execution

The owner approved the replacement product and warning-deny matrices and the
five-context plan. Prepare the ignored runtime manifest from the tracked
generator and run the gate only against an exact clean source tree. This
approval does not recover the original manifest or claim an acceptance result.
No bootstrap-product receipt or runtime manifest was created by this change.
