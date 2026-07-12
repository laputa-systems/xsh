# Chapter 4: Processes And System State

XSH keeps process execution explicit. `run` starts host commands, status values
can be inspected as data, captures are typed, and scoped environment or
directory changes stay inside their blocks.

This chapter moves from one command to larger process workflows. Tracing gets
its own chapter afterward.

## Running Commands

Use `run` when a script should execute an external command. In statement
position, unsuccessful exits and failed pipeline segments propagate as failures.

```xsh
run printf "%s\n" "run ok"
```

The command writes a line through `printf` and requires success.

Long commands can use the grouped invocation form. Each line inside the
parentheses is one command argument, and the trailing `?` still applies to the
whole run form.

```xsh
let out = run.text (
  printf
  "%s"
  "grouped run"
) ?

print $out
```

Command arguments that are clearly expression chains can be written without
wrapping parentheses, such as `input.display()` or `argv[0]`. Whitespace still
ends the argument, so a separated trailing `?` applies to the run form; write
`(expr?)` when the argument itself should contain a propagated expression.

Why XSH shines here: external execution is opt-in, which makes process
boundaries visible in code review and traces.

Compared with bash and CLI tools: a shell script can run `printf`, but the
command boundary, argv shape, and failure policy are implicit. In XSH, `run`
marks the boundary in source and gives the checker and tracer a real operation
to talk about.

`--cpumax=N` can be attached to process-backed `run` and `spawn run` forms when
a child process should be CPU-limited. `N` is a percentage of one CPU, so
`--cpumax=80` requests 80% of one core. Linux enforces the limit through
cgroups v2, macOS accepts the option as a no-op, and other non-Linux platforms
reject it when requested.

`run.builtin` and its status, capture, and stream variants are legacy spellings
that now execute like the corresponding `run` forms. Prefer the non-builtin
spelling in new code.

## First Useful Tool: Probe A Command

This is the first script in the guide that feels like a tool you might keep.
It finds a host command, captures text, and treats an expected failure as data.

```xsh
let shell = process.which("sh")?
let out = run.text sh -c "printf '%s\n' probe" ?
let status = run.status false
print (shell.name != "") out.trim() status.exited_with(1)
```

The script proves three things without parsing shell text: `sh` exists, command
output can be captured as UTF-8 text, and `false` exited with status 1.

Compared with bash and CLI tools: the bash version would usually mix command
substitution, `$?`, and string tests. XSH keeps those as three typed values:
`Path`, `Str`, and a status record.

## Status As Data

Use `run.status` when a nonzero exit is expected and should guide later logic.
It returns a status record instead of propagating unsuccessful completion.

```xsh
let status = run.status false
print status.exited_with(1)
```

The example checks that `false` exited with status code 1.

Why XSH shines here: expected process failures can be inspected without turning
into exceptions or disappearing into shell conditionals.

Do not use `run.status` for commands that must succeed. Plain statement-position
`run` is better there because unsuccessful completion propagates immediately.

## Process Fan-Out

Use `spawn` when independent child processes should start now and be waited
explicitly later.

```xsh
let ok = spawn run true ?
let expected_failure = spawn run false ?
let statuses = wait [ok, expected_failure]?
print statuses[0].ok statuses[1].exited_with(1)
```

Both children start before the `wait` expression. The list wait returns status
values in handle order, so the expected nonzero exit remains data.

Why XSH shines here: process handles keep ownership explicit. A live
non-detached child is waited, canceled, transferred outward, or cleaned up at
scope exit instead of becoming an accidental background process.

Do not use `spawn` just to look parallel. Use it when independent work can make
progress at the same time and when the script has a clear point where ownership
comes back through `wait`.

## Capturing Text

`run.text` captures stdout as `Str`. It is a good fit for command output that
is known to be UTF-8.

```xsh
let out = run.text printf "%s" "captured text" ?
print $out
```

The captured value is printed by XSH after the process exits.

Why XSH shines here: the capture type documents the boundary between a process
stream and script data.

Do not use `run.text` for data that may be binary or may contain invalid UTF-8.
Use `run.bytes` and convert explicitly when the encoding is known.

## Capturing Bytes

Use `run.bytes` when command output is binary or might contain NUL bytes.

```xsh
let out = run.bytes head -c 1 /dev/zero ?

if out == b"\0" {
  print "ok"
}
```

The example reads one byte from `/dev/zero` and compares it to a bytes literal.

Why XSH shines here: byte data does not have to masquerade as text before it
can be checked.

Use `run.capture --text` or `run.capture --bytes` when stderr and status are
part of the result. Nonzero child exits return `Ok({status, stdout, stderr})`,
while setup failures, timeouts, capture limits, and text decoding failures
return `Err(ProcessError)`.

For a compact comparison of every `run.*` form, return type, and nonzero-exit
policy, see `docs/REFERENCE.md` under `Run Forms`.

