# Canonical batch on 787

The pinned Linux musl batch ran on clean `787ba37a4`, tree `31681c63f0`.
The source checks before and after match. The batch exits 3.

The completed stages pass 305 process tests, 747 library tests, 116 XSHT tests,
14 soundness tests, and the privileged groups of 82 and 10 tests.
The XSHT stage includes the complete written and desugared comparison.
The native suite finishes with 13,720 passed, 160 failed, and 92 skipped.
Repository check, lint, structural checks, and docs do not run after that failure.
No timing credit or full acceptance is claimed.

The failure list and diagnostics are in [stdout.log](stdout.log).
The [outer receipt](canonical-replay.json) binds the source and launch.
The batch runs native tests as root in a privileged container.
A separate exact `rm` reproduction fails as root and passes as UID 1000.
That result explains one permission failure. Investigation of the remaining
permission, device, and network failures continues.

The run finished before the reboot. Its retained receipt and logs are complete;
the reboot does not turn the failed native suite into an interrupted result.
