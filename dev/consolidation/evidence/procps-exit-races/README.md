# Process sampling exit races

The canonical ed908 procps failure led to two distinct bounded reproducers.

## Held stat descriptor

With eight short-lived process workers, iostat returns status 3 and empty stdout;
stderr reports `linux-sample: No such process (os error 3)`. A deterministic
fixture holds a proc stat descriptor until its task is killed and reaped.
Test-only source `e0824bc720e8c5b920a2a89b4b31a96f0a75148c` reproduces the failure.
Repair `ba14ae8e41251f6e4d8092773a992c435141d86d` passes three focused tests:
held-descriptor ESRCH, narrow disappearance classification, and unexpected EISDIR
propagation. The raw red and green outputs remain separate.

The repair skips only NotFound and ESRCH at the affected disappearance boundaries;
the stat-read PermissionDenied policy is unchanged. Other I/O failures still
propagate. Main integrates this repair at `204467038`.

## Readable dead task

The updated products on clean `449320b1844cb5c7cc361df2fe27e387b37b12d1` still fail
the bounded iostat probe at sample 12: status 3, empty stdout, and malformed stat
for pid 4802. That failed run is retained in `post-esrch-449`.

A separate read-only Python probe reproduces the parser's field selection and
captures an exact 133-byte stat row for pid 28197. State is X and pgrp/session
are both -1; immediate directory and status observations report ENOENT. This
probe is not instrumented XSH execution. Its source, limits, raw bytes, and
observations are in `dead-stat-observation`.

Repair `3646630bb5d6e0f406b63676cf828091be6dd9b3` gives parsing three
outcomes: exited, valid, and malformed. Exact state X is exited before validating
live numeric fields. Sampling skips exited records; malformed live records
continue to fail. The captured-row regression and live negative/valid controls
have [source-review GO](review-dead-stat.md). Test-only source `d4e9ddd2d` fails
the exact X case with malformed stat while both live controls pass. The green
six-test module passes at `3646630b`; source and harness hashes stay unchanged
through runtime. Main integrates the reviewed fix at `787ba37a4`.

`dead-stat-redgreen` retains both runs and their rejected setup attempts. The
first red harness argv requested JSON without its required unstable-options
flag; the corrected invocation runs the tests. The first green Cargo invocation
incorrectly reuses the red harness because the older green worktree files share
the same container path and have older timestamps. That artifact is not credited.
Advancing only the green process source mtime forces actual recompilation:
Cargo reports `fresh: false` and the harness digest changes before the six passes.
Source bytes and Git tree remain unchanged. Runtime has no allocator preload.

## Integrated products at 787

Matching verification products build successfully on clean main `787ba37a4`,
tree `31681c63f02b825b4352017482230a33e78a819d`. The finite integration probe
completes all 500 iostat calls with eight churn workers under its 22-second cap;
every completed call exits zero and has the expected header. Written and freshly
desugared procps each pass all 39 tests. All sixteen desugar steps exit zero.
The exact native malformed-counter rejection case also passes. Source and product
identities match before and after; the pinned container has two CPUs, 4 GiB,
no swap, no network, and no runtime preload. Raw results and transformed inputs
are retained in `integration-787/attempt3`.

The earlier integration attempt reaches its cap after 866 completed calls. Its
last in-flight child is interrupted and returns status 3; the wrapper rejects
that cancellation status. This is inconclusive, not an ordinary iostat failure
or a passing probe. `integration-787/attempt2` retains it. The fresh finite probe
completes normally instead of relaxing the allowed cancellation statuses.
