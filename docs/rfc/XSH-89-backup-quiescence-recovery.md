# XSH-89: Backup quiescence recovery

Status: draft

## Summary

Make supervised backup execution retain responsibility for restoring quiesced
services after the backup child is lost. Record the services the attempt owns
before stopping them, reconcile that record when the child exits or supervision
restarts, and restore only the prior desired states still owned by that attempt.

Recommend extending the existing timer supervision path for the maintained
backup job. Route manual backups that request quiescence through the same owner.
Implementation belongs in `../laputa/`; [XSH-89](../../TODO.md) tracks it.

## Motivation

The [backup runner][runner] attempts its release hook after ordinary caught
errors. The [quiesce hook][hook] changes xinit desired state through stop/start
commands, and the [maintained configuration][config] names `logger` and
`dhcp-eth0`. Release currently depends on the backup process reaching its
epilogue. A kill or crash after stopping those services can leave them down.
The [timer timeout path][timeout] can terminate that process; operator
cancellation also stops further retries.

A later successful backup, reboot, or operator start may restore service. The
missing guarantee is recovery after a lost attempt, within a stated bound and
without starting a service that was already stopped. The source trace supports
this failure path; a current-source interruption reproduction is unproven.

## Design

### Keep recovery outside the backup child

Let the timer supervisor own the quiescence record and its recovery. The backup
child performs the snapshot work and reports normal completion; it is not the
only process able to release the stops. The recovery owner must be outside the
process group that timer cancellation terminates.

Before admitting a quiescing attempt, persist a versioned record containing its
invocation identity, the affected services, their prior desired states, the
stop operations this attempt owns, and its recovery budget. Write and sync the
record before changing service state. Existing atomic file and confined-root
operations should carry this state; no separate persistence framework is needed.

Only services whose prior desired state was up are acquired. Record enough
per-service progress to recover from a partial quiesce. Recovery queries whether
an attempted stop took effect even if the child or supervisor died before
recording its acknowledgement.

### Preserve control ownership

Associate each acquired stop with the attempt and a control revision. A later
explicit operator action supersedes that ownership. Recovery may restore up
only while the stop still belongs to this attempt; seeing `desired=down` alone
is insufficient evidence. If the current xinit protocol cannot expose that
distinction, add the smallest ownership/revision field to its existing control
path before enabling automatic restoration.

Services already down remain down. Successful release is idempotent, and the
record is retired only after every acquired stop is restored or explicitly
superseded. A failed release stays visible and eligible for bounded retry.

### Reconcile at termination and startup

On success, failure, timeout, or cancellation, first establish that the snapshot
writer has stopped, then release its owned quiescence. Keep process identity
checks so a recycled PID cannot stand in for the original writer. If the writer
cannot be stopped, report that condition instead of declaring recovery complete.

On supervisor startup, reconcile unfinished records before admitting another
quiescing backup. Store a boot identity with any boot-clock deadline; a deadline
from a different boot requires reconciliation, not numeric comparison. Do not
persist a process-local monotonic instant as a restart-stable timestamp.

Manual invocations that use service quiescence must use this supervised path or
provide the same independently owned recovery. The maintained job's snapshot,
retention, key, and restore-receipt formats remain the existing owners' concern.

### Acceptance

In a disposable maintained guest, block the backup child immediately after
successful quiescence. Exercise timeout, operator cancellation, SIGKILL, and
supervisor restart. Observe restoration within the configured recovery bound
and prove that cancellation starts no new backup attempt. Repeat with a partial
quiesce and a release failure.

Keep a previously stopped service down throughout. Issue an operator stop after
acquisition and verify that recovery respects it. Repeat recovery and verify no
extra transitions. Interrupt each durable-state transition to establish that a
stop cannot outlive both its child and its recoverable ownership record. Retain
the existing normal backup and restore drill.

## Alternatives considered

- **Signal handler or defer in the runner:** improves cooperative cancellation
  but cannot recover after SIGKILL or a process crash. Keep it as a fast normal
  cleanup path, without treating it as the sole owner.
- **Supervised watchdog per backup:** can survive the child if separately owned,
  but duplicates lifecycle machinery already present in the timer service.
- **Expiring xinit stop lease:** puts recovery beside desired state and also
  serves unscheduled callers. It expands xinit's control protocol and timer
  policy; prefer it if the timer owner cannot cover the required manual path
  without duplicating recovery logic.

## Open questions

1. **Recovery owner:** extend timer supervision or add an expiring stop lease to
   xinit. Recommend the timer owner for the shipped backup job, with a narrow
   xinit ownership revision so later operator commands win.
2. **Manual invocation:** route quiescing manual runs through the same supervisor
   or implement the xinit lease alternative. Recommend one supervised path.
3. **Recovery budget:** choose the stop/release deadlines and bounded retry
   policy. Recommend publishing the bound with the backup configuration and
   verifying it independently of the one-hour snapshot timeout.

## Decision

Pending. Prefer recovery in existing timer supervision, durable ownership of
each acquired stop, and reconciliation after both child loss and supervisor
restart. Do not claim interruption safety until the guest acceptance cases pass.

[runner]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/system/backup.xsh#L538-L579
[hook]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/system/backup_quiesce.xsh#L131-L153
[config]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/profiles/qemu-maintained/etc/laputa/backup.json
[timeout]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/system/timers.xsh#L1423-L1425
