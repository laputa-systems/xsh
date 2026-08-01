# XSH OS Runtime Specification

`examples/processes.xsh` is the curated process and host-state composition
showcase. Focused process, system, user, and group coverage lives in
`tests/xsh/stdlib/process.xsh`, `tests/xsh/stdlib/system.xsh`,
`tests/xsh/stdlib/user.xsh`, and `tests/xsh/stdlib/group.xsh`.

This document is the detailed contract for OS-facing runtime behavior in XSH.
`docs/SPEC.md` remains authoritative for language syntax and user-visible
semantics. This file records how those semantics are implemented at the process,
signal, wait-loop, and evaluator-checkpoint boundary.

The current implementation is Unix-oriented. It uses POSIX signals, process
groups, and `libc` signal APIs. A non-Unix port must preserve the observable
XSH semantics in `docs/SPEC.md`, but its handler and process-group mechanisms
may differ.

## 1. Design Boundary

The OS runtime exists to make process orchestration predictable:

- external commands run at explicit process boundaries;
- owned process work is grouped so it can be canceled as a unit;
- evaluator-owned waits are checkpointed for cancellation and signal hooks;
- OS signal handlers never execute XSH code;
- runtime trace events preserve evidence of shutdown and cancellation paths.

The OS runtime is not a service supervisor, bytecode VM, async runtime, event
loop, green-thread scheduler, or job control implementation. It does not try to
manage descendants that intentionally move into another process group.

## 2. Overall Design

The OS runtime is the coordination layer between tree-shaped XSH evaluation and
the host operating system's graph of processes, process groups, terminal state,
signals, and waits. The language spec defines what a script observes: explicit
`run` boundaries, `Status` values, `ProcessError` failures, `spawn`/`wait`
handles, scoped cwd/env, defers, signal hooks, and traces. This document defines
how the runtime preserves those observations when the host can interrupt,
reorder, or outlive ordinary expression evaluation.

The implementation is intentionally split into three responsibilities:

- Language-facing evaluation owns scopes, values, `$?`, `Result` propagation,
  defers, signal-hook bodies, process handles, and trace parentage.
- The process substrate owns argv/env/cwd conversion, redirections,
  `Command`-plan execution, process-group setup, terminal foreground handoff,
  `waitpid` status decoding, timeout enforcement, cancellation escalation, and
  detached-child reaping.
- The signal substrate owns process-global handler installation, async-signal
  safe signal recording, handler restoration, and child signal-disposition
  reset before exec.

No layer is allowed to smuggle host behavior around the others. The process
substrate does not decide XSH control flow. Signal handlers do not inspect
evaluator state. The evaluator does not perform raw `waitpid` or `tcsetpgrp`
itself; it asks the process substrate to do that work through explicit modes
and cancellation policy objects. This keeps the runtime testable and keeps
observable behavior anchored in the language contract rather than in incidental
Unix races.

The core design is ownership plus checkpoints. Every piece of process work that
XSH starts is either owned by an evaluator scope, owned by an active wait,
released to a background reaper because the user requested detach, or outside
XSH's control because it deliberately moved away from the managed process
group. Ownership answers "who must reap or cancel this?" Checkpoints answer
"when is it safe to turn an OS signal into XSH behavior?" Together they avoid
both extremes: the runtime does not run arbitrary XSH code inside a signal
handler, and it does not ignore shutdown until a whole script happens to return.

The process group is the unit of cancellation. A simple external command gets a
new group. A byte pipeline shares one group across all segments. A managed
`spawn` handle owns one child group until `wait`, `cancel`, lexical cleanup, or
detached release consumes it. For inherited foreground operations, the runtime
may also transfer terminal foreground to the child group and restore the prior
foreground group with a guard. XSH only promises to manage children that stay in
the process group it created or joined; daemons and tools that intentionally
double-fork, call `setsid`, or create their own process groups have crossed an
explicit boundary.

Waiting is mode-specific. Script waits do not expose stopped jobs as public
`Status` data; they keep waiting or return ordinary process errors according to
the language form being evaluated. Interactive foreground waits use stopped
status internally so `xshi` can implement limited job control. Nonblocking
waits are used for polling managed jobs and reapers. All modes share the same
raw status decoder so exit codes, signal deaths, and stopped states cannot drift
between subsystems.

