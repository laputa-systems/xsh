# Timeout cleanup tests wait for the child state

The two cleanup tests used a 50 ms timeout that started before the XSH child
script ran. The child could time out before it registered `defer`. In the
stdin test, the producer then waited for a cleanup marker that could never
appear, so the outer 2 s watchdog reported `timeout: process timed out`.

The controlled startup-delay run inserted a 150 ms shell delay before the
child started and kept the 50 ms timeout. It returned status 124 after 403 ms
with `ready=false`, `blocked=false`, and `cleanup=false`. This reproduces the
startup race. It is consistent with the original profile failure, but does not
prove that run's exact scheduling path. The earlier 81 ms pass is retained as
a non-reproduction and receives no credit.

The repaired tests give timeout a 10 s utility deadline and keep the existing
2 s outer watchdog. Each child writes `ready` after registering its deferred
cleanup. The CPU-bound child then spins. The stdin child reads `x`, writes a
`blocked` marker immediately before `io.stdin_bytes()`, and waits for more
input. The marker confirms the script reached that call site. It does not
prove an OS-level blocked-read state.

The stdin producer runs under its own process handle and writes through a
named FIFO outside timeout's managed child group. It keeps the FIFO writer
open until the child writes its cleanup marker. Before TERM, the test checks
that cleanup is absent and the writer is still running. EOF therefore cannot
release the child before cleanup. The test sends TERM only to the timeout
wrapper PID and asserts status 143, cleanup, empty output, and producer exit.
The writer has bounded 6 s readiness and cleanup waits. The test owns and
cancels it on every failure path under the shared 2 s budget.

The source change is limited to `core/tests/test-timeout.xsh`. The selected
signal test still verifies deadline statuses 124, 138, and 137. The existing
INT and TERM forwarding test also passes. No production timeout code changed.

## Focused verification

The four exact selectors each passed with 1 test, 0 failures, and 0 skips:

- CPU-bound cleanup: 92 ms.
- Blocked stdin cleanup: 77 ms.
- Selected signal and deadline statuses: 213 ms.
- INT and TERM forwarding: 153 ms.

The runs used the protected C release binaries, pinned Linux image, four CPUs
on CPUs 28 through 31, 8 GiB memory with no extra swap, and native lock inode
62012470. The runner checks product hashes before tests. The retained hash file
records the source and protected products after tests. The container was
removed and the native lock was available after the run. The run did not build
Rust binaries or run the full native suite.

`focused-green/` contains the commands, selector output, container status,
product hashes, and cleanup checks. `attempts/controlled-delay-red/` contains
the startup-race experiment. Other attempt directories retain the 81 ms
non-reproduction, initial checker errors, BusyBox checksum-flag preflight, and
stale Docker cidfile refusal. Those failed attempts receive no test credit.
