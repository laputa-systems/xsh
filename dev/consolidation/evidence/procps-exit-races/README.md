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

Proposed repair `3646630bb5d6e0f406b63676cf828091be6dd9b3` gives parsing three
outcomes: exited, valid, and malformed. Exact state X is exited before validating
live numeric fields. Sampling skips exited records; malformed live records
continue to fail. The captured-row regression and live negative/valid controls
have [source-review GO](review-dead-stat.md). Execution verification is pending.