Signals are recorded globally and interpreted locally. The handler records the
first primary signal and, if another arrives before shutdown finishes, one
escalation signal. Evaluators read those slots at checkpoints and decide what
the signal means for the currently running script: run a matching hook, forward
to already-active process groups after the pre-cancel budget, cancel live
`ProcessHandle` children, skip cleanup after escalation, or keep running because
the current hook owns the work being waited. The signal number is global host
input; the shutdown decision is evaluator state.

Signal hooks are a coordinated shutdown path, not asynchronous callbacks. A
hook is registered by top-level evaluation, captures the root-scope values that
exist at that point, and runs at most once for the primary signal. Hook code
uses ordinary XSH evaluation and ordinary effects. It has its own defers, can
start hook-owned process work, and can commit an exit status with `abort`.
Because it runs at checkpoints, it cannot interrupt arbitrary pure expression
evaluation or a blocking host syscall that XSH does not poll. Because escalation
is recorded separately, a second signal can still force bounded shutdown while
the hook is running or cleanup is pending.

Status-as-data and failure-as-control remain distinct across OS boundaries.
External commands can exit nonzero without causing a runtime failure when the
language form requests status data. Setup failures, redirection failures,
timeouts, invalid handles, wait I/O failures, and cancellation failures are
`ProcessError` data or runtime failures according to the public API signature.
The process substrate returns structured outcomes; the evaluator decides
whether to update `$?`, wrap a value in `Ok`, propagate an `Err`, or build a
traceback. This distinction is the reason cancellation policy returns
`Forward` and `Escalate` decisions rather than directly throwing from the wait
loop.

Lexical cleanup is part of process ownership. A live non-detached
`ProcessHandle` owned by a scope is canceled and reaped when that scope exits
unless the handle has been transferred to a surviving value. A detached handle
owned by an exiting scope is released to a background waiter instead. Cleanup
runs before user defers observe the completed scope, while `abort(force: true)`
and escalation keep their documented ability to skip remaining cleanup. This
ordering lets scripts reason about temporary files and service handoff without
leaking child processes.

Tracing is the evidence layer for the whole design. Runtime traces are not just
logs; they preserve the dynamic graph that source syntax alone cannot show:
which expression started a process, which handle id was allocated, which wait
consumed it, which signal was received, whether a hook ran, when forwarding
happened, and whether escalation killed active groups. Trace events must carry
structured argv, cwd, env, handle ids, signal numbers, process statuses, and
errors instead of reconstructed shell strings. A trace consumer should be able
to correlate source spans, process lifetimes, signal decisions, and shutdown
outcomes without guessing from text.

`xshi` uses the same low-level process and terminal primitives, but its session
policy is specified separately in `docs/SPEC-INTERACTIVE.md`. This file owns
the shared guarantees: process groups are created consistently, child signal
handlers are reset before exec, foreground terminal handoff is guarded, and
stopped statuses can be represented internally. `xshi` owns the one-job slot,
`fg`/`bg`, prompt-time reaping, and interactive-only `&` syntax. Normal `.xsh`
scripts never acquire shell job-control syntax through this substrate.

The design therefore favors conservative, explicit host control:

- start process work only through typed runtime entry points;
- group children so cancellation can be bounded;
- observe signals only at checkpoints;
- run hook code in the evaluator, never in the handler;
- preserve status data separately from process failures;
- clean owned children when scopes end;
- release only when the user requested detach;
- expose OS-facing decisions through structured traces.

Any extension to this file should preserve that shape. New host integrations
should first identify their owner, checkpoint behavior, cleanup responsibility,
signal interaction, public status/error shape, and trace evidence before adding
new API surface.

## 2.1 Surface Syntax Examples

This file specifies runtime behavior, but the OS-facing features are easier to
audit when their accepted source forms are visible beside the runtime contract.
The language grammar remains in `docs/SPEC.md`.

Signal hooks are top-level entry-script statements:

```xsh
on SIGINT --pre-cancel=150ms [fs, process, error] {
  p"/tmp/build.interrupted".write("interrupted\n")?
  abort(130)
}

on USR1 [] {
  abort(0)
}
```

The accepted grammar shape is:

```text
signal_hook_stmt = "on" signal_name hook_option* effect_list block
hook_option      = "--pre-cancel=" duration_literal
effect_list      = "[" (effect ("," effect)*)? "]"
```

`on` is contextual. Effects are required; `[]` is the explicit no-effect list.
`--pre-cancel` is optional and defaults to `150ms`. Signal names are named
identifiers written with or without one leading `SIG` prefix. Numeric hook
declarations are rejected.

