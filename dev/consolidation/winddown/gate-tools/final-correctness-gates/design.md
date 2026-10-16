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

The product command names the four packages and uses Cargo's single `--bins`
selector. It builds the same five required executables: `xsh`, `xsht`, `xshi`,
`xsh-fuzz`, and `xsh-test-helper`.

The final runner mounts the frozen source tree read-only in every mode. Cargo
targets are mounted separately, explicitly read-write for build, lint, and
cache preparation and read-only for runtime execution. Each mode gets an
explicitly read-write host-backed `.work/tmp` mount and sets `TMPDIR` to that
path. Target paths must stay within the gate's ignored outputs and may not pass
through symlinks. The runner checks the repository's `.work` exclusion before
creating the target directory.

## Output streams and attempt records

`Launch.run()` writes each child stream to its own file under the attempt's
`logs` directory. Each step records `stdout_log`, `stderr_log`,
`stdout_sha256`, and `stderr_sha256`. The combined `.log` keeps the command
metadata and labelled copies for human review. It is not an output source.
Cargo artifact parsers read the stdout file. The pinned-image check requires
exit code 0 and stdout bytes exactly equal to the pinned image ID followed by
one newline. Stderr stays available as a diagnostic and does not change the
image check.

The receipt keeps schema version 1 and adds those four fields to new step
records. Existing receipts remain unchanged and are not backfilled. Each retry
uses a new attempt directory, so a failed receipt stays distinct from a later
attempt.

## Source and tool admission

Execution requires `--source-root` with an absolute canonical checkout path.
The accepted root is the controller checkout or a direct worktree under its
`.work/wt` directory. The launcher confirms that the selected path is the Git
repository root and that its common Git directory matches the controller's.
It checks the requested commit, tree, and clean status in that selected source
checkout before inspecting the pinned image or starting a build. The runner
uses that same root as its working directory and read-only source mount.
Supplemental contexts keep the existing `/tmp` tmpfs and mount the selected
source checkout's ignored `.work/tmp` separately for disk-backed `TMPDIR`.

The controller root continues to own the Cargo and native-suite locks, pinned
image and tool inputs, corpus mount, and ignored output directory. Launcher,
plan, runner, and helper files come from the tracked gate-tools directory
containing the invoked launcher. The launcher keeps that tool checkout
independent from `--source-root`; both must share the controller Git
directory. Their paths, expected tool commit and tree, and the selected source
Git directory are bound in the receipt separately from the output path. Both
source and tool snapshots are checked before and after external commands. The
generated manifest stays in the controller's ignored final-review output
directory, and attempts remain direct children of that directory. This keeps
infrastructure controls and receipts stable while verifying a frozen linked
worktree without consulting the controller checkout's source status.

The `correctness-be0b-20261016` attempt failed preflight because its generated
`container_cpus` value was the string `"12"`, while the launcher requires the
integer `12`. The `correctness-be0b-20261016-r2` attempt failed the image check
after Docker returned the expected image ID. The combined log placed buffered
command metadata after child output, so the last-line check read metadata
instead of the ID. Both attempts remain failures. The output-stream repair
does not change their approved command selection or resource limits: 12 CPUs,
24 GiB of memory, and no swap.

The `correctness-be0b-20261016-r3` attempt failed Cargo argument parsing before
compilation because the product command repeated `--bins`. The replacement uses
one package-wide selector. Pinned Cargo parser probes for the product, all three
harness, and Clippy commands reached the intentionally missing manifest error
without invoking compilation.

## Approval and execution

The owner approved the replacement product and warning-deny matrices and the
five-context plan. Prepare the ignored runtime manifest from the tracked
generator and run the gate only against an exact clean source tree supplied
through `--source-root`. This approval does not recover the original manifest
or claim an acceptance result. No bootstrap-product receipt or runtime
manifest was created by the initial gate-input repair.