## Script Stdin And Stdout

Use `io.stdin_text()?`, `io.stdin_line()?`, or `io.stdin_bytes()?` when the
script itself should consume stdin. Use `io.write_stdout(text)?` or
`io.write_stdout_bytes(bytes)?` when output should not get `print`'s automatic
newline or display conversion. `io.stdin_line` reads one UTF-8 line without its
line ending. `io.write_stdout_bytes` writes bytes exactly, including non-UTF-8
data. These calls require the `io` effect. For the complete effect summary, see
`docs/REFERENCE.md` under `Effects`; detailed standard-library signatures remain
in `docs/STDLIB.md`.

## Scoped Environment

Environment assignments can be attached to a process or scoped to a block.
Inside an `env` block, typed environment reads use the requested shape.
When the variable name is dynamic, `env.get_or`, `env.bool`, `env.path`, and
`env.int` provide fallback-aware reads without hand-written `match env.get`
wrappers.
For PATH-like variables where empty entries and original positions matter, use
`env.path_entries(name)?` instead of splitting strings by `:`.

```xsh
let cc_raw = run.text CC=cc CFLAGS="-O2 -pipe" printenv CC ?
let cflags_raw = run.text CC=cc CFLAGS="-O2 -pipe" printenv CFLAGS ?
let cc = cc_raw.trim()
let cflags = cflags_raw.trim()
let configured = f"${cc}|${cflags}"
print $configured

env {
  DESTDIR = /tmp/xsh-dest
  XSH_EXAMPLE_FLAG = "yes"
  XSH_EXAMPLE_THREADS = "8"
} {
  let dest = env.path("DESTDIR")?
  let flag = env.bool("XSH_EXAMPLE_FLAG")?
  let threads = env.int("XSH_EXAMPLE_THREADS")?
  let fallback = env.get_or("XSH_EXAMPLE_MISSING", "fallback")?
  print $dest
  print $flag $threads $fallback
}
```

The first commands set process-local variables. The block then exposes
`DESTDIR` as a `Path`.

Why XSH shines here: environment changes are local by default, and typed reads
make downstream path handling explicit.

Compared with bash and CLI tools: `VAR=value cmd` and `cd dir; ...` are easy to
leak across later commands. XSH makes both forms lexical, so a review can see
where state begins and ends.

## Scoped Working Directories

`cd` can run a block in another directory and then restore the previous working
directory.

```xsh
let before = run.text pwd ?

cd examples {
  let inside = run.text pwd ?
  let changed = inside != before
  print $changed
}

let after = run.text pwd ?
let same = after == before
print $same
```

The example proves that the working directory changes inside the block and is
restored afterward.

Why XSH shines here: directory state becomes a lexical scope instead of a
global side effect that every later command must remember.

## Process And System Records

Standard modules expose host state as records. Process lists, user and group
lookups, system identity, OS release data, memory counters, signals, clocks,
and measured commands can be composed with normal expressions.

```xsh
let shell = process.which("sh")?

let process_count = process.list()
  |> where .pid > 0
  |> count()

let host = system.hostname()?
let os = system.uname()?
let me = user.current()?
let same_user = user.by_uid(me.uid)?
let same_group = group.by_gid(me.gid)?
let term = process.signal("TERM")?
time.sleep(1ms)

let command = process.command {
  run true
}

let measured = time.measure(command)?
print ${shell.name == "sh"} ${process_count > 0} ${host != ""} ${os.sysname != ""} ${me.uid >= 0}
print ${same_user.uid == me.uid} ${same_group.gid == me.gid} ${term.number > 0} ${time.now() > 0}
print (time.format(0, "%Y", utc: true)?)
print ${me.name != ""} ${me.home.display() != ""} $measured.status.ok ${measured.duration_ms >= 0}
```

Output note: the exact host values vary, so the example prints booleans and a
fixed UTC year check.

Why XSH shines here: system facts arrive as typed records, which makes checks
and table-building less fragile than parsing command output.

## Signal Hooks

Entry scripts can declare a top-level hook for a shutdown signal:

```xsh
on USR1 [] {
  print "signal"
  abort(0)
}

let pid = process.current_pid()?
process.kill(pid, signal: "USR1")?
process.stats(pid)?
```

Hooks run at evaluator checkpoints, not inside OS signal handlers. They are for
short shutdown prep: writing a marker, removing a lock file, asking owned work
to stop, or choosing an exit status. A repeated signal escalates instead of
running the hook again.

Hook effects are explicit. Use `abort(130)` for the conventional Ctrl-C status,
and use explicit timeouts for process work started inside a hook. Hooks are
entry-script-only in v1; imported modules cannot register them.

## Summary

Process workflows in XSH are explicit about execution, capture type, scoped
state, and failure propagation. A later advanced chapter shows how tracing
makes those runtime boundaries visible.
