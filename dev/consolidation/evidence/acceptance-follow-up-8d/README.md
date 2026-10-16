# Acceptance follow-up on 8d

Main is frozen at source commit
`8d0d6362418a6ab1b58582cf5574a7e0f4ac3542`, tree
`08769a1d438d5d93f55bb21e7a4dbe8051f7b966`. The source is clean. At handoff
refresh, container `xsh-canonical-canonical-8d0d63624` reports btop 58 passed,
three skipped, and root stage 223 in progress. No terminal canonical result is
available.

The 8d bootstrap built `xsh` prefix `d058` from the same Rust source. The
tracked driver checks resource and command bounds, swap, cleanup, read-only
source, and the shared lock. This qualification belongs to the running batch;
it does not supply a terminal suite result.

Integrated focused results include queue `b6` (11 tests and a four-file
check/lint); lint changes `a22` (1,526 whole-check passes, zero findings and 113
notes, metrics check, three docs tests); the SGID private-scratch repair (288
Python scripts); and lint cleanup `f958` (zero findings and notes, three
negative preflight cases). The `a22` check results are not qualified against an
unchanged shared product: bootstrap changed `xsh` from prefix `a911` to `d058`.
A stable private-copy rerun is active.

The 8d driver fix passes 22 tests as root. The earlier `9a` replay failed with
`NameError` before Docker, so no suite ran in that attempt. No release run,
performance timing, functional close, or final canonical result is claimed.
