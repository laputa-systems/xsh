# A Tour of XSH

XSH is a clean-slate systems scripting language for a modern Linux userspace.

It is not a POSIX shell replacement in the compatibility sense, and it is not
an interactive terminal interface. It is a language for writing the glue that
holds a system together: package managers, build recipes, init systems, service
supervisors, installer scripts, maintenance tools, and distribution policy.

## The Archaeological Site

The modern Linux userspace is an archaeological site. Beneath every build lies
sedimentary layers of languages accumulated over decades: shell scripts call m4
macros, configure scripts emit Makefiles, Makefiles invoke compilers through
wrapper scripts, and those wrappers are often written in Perl, Python, awk, sed,
or a private DSL that nobody meant to become infrastructure.

This is Unix sludge: the entropic product of many weak languages duct-taped
together because no single glue language was powerful enough for the whole job.

XSH starts from a different premise. The system deserves one strong language for
glue, one that can speak fluently to processes, files, paths, byte streams,
structured data, and system state without turning every boundary into a quoting
puzzle.

## What Shell Got Right

The old Unix shell succeeded because it made operating-system pieces feel
composable. Processes, files, pipes, environment variables, working
directories, exit statuses, and argument vectors became everyday building
blocks. A small script could assemble existing programs into something larger
than any one of them.

That idea is still right.

XSH keeps the useful model: coarse-grained reuse, explicit process boundaries,
pipeline-shaped data flow, ordinary source files, and scripts that can grow into
tools. It treats the Unix process model as an asset, not as historical baggage.
The expensive work should be visible as a process, a file operation, or a typed
host API, not hidden behind a scheduler or runtime trick.

## What Shell Got Wrong

The traditional shell encoded too much composition in strings and ambient state.
It made parsing dynamic, word splitting implicit, quoting fragile, error flow
surprising, and standards vague enough that every serious script eventually
became a local dialect.

XSH rejects that sludge:

- no implicit eval;
- no hidden word splitting;
- no untyped text as the only interface between programs;
- no ad hoc DSL stacking where a script generates another language to generate
  another language;
- no pretending that decades of compatibility quirks are a design philosophy.

The goal is not to preserve the old spellbook. The goal is to carry forward the
part of Unix that was worth preserving.

## One Glue Language

XSH is not trying to be a general-purpose application language. It is trying to
be the best possible language for orchestration: starting processes, shaping
argv, moving through directories, reading and writing files, transforming text
and bytes, crossing JSON boundaries, inspecting host state, and making expected
failures visible.

Shell is the language of heterogeneity. It must speak to everything. XSH says
that heterogeneity should be handled with clarity rather than incantation.

For old Unix hands, the promise is familiar: small pieces, composed well. The
difference is that XSH gives that promise a modern type system, structured
errors, typed paths, structured streams, and a runtime that can trace what
happened.

That is the worthy successor: not a clone of the old shell, and not a small
application runtime wearing shell syntax, but a clean language for the work the
old shell proved was essential.

## What XSH Is Not

XSH does not compete with runtimes designed for fine-grained concurrency,
long-lived application services, or interactive terminal interfaces. Keep those
jobs in a service runtime, a dedicated TUI framework, or a specialized tool;
use XSH to compose the host-facing work around them.

The rest of this tour shows what that means in practice. Most sections start
from a shell script you have probably written, point at the place it breaks,
and then show the XSH version. Read it top to bottom once before you write
your first script; after that, [`docs/SPEC.md`](SPEC.md) and `xsht api` are the
references.

Every `xsh` block below is a complete program that passes `xsht check`. Blocks
followed by output were run to produce it. Blocks marked `# platform: linux`
read `/proc` or other Linux-only interfaces.

## Running a Script

```xsh
{{.tour.hello.source}}
```

<!-- expected-output -->

```text
{{.tour.hello.output}}
```

Two binaries do the work. `xsh` runs scripts; `xsht` is the toolchain:

```bash
xsh hello.xsh                 # run it
xsh deploy.xsh -- web-2 -f    # arguments after `--` go to the script
chmod +x hello.xsh && ./hello.xsh
xsht check hello.xsh          # parse, resolve, and type-check without running
```

`xsh` checks the whole program before executing anything. A type error on the
last line means the first line never runs, so a half-applied change caused by
a typo three screens down cannot happen. Check failures exit with status 2;
runtime failures exit with status 3.

Script arguments arrive as `args: List[Str]`. For anything beyond a couple of
positional words, declare the interface instead of parsing it. A `cli main`
signature is the argument parser, the usage text, and the type conversion:

```xsh
{{.tour.largest_files.source}}
```

Required parameters are positional; defaulted ones become options
(`--limit 3`, `--hidden`); `-h` prints generated help. A bad `--limit` value
is rejected with usage status 2 before any of your code runs.

## Values at a Glance

```xsh
{{.tour.values.source}}
```

<!-- expected-output -->

```text
{{.tour.values.output}}
```

Every value has a type, and the checker infers most of them: `Str`, `Int`,
`Float`, `Duration`, `Path`, `List[Str]`, a record `{cpu: Int, memory_mb: Int}`,
and an optional `Str?`. `const` is data fixed when the script is checked, `let`
is an immutable runtime binding, and `var` is mutable. `f"..."`
interpolates `{expr}`; plain `"..."` never interpolates.

There are no implicit conversions. `"8080" + 1` is a check error, and
`"8080".parse_int()` returns `Result[Int]` because parsing can fail. You will
see that pattern everywhere: anything that can fail says so in its type.

## Commands and argv

The bug every shell scripter has shipped at least once:

```bash
file="quarterly report.pdf"
rm $file                      # removes "quarterly" and "report.pdf"
flags="-l -a"
ls $flags "$dir"              # works only *because* of word splitting
for f in $(find . -name '*.log'); do gzip $f; done   # spaces, globs, newlines
```

In XSH a value is one argv item. Lists splice only where you write `@`. Words
are never split, globbed, or expanded:

```xsh
{{.tour.argv.source}}
```

<!-- expected-output -->

```text
{{.tour.argv.output}}
```

`$name` and `${expr}` interpolate into a command word; `"*.log"` stays a
literal asterisk. When you actually want a glob, say so with a glob literal,
`g"*.log"`, which produces a `List[Path]` you can splice with `@`. There is no
shell underneath: external programs only run through `run`, and a bare word
like `make` in statement position is an unresolved name, not a `PATH` lookup.

The `run` family chooses what you get back:

| Form | Result |
|---|---|
| `run cmd ...` | statement: fails the script on nonzero exit; value: `Status` |
| `run.status cmd ...` | `Status`, never fails on exit code |
| `run.text cmd ...` | `Result[Str]`: captured stdout |
| `run.bytes cmd ...` | `Result[Bytes]` |
| `run.capture --text cmd ...` | `Result[{status, stdout, stderr}]` |
| `run.stream --text cmd ...` | `Result[Stream[Str]]`: lines as they arrive |

Byte pipelines and redirections look the way you expect:
`run tar -cf - $dir | run zstd -q > $archive`. Environment and working
directory changes are scoped to a block, so they cannot leak into the rest of
the script:

```xsh
{{.tour.scoped_env.source}}
```

<!-- expected-output -->

```text
{{.tour.scoped_env.output}}
```

