# Final performance observer launch plan

## Frozen source and platform

Run from the clean main checkout at `/home/josh/d/laputa-systems/xsh`.
The frozen source is commit `7b1485310abdf2190ce4532d840114e74ff7c2cd`
with tree `0f3394902504cbe5271edd8916f504bd493f091a`. Confirm both values and a
clean tracked and untracked status immediately before each gate. Use the pinned
Linux x86_64 musl image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.

This observer batch is mandatory in addition to the nine required serial
correctness suites: canonical normal, selected release opt3 correctness,
forced-frame, and three reference/candidate scheduler pairs. It does not
replace any of those runs.

## Inputs that must come from completed gates

Use an actual functional-close record whose `functional_complete` field is
`true` and whose `commit` equals the frozen commit. Do not create a placeholder
record.

Use a completed profile-cost receipt from this exact checkout and source
identity. It must select only `release-opt3`, with release opt level 3, thin
LTO, and 16 codegen units. Its `xsh`, `xsht`, and `xsh-test-helper` binaries
must be executable, share one release directory, and match the receipt hashes.
The profile-cost run also requires a ready host for its timed build and native
suite stages.

Prepare the final-correctness launcher inputs at
`.work/final-review-evidence/final-correctness-gates` from their exact reviewed
sources. The approved `manifest.json` must describe five supplemental Linux
contexts, include the recursive privileged cp block-device filter exactly
once, omit broad Rust/native loops, and include its required product builds,
harness builds, and warning-deny commands. Keep the tracked launcher, bounded
runner, nested Cargo forwarder, Rust wrapper, and cleanup self-test byte-identical
to their reviewed source pins. Supply the actual frozen baseline binaries and
toolchain prerequisites required by that manifest. The launcher requires host
UID/GID 1000 and writes each execution receipt to a new direct child of its
correctness-gates directory.

The correctness launcher is a separate post-batch step. After the observer
batch succeeds, run the launcher with the same commit and tree. The observer's
plan records the exact handoff command and counts; it does not execute that
supplemental launcher itself.

## Artifact pins

Keep the five copied observer tools and this `launch-plan.md` in
`.work/final-review-evidence/final-performance-gates`. `final-artifact-manifest.json`
must name exactly these six basenames under
`performance_observers.files`, with each value equal to the file's SHA-256.
`final-artifact-manifest.json.sha256` must contain the SHA-256 of the manifest
followed by two spaces and `final-artifact-manifest.json`. Preserve the
observer's runtime checks; do not loosen its required file set or hash checks.

## Launch order

First run the selected profile-cost gate with the completed close record and a
new output under `.work/consolidation`:

```sh
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/profile-cost.py \
  --tree /home/josh/d/laputa-systems/xsh \
  --commit 7b1485310abdf2190ce4532d840114e74ff7c2cd \
  --functional-close "$FUNCTIONAL_CLOSE" \
  --output "$PROFILE_OUTPUT"
```

Once the real profile-cost receipt and all correctness launcher inputs exist,
run the observer with a new output directory directly under
`.work/final-review-evidence/final-performance-gates`:

```sh
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py \
  --execute \
  --tree /home/josh/d/laputa-systems/xsh \
  --commit 7b1485310abdf2190ce4532d840114e74ff7c2cd \
  --functional-close "$FUNCTIONAL_CLOSE" \
  --profile-receipt "$PROFILE_RECEIPT" \
  --release-opt-level 3 \
  --output "$OBSERVER_OUTPUT"
```

The observer runs isolated repository lint followed by one canonical
`cargo dev gate --batch`. It holds the profile, native-suite, and Cargo locks,
uses the pinned Docker image, records container RSS and cgroup memory data, and
checks the selected release binaries before and after both stages. Run no other
native suite concurrently.

## Timing and resource policy

A successful `perf-gate-ready` before a stage permits timing. Accept its timing
only when the immediate `perf-gate-ready -1` after that measured stage also
succeeds. If readiness fails before the stage, still run the correctness and
resource observation, record memory and binary evidence, and leave timing absent
or discarded. Do not run the post-stage readiness probe for a stage that was
not timed. If readiness fails after a timed stage, discard that timing. Record
`perf gate deferred: host busy` when the host remains busy; that is not a timing
pass. Memory and binary-size observations remain required in either case.