Process fan-out uses `spawn`, `wait`, and `ProcessHandle.cancel`:

```xsh
let build = spawn run make all ?
let tests = spawn run make test ?
let statuses = wait [build, tests]?

let child = spawn run sleep 60 ?
child.cancel(signal: "TERM", kill_after: 0ms)?
```

A typed `Command` plan can also be spawned:

```xsh
let cmd = process.command {
  cwd = p"src"
  run make check
}

let handle = spawn cmd?
let status = wait handle?
```

`spawn run` accepts exactly one plain `run` or `run.status` segment and returns
`Result[ProcessHandle, ProcessError]`. Captures, streams, and byte pipelines are
not accepted by `spawn run` in v1. `spawn command_expr` evaluates the expression
to `Command` and starts that plan as an owned handle. `wait handle` returns
`Result[Status, ProcessError]`; `wait [handles]` returns
`Result[List[Status], ProcessError]`. A trailing `?` applies to the `Result`
produced by the whole `spawn`, `wait`, or `cancel` expression.

## 3. Signal State

XSH uses two process-global atomic slots for handled signals:

- `PRIMARY_SIGNAL`: the first handled signal observed for the current script
  run.
- `ESCALATION_SIGNAL`: the first later handled signal observed before the
  primary shutdown path finishes.

The OS handler uses only async-signal-safe atomic operations:

1. If `PRIMARY_SIGNAL` is empty, store the received signal there.
2. Otherwise, if `ESCALATION_SIGNAL` is empty, store the received signal there.
3. Never allocate, format, lock ordinary Rust data, evaluate XSH code, run
   cleanup, or inspect evaluator state in the handler.

The first handled signal wins. Later handled signals do not replace the primary
signal and do not start another hook.

At script start, binaries clear both slots and install base handlers for
`SIGINT` and `SIGTERM`. Hook declarations can install additional handlers as
they are evaluated. At script end, the installed handlers are restored and the
state is cleared before another invocation can observe it.

## 4. Handler Guards

Signal handler installation is guarded by RAII:

- installing a handler captures the previous handler with `sigaction`;
- a failed multi-signal installation restores any handlers already installed
  during that attempt;
- dropping the guard restores previous handlers in reverse order;
- top-level binaries keep the base `SIGINT`/`SIGTERM` guard alive for the whole
  invocation;
- evaluators keep per-hook guards alive after hook registration.

This prevents tests and embedded evaluator use from leaking handler state across
script invocations.

## 5. Child Handler Reset

Child processes must not inherit XSH's cancellation and hook handlers as their
own runtime policy. Before exec, XSH resets handled hook-surface signals to
default dispositions:

```text
HUP INT QUIT TERM USR1 USR2 ALRM XCPU XFSZ
```

This preserves the expected host behavior for external commands while allowing
the parent XSH process to observe and coordinate signals.

## 6. Hook Signal Surface

Hook signal names are normalized by uppercasing ASCII letters and stripping one
leading `SIG` prefix. The v1 accepted hook names are:

```text
HUP INT QUIT TERM USR1 USR2 ALRM XCPU XFSZ
```

`XCPU` and `XFSZ` are accepted only where libc exposes them and the runtime can
resolve them. Numeric declarations are rejected even though lower-level process
APIs can work with signal numbers. `KILL`, `STOP`, `CHLD`, `CONT`, `TSTP`,
`TTIN`, `TTOU`, and `PIPE` are rejected for signal hooks.

Signal validation is centralized in `src/runtime/signal.rs`; parser, checker, and
runtime behavior must not duplicate divergent allowlists.

## 7. Evaluator Shutdown State

Each evaluator maintains shutdown state derived from the global signal slots:

- whether `signal.received` has been traced;
- whether a hook has started;
- whether the evaluator is currently running a hook;
- whether the primary signal has been forwarded to child process groups;
- whether escalation has been traced;
- committed shutdown status, if a hook chose one;
- committed force shutdown, if a hook called `abort(..., force: true)`;
- pre-cancel deadline;
- source span of the active hook.

The evaluator owns hook execution. Global atomics only indicate that a signal
arrived; all XSH semantics happen at evaluator checkpoints.

## 8. Hook Registration

A signal hook is registered when its top-level declaration is evaluated, not
when the file is parsed and not when functions are collected.