`e"GREETING"` reads a variable; [Environment Variables](#environment-variables)
covers reading, setting, and what children inherit.

## Paths

Paths are a type, not strings that happen to contain slashes. They hold native
bytes, so a filename that is not valid UTF-8 still round-trips to `run`
untouched.

```xsh
{{.tour.paths.source}}
```

<!-- expected-output -->

```text
{{.tour.paths.output}}
```

Literals that start with `/`, `./`, or `../` are paths. `p"..."` makes a path
from any literal and `fp"..."` builds one with interpolation. There is no `/`
operator on paths and no implicit normalization: what you wrote is what the
kernel sees. File operations are methods (`read_text`, `lines`, `write`,
`write_atomic`, `exists`, `mkdir`) or `fs.*` functions (`fs.files`, `fs.walk`,
`fs.copy`, `fs.rename`, `fs.mounts`).

## Errors, Results, and `?`

`set -euo pipefail` is a promise the shell does not keep:

```bash
set -euo pipefail
count=$(grep -c ERROR app.log)  # zero matches: grep exits 1, script dies silently
deploy() { cd /srv/app; git pull; systemctl restart app; }
if deploy; then echo ok; fi     # set -e is off inside deploy: every step runs
local version=$(get_version)    # `local` succeeds, so the failure is masked
```

XSH has no `set -e` to forget. A `run` in statement position that exits
nonzero fails the script, every time, in every context, with the command that
failed:

```xsh
{{.tour.backup.source}}
```

```text
tar: Failed to open '/backups/app.tgz'
err: `tar` exited 1
executable: /usr/local/bin/xsh
cwd: /home/ops
argv: tar -czf /backups/app.tgz /var/lib/app
at backup.xsh:2:3-2:26
call path:
  1. proc backup at backup.xsh:6:1-6:39
```

When a nonzero exit is an answer rather than a failure, say which codes are
acceptable. `grep` exits 1 for "no matches":

```xsh
{{.tour.accepted_status.source}}
```

<!-- expected-output -->

```text
{{.tour.accepted_status.output}}
```

Functions that can fail return `Result[T]`. Four tools handle it:

- `expr?` unwraps `Ok` or returns the `Err` to the caller.
- `expr ?? fallback` recovers with a default.
- `match` handles each outcome.
- `guard ... else { ... }` exits early when a precondition fails.

Expected failures get names. An `error` declaration is a small family of typed
variants that callers can match on, instead of grepping message strings:

```xsh
{{.tour.typed_errors.source}}
```

<!-- expected-output -->

```text
{{.tour.typed_errors.output}}
```

The `[fs, error]` clause lists the proc's effects; the [Effects](#effects)
section covers it. `error` is the effect that permits `?`.

Two more tools round this out. `ctx "description" { ... }` attaches context to
anything that fails inside the block, so a bare "No such file or directory"
becomes "No such file or directory (ctx: loading /etc/app.json)". `try { ... }`
turns a block into a `Result` value when you want to collect failures as data
instead of propagating them.

## Environment Variables

In shell, `$DEPLOY_TARGET` is the empty string whether the variable is unset,
empty, or misspelled, and `export` changes every later command in the script.
XSH reads a variable with an e-string, `e"NAME"`, whose value is a
`Result[Str]`: a missing variable is an `Err` you handle like any other.

```xsh
{{.tour.env_read.source}}
```

<!-- expected-output -->

```text
{{.tour.env_read.output}}
```

`e"NAME"` is the same read as `env.Str.NAME` and `env.get("NAME")`, so `?`
propagates a missing variable, `??` supplies a default, and `match` tells the
cases apart. The name is a literal identifier and e-strings never interpolate;
read a computed name with `env.get(f"{prefix}_HOME")`. Typed reads do the
conversion and keep a malformed value an error: `env.int` and `env.bool` take a
fallback used only when the variable is unset, and `env.Path.NAME` and
`env.path` read native bytes.

Environment values are bytes, which is the non-UTF-8 rule's reason for being:
a text read of a value that is not valid UTF-8 fails rather than guessing,
while a `Path` read and every child process get the bytes unchanged.

Assigning to an e-string sets a variable:

```xsh
{{.tour.env_set.source}}
```

<!-- expected-output -->

```text
{{.tour.env_set.output}}
```

`e"NAME" = value` takes any value that converts to one command argument, the
way `run` converts its arguments: a `Path` keeps its bytes and `3` becomes
`"3"`. The right-hand side is an ordinary expression, so `e"CC" = clang` names
a binding and the text is `"clang"`. There is no unset, and `null` is rejected
rather than meaning one.

An `env NAME=value { ... }` or `env (record) { ... }` block is a scope: when it
ends, by finishing, failing, or returning early, the environment goes back to
what it was when the block started. An assignment therefore lasts until the
innermost enclosing `env` block ends, and outside any block for the rest of the
script. A proc that sets a variable sets it for its caller too, because the
environment belongs to the running script, not to a binding.

A child process inherits XSH's environment as it is when the child starts:
what the script started with, plus assignments, scope overlays, and `env.PATH`
edits. A `NAME=value` word before a command adds to that child alone. XSH never
changes its own process environment, so none of this leaks into the host.

`env.PATH` is a typed view of `PATH`: `prepend`, `append`, and `pop` take and
return `Path` values, and `dir in env.PATH` tests an exact entry, never a
substring. Like an assignment, the edit ends with its `env` scope.

Reading and setting are the `env` effect, so a `pure` function can do neither.

## Records, Lists, and Maps

Records are the unit of structure. A `type` names a schema; the checker then
knows every field and rejects typos.

```xsh
{{.tour.records.source}}
```

<!-- expected-output -->

```text
{{.tour.records.output}}
```

Values have value semantics: `upgraded` is a new record and `fleet` is
unchanged. Maps iterate in key order and `group-by` keeps encounter order, so
output is deterministic. List patterns in `match` replace most hand-written
argument dispatch.

A schema's constructor also takes fields positionally, in declaration order,
when no single value could fit two of them. Where the checker knows which enum
or error family a value must be, `.Binary` names its variant:

```xsh
{{.tour.manifest.source}}
```

<!-- expected-output -->

```text
{{.tour.manifest.output}}
```

`Host("web-1", "web", 4)` is an error instead, because `name` and `role` are
both `Str` and a swap would go unnoticed; such fields are passed by name. A
leading-dot variant needs that expected type: `let kind = .Binary` is an
error, and inside a stream stage `.name` always reads the item.

## Streams and Pipelines

The classic log one-liner:

```bash
grep ' 500 ' access.log | awk '{print $7}' | sort | uniq -c | sort -rn | head
```

It works until a response size happens to be 500, or the log format gains a
field and `$7` silently becomes something else. Every stage re-parses text the
previous stage flattened.

XSH parses once, at the edge, into records. After that, `|>` passes typed
values between stages:

```xsh
{{.tour.access_log.source}}
```

<!-- expected-output -->

```text
{{.tour.access_log.output}}
```

`stream` declares a lazy producer: nothing is read until a pipeline pulls, and
a pipeline that stops early closes the file. `rx"..."` regexes are compiled
and validated by `xsht check`, so a broken pattern is a check error rather than
a runtime surprise. `$` means the end of the whole text, not the end of a line,
so trim what you read from files before anchoring (or use `(?m)`). The
`/healthz` line with a 500-byte body is not miscounted
as a server error, because `status` and `size` are different fields with
different meanings, not columns 9 and 10.

Pipelines evaluate to a `List`. The stage vocabulary is small and regular:
`where`, `map`, `flat-map`, `sort`, `sort-by`, `take`, `drop`, `unique-by`,
`enumerate`, `batch`, `par-map`, and terminals such as `count`, `sum`,
`first`, `group-by`, `fold`, and `each`. `.field` is shorthand for a
one-field projection. `xsht api language:stream.where` (and so on) documents
each stage.

Keep the two pipe operators straight: `|` connects processes byte-to-byte,
exactly like the shell; `|>` connects XSH values.

## Host State Without Scraping Text

Much of sysadmin scripting is scraping `ps`, `ss`, `df`, and `ip` output whose
columns shift between versions and locales. XSH reads the same state as
records:

```bash
ps -eo pid,etimes,comm --sort=-etimes | head -4       # truncates comm at 15 chars
df -P | awk 'NR>1 && $5+0 >= 90 {print $6, $5}'        # breaks on mount points with spaces
ss -ltnp | awk '{print $4}' | grep -o '[0-9]*$'        # depends on ss's column layout
```

```xsh
{{.tour.host_state.source}}
```

Many tools can already emit JSON; take them up on it. Instead of
`ip -o addr | awk '{print $2, $4}'`, ask `ip` for JSON and validate it against
the fields you need (the [JSON Boundaries](#json-boundaries) section explains
`.require`):

```xsh
{{.tour.ip_addresses.source}}
```

```text
eth0         up       192.168.215.2/24
lo           unknown  127.0.0.1/8
```

The schema names only the fields this script uses; `ip` may add others in
future releases without breaking it, and a release that renames one fails
loudly at `.require` instead of printing an empty column.

When the state you need is only exposed as text, parse it once into a record
and get back to typed values. Finding the CPU-heaviest processes from
`/proc/PID/stat` is a good example. The second field is the command name in
parentheses, and it may itself contain spaces and parentheses, which is why
`awk '{print $14+$15}' /proc/*/stat` quietly reports garbage for
`(Web Content)`:

```xsh
{{.tour.top_cpu.source}}
```

The greedy `(.*)` matches up to the last `)` on the line, so the command name
can contain anything. `guard let` binds an `Ok` value or runs its `else` block,
which here skips processes that vanished mid-scan instead of failing the run.
Over an optional it binds the value that is not `null` the same way:
`guard let user = lookup(id) else { return }`.

## JSON Boundaries

`jq` is a second language you embed as strings inside the first. In XSH, JSON
is just data, with one rule: decoded JSON has type `Any`, and you must check
it against a schema before using its values.

```xsh
{{.tour.json_services.source}}
```

<!-- expected-output -->

```text
{{.tour.json_services.output}}
```

`.require(T)` validates the whole value, nested lists and records included,
and returns `Result[T]`. A missing or mistyped field fails at the boundary with
its path, not three functions later as a confusing `null`. Once validated,
every field access is checked statically.

When the shape is genuinely open, match on it with type patterns instead:

```xsh
{{.tour.json_open_shape.source}}
```

<!-- expected-output -->

```text
{{.tour.json_open_shape.output}}
```

`json.read(path)` and `json.write path (value)` do the same at file
boundaries. Records encode with sorted keys, so output is stable enough to
diff and commit. [`docs/JSON.md`](JSON.md) has the full contract, including
JSON lines.

## Effects

A proc can declare which kinds of side effects it performs:

```xsh
{{.tour.effects.source}}
```

<!-- expected-output -->

```text
{{.tour.effects.output}}
```

The effects are `fs`, `process`, `net`, `env`, `time`, `io`, and `error`
(permission to propagate with `?`). A declared list is an upper bound the
checker enforces through every call. Had `disk_used_kb` claimed only `[fs]`:

```text
err[check.effect-violation]: `run` requires the `process` effect
  disk.xsh:2:13
    let out = run.text du -sk $root ?
              ^^^^^^^^^^^^^^^^^^^^^ `run` requires the `process` effect
```

`pure` functions have no effects at all: no processes, no files, no clock.
They are where parsing and policy belong, and they are trivially testable.
Private procs without a clause get their effects inferred from their bodies;
write the clause where you want a promise, such as exported APIs and anything
that must not touch the network.

## Functions and Inference

The rule of thumb: annotate parameters, let the checker infer the rest.

```xsh
{{.tour.inference.source}}
```

<!-- expected-output -->

```text
{{.tour.inference.output}}
```

Parameters carry types, or defaults that imply them (`over = 80` is an
`Int`). Local bindings rarely need annotations. A private `pure` function
infers its return type from its tail: `usage` returns `Int`, and `hottest`
returns `Result[Disk]` because `first()` fails on an empty list. A private
`proc` whose tail is a value returns `Result[T]`, so `mount_of` returns
`Result[Disk]` and callers use `?` or `??`. A proc whose body ends in a
statement returns `Result[Unit]`. Exported functions, `main`, and recursive
functions declare their return types, because those are promises other files
depend on. A proc that mixes early `return Err(...)` exits with a value tail
still infers `Result[T]`; its error type joins every failure path.

Arguments can be positional, named (`over: 100`), or punned (`over:` passes
the local `over`). Rest parameters (`...hosts: List[Str]`) collect the tail.
The last expression of a block is its value; `return` is for early exits.

## Concurrency: Fan Out, Collect

XSH's unit of concurrency is the process and the pipeline stage, never a
thread or a future. `par-map` runs its block on a bounded worker pool and
returns results in input order:

```xsh
{{.tour.par_map.source}}
```

<!-- expected-output -->

```text
{{.tour.par_map.output}}
```

If a block uses `?` and one item fails, the stage stops scheduling new work
and the error propagates. Leave the `?` off and wrap the work in `try { ... }`
to get a `List[Result[T]]` instead, keeping every success and every failure.

For long-running processes that should overlap with other work, `spawn`
returns a handle and `wait` collects one or a list:

```xsh
{{.tour.spawn_wait.source}}
```

Handles are owned by the scope that created them. If the script fails or is
interrupted before `wait`, XSH terminates and reaps the children; nothing is
left running in the background by accident.

## Cleanup with `defer`

The shell's cleanup story is `trap`:

```bash
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
lock=/run/rotate.lock
touch "$lock"
trap 'rm -f "$lock"' EXIT     # silently replaces the first trap: $tmp now leaks
```

`defer` registers cleanup for the enclosing block. Deferred actions run in
reverse order when the block exits for any reason: normal completion,
`return`, `?` propagation, or a runtime failure.

```xsh
{{.tour.defer.source}}
```

<!-- expected-output -->

```text
{{.tour.defer.output}}
```

`fs.tempdir()` returns a handle to a private directory; pair it with
`defer scratch.close()?` on the next line, as the examples in this tour do.

### Editing a config file safely

`sed -i` on a live config is a classic outage: no backup, no validation, and
nothing happens at all if the line you expected is missing. Do the edit in a
`pure` function, write atomically, and keep a backup:

```xsh
{{.tour.edit_config.source}}
```

<!-- expected-output -->

```text
{{.tour.edit_config.output}}
```

`write_atomic` writes a sibling file and renames it into place, so readers see
either the old config or the new one. In production, validate the candidate
before the rename, for example with `run sshd -t -f $candidate`, then
`run systemctl reload sshd`.

## Timeouts and Retries

Every `run` form accepts `--timeout`. A timeout is a typed error you can match,
not exit status 124 that you have to remember:

```xsh
{{.tour.timeout.source}}
```

<!-- expected-output -->

```text
{{.tour.timeout.output}}
```

`retry` re-runs a block after each delay until it succeeds, and returns the
last error when the delays run out. `on (...)` restricts retries to errors
worth retrying:

```xsh
{{.tour.retry.source}}
```

<!-- expected-output -->

```text
{{.tour.retry.output}}
```

A `Fatal` error would stop immediately without consuming the remaining delays.
Combine the two for the common case of a flaky network command:

```xsh
{{.tour.retry_curl.source}}
```

## Rendering Config from a Template

Generating config with `cat <<EOF` and shell variables means every `$` in the
target format is a quoting hazard. The `template` module renders Go
text/template syntax against ordinary XSH data:

```xsh
{{.tour.template.source}}
```

<!-- expected-output -->

```text
{{.tour.template.output}}
```

`{{"{{"}}.field}}` reads a field, `{{"{{"}}range}}` iterates, `{{"{{"}}if}}` branches, and
`{{"{{"}}template "name"}}` includes a named block. Write the result with
`write_atomic` and the reload story is the same as for the hand-edited config
above.

## A Real Program

Here is a preflight check that decides whether a host is ready for traffic. It
reads a JSON config, probes service ports and health endpoints in parallel,
checks disk headroom and required files, and prints either a table or a JSON
report. It exits 1 if anything failed, so it can gate a deploy.

```xsh
{{.tour.preflight.source}}
```

```text
ok   port 5432 (db)                   postgres
FAIL port 8080 (api)                  nothing listening
FAIL health api                       Connection refused (os error 111)
ok   disk /                           61% used
ok   disk /var                        38% used
ok   file /etc/app/app.conf           present
2 check(s) failed
```

A config with a wrong type stops before any probe runs, and the `ctx` block
says which file was being loaded:

```text
$ xsh preflight.xsh -- bad.json
err: schema: schema check failed at disk_threshold: expected Int, found Str (ctx: loading bad.json)
executable: /usr/local/bin/xsh
...
```

Things worth noticing:

- The only text parsing is `json.read`, and its result is validated once
  against `Config`. Everything after that is typed.
- `svc.health` is `Str?`; after the `!= null` test the checker knows it is a
  `Str`, so `healthy(svc, svc.health)` type-checks.
- `healthy` turns network failures into data (`Check` with `ok: false`) instead
  of propagating them, because an unreachable endpoint is a finding, not a
  crash. `listening` propagates, because failing to read the socket table means
  the check itself is broken.
- The effect clauses document exactly which procs touch the network.
- `abort(1)` exits with a chosen status without a traceback. Deferred cleanup
  still runs.

## Testing

Tests are declarations in ordinary XSH files. `xsht test` discovers
`test NAME { ... }` blocks under the configured test roots, runs each one in a
fresh evaluator with its own temp directory, and reports failures with the
values involved. `assert condition` (with an optional message) is the
assertion.

A small project, with a library module, a script that uses it, and tests for
both:

```ini
# file: {{.project.xsht_config.path}}
{{.project.xsht_config.source}}
```

```xsh
# file: {{.project.lib_sshd.path}}
{{.project.lib_sshd.source}}
```

```xsh
# file: {{.project.bin_harden.path}}
{{.project.bin_harden.source}}
```

```xsh
# file: {{.project.tests_test_sshd.path}}
{{.project.tests_test_sshd.source}}
```

```text
$ xsht test
running 3 tests
tests/test-sshd.xsh::appends_missing_key ... ok 1ms
tests/test-sshd.xsh::replaces_commented_default ... ok 1ms
tests/test-sshd.xsh::harden_script_edits_file_and_keeps_backup ... ok 21ms

test result: ok. 3 passed; 0 failed; 0 skipped
```

The pieces:

- `assert a == b` reports both sides when it fails. Comparisons, `and`/`or`
  chains, and ordering chains all report the operands that made them false.
  A bare `Bool` statement is an error, never a silent no-op, so a forgotten
  `assert` cannot pass vacuously.
- `{ |ctx| ... }` binds the test context. `test.temp_dir(ctx)` and
  `test.temp_file(ctx, ...)` create files under a per-test root that the runner
  removes.
- `test.run_script(ctx, source, args:, env:, stdin:)` runs a whole script under
  `xsh` and returns its status, stdout, and stderr. Child scripts inherit the
  configured `module_path`, so `use sshd` resolves exactly as it does in the
  test file.
- `test.mock(ctx, "net.request", matcher, result)` substitutes host effects such
  as `net.*` and `dns.*` calls. `test.skip("reason")` skips.
- Pure functions like `set_option` need no setup at all, which is a good reason
  to put policy in them.

Useful flags: `xsht test FILTER` runs matching tests, `--nocapture` shows
output live, and `--cov` adds a coverage report.

## Project Layout

The layout above is the canonical one:

```text
ops/
  xsht-config.ini
  bin/          entry scripts (cli main), one per tool
  lib/          modules shared by the scripts
  tests/        test-*.xsh files with test declarations
```

### Modules

Any `.xsh` file is a module. `export` makes a declaration visible; everything
else stays private. A module that exports anything starts with a `##!` doc
block, and every export has a `##` doc comment; `xsht check` enforces both.
Modules may contain declarations and `let`/`const` values, but no top-level
commands or control flow, so importing one never has side effects.

`use NAME` binds exactly one namespace, and nothing is injected into your
scope:

```text
use sshd                  # lib/sshd.xsh, used as sshd.set_option(...)
use checks.disk           # lib/checks/disk.xsh, used as disk.*
use checks.disk as usage  # the same module, bound as usage.*
```

Resolution is file-relative first, then each directory in `XSH_MODULE_PATH`
(colon-separated), then each `module_path` root of the project's
`xsht-config.ini`. That has two consequences worth knowing:

- `xsh` reads `module_path`, and only that, from the nearest
  `xsht-config.ini` above the entry script, exactly as `xsht` does:
  `xsh bin/harden.xsh` finds `lib/sshd.xsh` with nothing else set, and so
  does `cd bin && xsh harden.xsh`. A script with no config above it has no
  project roots in either tool, and the current directory is never searched;
  install such a script's modules next to it or set `XSH_MODULE_PATH` in the
  unit file or wrapper that launches it.
- File-relative lookup wins, so a test file named `tests/sshd.xsh` that says
  `use sshd` imports itself. Name test files `test-*.xsh`.

### `xsht-config.ini`

`xsht` reads the nearest `xsht-config.ini` above each file; `xsh` reads its
`module_path` for the entry script and ignores the rest. Relative paths
resolve from the config's directory. A config that does not decode is an
error for both.

```ini
# Extra files or directories for no-argument `xsht check`, `lint`, `fmt`.
include = tools
# Glob patterns excluded from discovery.
exclude = build/**/*.xsh
  vendor/**/*.xsh
# Module search roots for running, checking, linting, and tests (default: .).
module_path = lib
# Where `xsht test` looks for test declarations.
test_roots = tests

[dead-code]
# Files where unused-function and unreachable-code lints stay quiet.
exclude = examples/**/*.xsh

[coverage]
# Files left out of `xsht test --cov` reports.
exclude = tests/**/*.xsh
```

Multi-value keys continue on indented lines. A `[format]` section with
`line-width = 100` changes the formatter's target width (default 120).

## Formatting and Linting

```bash
xsht fmt                       # format every discovered file in place
xsht fmt --check               # exit 1 if anything would change (CI)
xsht lint                      # report warnings and errors
xsht lint --fix                # apply the safe autofixes
xsht lint --only lint.prefer-named-argument-pun --fix bin/
```

The formatter has one style and no options beyond line width. The linter
knows the language's idioms: it suggests `assert` for a stray `Bool`
statement, named-argument puns, list comprehensions over accumulator loops,
guard clauses, and removal of annotations the checker can already infer.
Every autofix is applied only if the rewritten file still parses and checks
with no new diagnostics. `--only RULE[,RULE...]` restricts both the report and
the fixes, which makes large migrations reviewable one rule at a time.

## Where to Go Next

- [`docs/SPEC.md`](SPEC.md) is the language contract. When this tour and the
  spec disagree, the spec wins.
- `xsht api` answers "what is the signature of...?" without leaving the
  terminal:

  ```bash
  xsht api summary                    # every module, method, and record
  xsht api module:fs                  # one module
  xsht api api:fs.files               # one function, with its contract
  xsht api method:Path.write_atomic   # a method
  xsht api language:core.defer        # a language feature
  xsht api search:timeout             # full-text search
  ```

- [`docs/SPEC-TYPING.md`](SPEC-TYPING.md) covers inference, `Any`, and
  narrowing; [`docs/STREAMS.md`](STREAMS.md) covers pipelines;
  [`docs/JSON.md`](JSON.md) covers JSON; [`docs/SPEC-OS.md`](SPEC-OS.md)
  covers signals and process groups.
- `xsht trace script.xsh` runs a script and shows where the time went: every
  process, its argv as an array, and how long each proc took.
- `showcase/` holds larger programs (`px.xsh`, `ecount.xsh`, `run-retry.xsh`,
  `wait-for.xsh`, and more), and `core/` holds coreutils-style tools written
  in XSH.