Registration:

- normalizes and validates the signal;
- parses or defaults `--pre-cancel` to `150ms`;
- installs a handler for that signal;
- stores a clone of the hook body;
- captures the root-scope bindings that already exist;
- records whether the same signal was already pending before registration.

If the signal was already pending before registration, that hook ignores the
pending primary signal. This is what makes "signals before the hook declaration
is evaluated" behave like no matching active hook.

## 9. Hook Scope

Hook execution uses a root-scope snapshot captured at registration time.

The hook can see:

- root procs and pures collected before top-level execution;
- standard modules;
- top-level values evaluated before the hook declaration;
- hook-local bindings declared inside the hook body.

The hook cannot see later top-level values. Hook-local defers are scoped to the
hook block and run through the ordinary defer mechanism unless force abort or
escalation stops cleanup.

## 10. Checkpoints

Signal servicing happens only at evaluator-owned checkpoints. The runtime must
checkpoint at these boundaries:

- before and after root top-level statements;
- before and after block statements;
- loop iteration boundaries;
- before and after deferred cleanup actions;
- process wait polls for `run`, pipelines, captures, `process.run`, and
  `time.measure`;
- chunked `time.sleep` waits;
- parallel stream parent scheduling and result collection.

The runtime does not promise arbitrary interruption of CPU-bound expression
evaluation or blocking host filesystem/network calls that do not use an
XSH-owned poll loop. Those paths observe signals at the next checkpoint after
they return.

Checkpoints must be cheap when no signal is pending.

## 11. Service Algorithm

At a checkpoint, the evaluator reads the global signal snapshot.

If no primary signal exists, service returns immediately.

If escalation exists:

- emit `signal.escalate` once;
- kill active owned process groups immediately;
- mark shutdown complete;
- skip remaining cleanup after the current safe point.

If a hook is currently running:

- if the pre-cancel deadline has expired, forward the primary signal to
  non-hook-owned active process groups;
- otherwise continue the hook.

If a hook already started or shutdown is already complete, service returns.

If no matching hook is registered:

- for `SIGINT` and `SIGTERM`, existing process cancellation policy forwards to
  active child process groups and returns the existing canceled runtime failure
  shape;
- for other signals without an installed hook, host default behavior applies.

If a matching hook is registered:

1. Mark hook started.
2. Set the pre-cancel deadline.
3. Emit `signal.received`.
4. Emit `signal.hook.enter`.
5. Run the hook body with the captured scope and a fresh defer stack.
6. Emit `signal.hook.exit`.
7. Forward the primary signal to non-hook-owned active process groups if it has
   not already been forwarded.
8. Commit shutdown status or error according to the hook result.

Only one hook can run for the primary signal.

## 12. Pre-Cancel Forwarding

`--pre-cancel` is the time a hook may delay forwarding the primary signal to
already-active child process groups.

The budget starts when hook execution starts. Forwarding is idempotent. XSH
forwards the primary signal when either condition is true:

- the hook completes before forwarding; or
- the hook reaches a checkpoint after the pre-cancel deadline.

A hook-owned blocking wait is not canceled by the primary signal. This lets a
hook run explicit handoff work such as `supervisorctl stop` after the first
`Ctrl-C`. Hook-owned waits still observe escalation.

## 13. Process Groups

Every external `run` command has a process-group cancellation root. A byte
pipeline has one cancellation root shared by every segment. Active process
groups are tracked by the evaluator with an ownership flag:

- non-hook-owned: process work that was active before or outside the hook;
- hook-owned: process work started while the hook is running.

Forwarding sends the primary signal only to non-hook-owned active groups.
Escalation sends `SIGKILL` to all active groups.

After forwarding, process wait code preserves existing timeout, capture-limit,
status collection, and canceled `ProcessError` behavior. For `SIGINT` and
`SIGTERM`, no-hook cancellation keeps the existing runtime failure shape. For
non-`INT`/`TERM` hooked signals, canceled process errors carry the actual
forwarded signal number.

## 14. Cancellation Policy

Process waits ask a cancellation policy instead of directly consuming the
global signal slot.

The evaluator policy can return:

- `Continue`: keep waiting;
- `Forward(signal)`: send the primary signal to the process group;
- `Escalate(signal)`: kill the process group immediately.

This lets the evaluator run a matching hook before the process runtime forwards
the signal to owned work. Default process helpers still use the legacy no-hook
policy when no evaluator policy is available.

## 15. `time.sleep`

`time.sleep` is checkpointed. It computes a deadline, sleeps in chunks no
larger than the process wait poll interval, and services pending signals
between chunks.

Outside a hook, a signal can interrupt long sleeps promptly. Inside a hook,
sleep ignores the primary signal and continues until completion unless
escalation is requested.

## 16. Parallel Streams

Parallel stream worker evaluators never run hooks. Workers are forked with an
empty hook table and no signal handler guards. They may observe cancellation or
escalation at their own checkpoints, but hook ownership stays with the parent
evaluator.

The parent evaluator services signals:

- before scheduling each worker job;
- while collecting worker results with a timeout-based receive loop;
- after shutdown begins, before scheduling any more work.

When shutdown commits a status, the parent stops scheduling and returns unit so
the top-level shutdown status can determine process exit.

## 17. Defers And Force Abort

Normal `abort(status)` from a hook commits the requested status, cancels owned
child process groups, and lets hook-local and outer defers run unless
escalation interrupts cleanup.

`abort(status, force: true)` commits the requested status, cancels owned child
process groups, and skips both hook-local and outer defers. The force decision
is stored in evaluator shutdown state so cleanup code can observe it after the
hook body unwinds.

Repeated signals during cleanup are escalation. Escalation kills active owned
process groups and stops remaining cleanup after the current safe point.

## 18. Exit Status

If a hook calls `abort(status)`, that status wins unless a later runtime failure
takes precedence.

If a matching hook completes normally:

- `INT` and `TERM` default to status `3`, matching XSH runtime cancellation;
- non-`INT`/`TERM` hooks default to `128 + signal_number`.

If a hook returns `Err` or fails at runtime, diagnostics and traceback record
the hook failure and the script exits with runtime failure status `3`.

Module APIs that wrap process waits, such as `process.run` and `time.measure`,
must not let a forwarded-child cancellation override a hook-committed shutdown
status.

## 19. Trace Events

The signal trace event kinds are:

```text
signal.received
signal.hook.enter
signal.hook.exit
signal.forward
signal.escalate
```

Signal payloads include:

- normalized signal name;
- signal number;
- shutdown phase;
- whether a matching hook existed;
- whether forwarding had already occurred;
- `pre_cancel_ms` when known;
- escalation signal name and number when present;
- hook error kind and message when hook exit failed.

No-hook `SIGINT` and `SIGTERM` paths keep their existing trace behavior unless
a hook is registered.

## 20. Tooling And Interactive Scope

`xsht check`, `xsht fmt`, and docs tooling parse, check, and format hook syntax
but do not install hook handlers unless they execute scripts.

`xshi` does not support ordinary `.xsh` signal hook declarations in interactive
input. The checker rejects them as interactive-unsupported syntax.

## 21. Verification Surface

OS runtime changes that affect signal hooks should be covered at these levels:

- syntax and formatter tests for contextual `on`, required effects, options,
  comments, and idempotent formatting;
- checker tests for signal validation, placement, exports, modules,
  `module.load`, forward references, effects, and hook result types;
- runtime tests for hook status, default status, trace events, `time.sleep`,
  hook-owned process work, pre-cancel forwarding, escalation, defers,
  force abort, hook failure cleanup, and parallel stream parent checkpoints;
- no-hook `SIGINT` and `SIGTERM` cancellation regression tests;
- a cataloged deterministic example that does not require a human signal.

OS runtime changes that affect `spawn`, `wait`, or `ProcessHandle.cancel`
should be covered at these levels:

- syntax and formatter tests for `spawn run`, `spawn command_expr`,
  `wait handle`, `wait [handles]`, trailing `?`, and cancel named arguments;
- checker tests for `ProcessHandle` annotations, handle fields, required
  `process` and `error` effects, invalid wait targets, and rejected capture or
  pipeline spawn forms;
- runtime tests for status-as-data, list wait ordering, duplicate and
  already-consumed handles, timeout-from-spawn timing, setup failures, explicit
  cancellation, lexical cleanup, detached release, and ownership transfer
  through return values and containers;
- signal cancellation tests proving live non-detached handles are canceled at
  evaluator checkpoints rather than inside OS handlers;
- trace tests proving spawn, wait, and cancel events include handle ids and
  structured argv/status/error payloads;
- cataloged deterministic examples that avoid long sleeps and timing-sensitive
  output.
