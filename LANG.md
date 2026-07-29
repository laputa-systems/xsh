# XSH Language Proposals

Tracked design changes for the XSH language. Each entry has a rationale and a
before/after example.

## Greppable implementation handles

Proposal lifecycle work should start with these concrete owners:

| Concern | Symbols | Owner and coverage |
|---|---|---|
| semantic contract | `Checker`, `docs/SPEC.md`, `docs-src/CHAPTER-*.md.in` | `src/sema/check.rs`, language contract docs, semantic integration tests |
| deprecation diagnostics | `LintExprVisitor`, `FixHint::replacement`, `FixHint::deletion` | `crates/xsht/src/lint.rs`, `src/diagnostic.rs`; `crates/xsht/tests/lint.rs` |
| standard API registration | `api_spec`, `RuntimeOp` | `crates/xsh-registry/src/signature/*`, `crates/xsh-registry/src/runtime_op.rs`; `tests/fixtures/modules/standard-modules.txt` |
| examples and generated docs | `examples/catalog.json`, `xsht docs build`, `xsht docs check` | `examples/*`, `docs-src/*`, `crates/xsht/src/docs.rs`; docs gate in `docs/TEST-MAP.md` |

`LANG.md` contains open proposals only. Once a proposal reaches
`Checker`/`RuntimeOp`/runtime behavior, move its normative explanation to
`docs/SPEC.md` and keep these handles current.

## Process: Implementing a Proposal

When a proposal is implemented, the commit must do all of the following.

**1. LANG.md** — remove the entry from *Open Proposals*. It no longer belongs
here. The rationale and example live in `docs/SPEC.md` and the relevant
`docs-src/CHAPTER-*.md.in`. LANG.md only tracks things that are not yet done.

**2. SPEC.md** — update `docs/SPEC.md` to describe the new behaviour as
normative. Add to the grammar if it introduces new syntax, add to the type
listing if it introduces a new value kind, update an existing section if it
changes existing behaviour (e.g., deprecations).

**3. Lint rule** — if the feature deprecates an old spelling or pattern, add a
lint rule in `crates/xsht/src/lint.rs` that flags the outdated form. Use
`Severity::Warning` and the `lint.*` code namespace. Follow existing rules as
models.

**4. Autofix** — attach a `FixHint::replacement` or `FixHint::deletion` to
the lint diagnostic whenever the transformation is mechanical. This lets
`xsht lint --fix` migrate existing code automatically without human review.
If the transformation is ambiguous, emit the warning without a fix and document
why in the lint rule comment.

**5. Migrate existing code** — run `xsht lint --fix` on `showcase/*.xsh`,
`examples/*.xsh`, and `tests/xsh/*.xsh` immediately after adding the rule.
Commit the migrated files in the same commit or the next.

**6. Update docs chapters** — edit the relevant `docs-src/CHAPTER-*.md.in`
template to use only canonical patterns. Remove examples that demonstrate the
old form. If the feature is significant enough, add a dedicated section with
its own example.

**7. Add or update an example** — add a focused `examples/*.xsh` file (or
update the closest existing one) that demonstrates the feature. Add or update
the entry in `examples/catalog.json`. The example must be cataloged, pass
`xsht fmt --check`, and produce stable output.

**8. Regenerate docs** — run `./target/debug/xsht docs build` and confirm
`./target/debug/xsht docs check` passes.

**9. Commit message** — reference the LANG.md entry:
`feat: implement implicit main invocation (LANG.md)`

---

## Open Proposals

### `spawn` stderr redirection

**Rationale.** `spawn` accepts `cwd:` and `env:` but has no way to redirect
stderr independently of the parent process. Long-running child processes
such as QEMU emit diagnostics to stderr that are not errors from the
script's perspective — "terminating on signal 15" is QEMU's normal
response to `terminate_if_live`. Without a `stderr:` option, scripts must
either live with the noise or wrap every invocation in `sh -c 'exec "$@"
2>/dev/null' --`, which obscures the actual command, complicates PID
tracking, and defeats lint checks for the wrapped binary name.

**Proposal.** Add an optional `stderr:` parameter to `spawn`:

- `stderr: Path` — redirect child stderr to the given file (truncated).
  `/dev/null` suppresses output.
- `stderr: "inherit"` — keep the current behaviour (default, no change).
- The value is a `Path` or the literal `"inherit"`. A bare `Path` means
  open-for-write, create, truncate, same ownership semantics as
  `unix.spawn_process_group_log`.

**Before.**

```xsh
let qemu = spawn process.command_argv(
  qemu_bin,
  qemu_args,
  cwd: root,
  env: {},
)?
```

QEMU "terminating on signal 15" leaks to the terminal.

**After.**

```xsh
let qemu = spawn process.command_argv(
  qemu_bin,
  qemu_args,
  cwd: root,
  env: {},
  stderr: /dev/null,
)?
```

**Call sites that want this.** `installer-qemu-test.xsh` (two QEMU spawns),
`installer-qemu-manual.xsh` (ditto). Once implemented, replace each
`spawn process.command_argv(...)` with the `stderr: /dev/null` variant
and remove any `# QEMU stderr noise` comments.

### Integer bitset and radix helpers

**Rationale.** XSH is a systems scripting language, and systems scripts
regularly handle bitsets: Unix mode bits, file flags, mount flags, signal masks,
wait statuses, protocol flags, and device metadata. Without first-class bitset
helpers, core applets had to emulate bit operations with division, modulo,
manual bit lists, addition, and subtraction. That is noisy and easy to get
wrong, especially in permission tools such as `chmod` and formatted metadata
tools such as `stat`.

XSH does not need C-style bitwise operator syntax to solve this. Method-shaped
integer helpers fit the existing "methods are the primary API surface" rule and
avoid growing the operator grammar.

**Proposal.** Add explicit `Int` methods for bitset operations and radix
conversion:

- `i.bit_and(mask: Int) -> Int`
- `i.bit_or(mask: Int) -> Int`
- `i.bit_xor(mask: Int) -> Int`
- `i.bit_not(width: Int = default) -> Int`
- `i.has_bit(mask: Int) -> Bool`
- `i.shift_left(bits: Int) -> Int`
- `i.shift_right(bits: Int) -> Int`
- `i.format_radix(base: Int, width: Int = default) -> Str`
- `Str.parse_int(base: Int = 10) -> Result[Int]`

All bit operations should reject negative operands unless a specific operation
defines a finite width. `bit_not` requires either a documented default width or
an explicit width so scripts do not accidentally depend on host integer
representation.

**Before.**

```xsh
pure has_bit(mode: Int, bit: Int) -> Bool {
  return mode / bit % 2 == 1
}

let readable = has_bit(meta.mode, 0o400)
```

**After.**

```xsh
let readable = meta.mode.has_bit(0o400)
let perms = (meta.mode.bit_and(0o777)).format_radix(8, width: 3)
```

### Low-overhead execution profiling and progress tracing

**Rationale.** Long-running XSH programs currently need ad hoc file writes or
`print` statements to expose progress. That works for correctness debugging,
but it is too expensive and too noisy for inner loops such as package graph
walks, archive extraction, or Kbuild parsing. It also makes performance
problems hard to localize: when a scratch build appears to use little CPU, the
operator needs to know whether XSH is blocked on I/O, spending time in parsing,
running user code slowly, or repeatedly executing an unexpectedly hot helper.

XSH should provide a low-overhead way to answer those questions without changing
the program being debugged.

**Proposal.** Add tooling support for sampling and structured progress events:

- `xsht profile COMMAND...` or `xsh --profile COMMAND...` records wall time,
  CPU time, allocation counts if available, and hot XSH source spans/functions.
- `trace.emit(name, fields: record)` records structured events to an optional
  sink when tracing is enabled, and becomes nearly free when disabled.
- `trace.every(name, n, fields: record)` emits at most once per `n` calls, so
  loops can expose useful progress without hand-rolled counters and file I/O.
- The profile output should be readable inside the scratch image with only XSH
  tooling, and copyable to the host for richer analysis.

**Before.**

```xsh
if progress {
  p".xsh-kbuild-progress".write_text("current=${dir.display()} line=${i}\n")?
}
```

**After.**

```xsh
trace.every("kbuild.line", 300, {dir: dir.display(), line: i})
```

### Monotonic timing and run duration API

**Rationale.** Scripts need cheap elapsed-time measurements for long-running
work without shelling out to tools such as `date` and without changing package
or helper schemas just to add a `time` effect. Laputa's Linux iteration loop
exposed this gap: package-side phase markers were useful, but measuring phase
duration inside the package build path either required the `time` effect, which
did not fit the existing PM package export schema, or a process call to `date`,
which was not portable inside the scratch chroot.

XSH should make timing ordinary systems-script data. It should be monotonic for
elapsed durations, wall-clock only when explicitly requested, and available on
process/run results because command duration is one of the first facts users
need during build and package iteration.

**Proposal.** Add a small timing API and extend the run/process result records:

- `time.monotonic_ms() -> Int`
- `time.monotonic_us() -> Int`
- `time.wall_unix_ms() -> Int`
- `time.elapsed_ms(start: Int) -> Int`
- `time.elapsed_us(start: Int) -> Int`
- `Status.duration_ms: Int`
- `Status.duration_us: Int`
- `run.capture`, `run.text`, and related run helpers return output records with
  `duration_ms` and `duration_us` alongside `status`, `stdout`, and `stderr`.

`time.monotonic_*` values are only meaningful for subtraction within one XSH
process. They must not be serialized as wall-clock timestamps. The wall-clock
API is separate so scripts do not accidentally compare system time across NTP
adjustments, suspend/resume, or clock changes.

The run duration fields should be measured by the runtime around the child
process wait and should include the full child lifetime as seen by XSH. They do
not need to include time spent decoding stdout into text after the child exits;
that can be a separate profiling concern.

**Before.**

```xsh
let start = time.now()
make.run_tasks(tasks, jobs)?
print build-stage-done ${time.now() - start}ms
```

or, in schemas where `time` cannot be added:

```xsh
let start = run.text date "+%s" ?
build_stage()?
let finish = run.text date "+%s" ?
print build-stage-done ${finish.trim().parse_int()? - start.trim().parse_int()?}s
```

**After.**

```xsh
let start = time.monotonic_ms()
make.run_tasks(tasks, jobs)?
print build-stage-done ${time.elapsed_ms(start)}ms
```

For command execution:

```xsh
let out = run.capture --text cc "-c" "kernel/fork.c" "-o" "kernel/fork.o" ?
print compile-fork ${out.status.duration_ms}ms
```

### Ambient filesystem policy as a first-class gate

**Rationale.** XSH has started moving host filesystem access behind
capability-oriented APIs, and `tests/ambient_fs_policy.rs` now uses Rust AST
analysis to catch direct ambient filesystem usage in the implementation. That
test is valuable because text scans miss aliases and imports, but it is still a
repository-local convention: the policy lives in a test allowlist, and tightening
it requires manual audit.

The language and toolchain should make ambient filesystem access an explicit
design boundary. Runtime modules, standard library APIs, package tooling, and
tests should be able to state which ambient operations are permitted and why.
As more APIs gain `FsRoot` variants, the policy should shrink rather than rely
on reviewer memory.

**Proposal.** Promote ambient filesystem policy into a maintained tooling
surface:

- Keep an AST-based checker for Rust implementation code that recognizes
  direct paths, imported aliases, and crate-level ambient authorities.
- Add policy categories such as `runtime-boundary`, `cli-host-tool`,
  `test-fixture`, `platform-probe`, and `legacy-awaiting-capability-api`.
- Require each allowlist entry to name a category and a short reason.
- Prefer narrow file/function spans over directory-wide allowlist entries.
- Add a companion `xsht policy ambient-fs` command or cargo xtask so the check
  can be run directly during migration, not only as a Rust test.
- Track the count of allowlist entries by category so broadening the policy is
  visible in review and CI.

This proposal is about implementation hygiene, not a user-visible language
restriction. User scripts still request the `fs` effect; the policy governs the
XSH implementation and standard modules so `fs` can become progressively more
capability-oriented internally.

**Before.**

```rust
const ALLOW: &[(&str, &str)] = &[
    ("src/runtime/", "runtime needs filesystem access"),
];
```

**After.**

```rust
allow!(
    file = "src/runtime/eval/modules/fs.rs",
    category = "runtime-boundary",
    reason = "implements the public fs module and converts Path APIs to capability operations",
);
```

### Structured language-level threads

**Rationale.** XSH already has the concurrency surfaces that matter most for
systems scripting: process fan-out through `spawn`/`wait`, parallel filesystem
walks, and bounded stream stages such as `par-map` and `each --jobs`. Those
should remain the default answer for homogeneous per-item work.

There are still a few orchestration shapes that are awkward without
language-level threads: one task polling for file changes while another waits
on a command, concurrent heterogeneous probes that return different typed
records, or a producer/consumer pair that needs bounded in-process handoff.
Bash background jobs cover some of this at the process level. XSH could cover
the in-language version without becoming an async application runtime if the
feature stays explicit, typed, scope-owned, and cooperatively canceled.

**Proposal.** Add a small structured thread surface backed by OS threads:

- `spawn thread [effects] { ... }` starts one XSH thread immediately and
  returns `Result[ThreadHandle[T], ThreadError]`, where `T` is inferred from the
  block tail value.
- `wait handle` joins a live thread and returns `Result[T, ThreadError]`.
- `wait [h1, h2, ...]` joins distinct live thread handles in input order and
  returns `Result[List[T], ThreadError]` when all handles have the same result
  type.
- `handle.cancel() -> Result[Unit, ThreadError]` requests cooperative
  cancellation. Threads observe cancellation only at evaluator checkpoints and
  through explicit thread cancellation helpers.
- Un-waited thread handles are scope-owned. On scope exit, non-detached handles
  are canceled and joined before cleanup continues. V1 should not have detached
  in-process threads.
- Add explicit sync primitives for the rare cases that need shared state:
  `sync.mutex[T](initial)`, with `.get()`, `.set(value)`, and
  `.update { |value| ... }`; and `sync.channel[T](capacity: Int)` for bounded
  send/receive/close handoff.

Thread blocks run with forked evaluator state. Captured values, returned
values, mutex values, and channel item values must be checker-approved
sendable values. Immutable scalar/container data is sendable when all contained
values are sendable. Ordinary `var` bindings, live streams, `ProcessHandle`,
filesystem locks, dynamic `Any`, and other runtime-owned capabilities cannot
cross the thread boundary unless a future proposal specifies a safe contract.
Dynamic host data must be checked with a schema boundary such as
`.require(Schema)?` before it crosses into or out of a thread.

This is deliberately not a green-thread or async proposal. There is no future
type, no `await`, no scheduler API, no callbacks, no preemptive kill, no
implicit event loop, and no shared ordinary mutation. For per-item parallel
work, `par-map` and `each --jobs` remain the preferred forms. For external
commands, `spawn run` and `wait` remain the preferred forms.

**Before.**

```xsh
var service_report = check_services(config.services)?
var disk_report = check_disk_usage(config.paths)?
var package_report = audit_packages()?

let report = {
  services: service_report,
  disk: disk_report,
  packages: package_report,
}
```

**After.**

```xsh
let services = spawn thread [net, error] {
  check_services(config.services)?
}?

let disk = spawn thread [fs, error] {
  check_disk_usage(config.paths)?
}?

let packages = spawn thread [process, error] {
  audit_packages()?
}?

let report = {
  services: wait services?,
  disk: wait disk?,
  packages: wait packages?,
}
```

For explicit shared progress:

```xsh
let total = sync.mutex[Int](0)

let handles = roots
  |> map { |root|
    spawn thread [fs, error] {
      let count = count_files(root)?
      total.update { |n| n + count }?
    }?
  }

wait handles?
print f"files: ${total.get()?}"
```

### JSON ergonomics v2

**Rationale.** JSON ergonomics v1 deliberately keeps nested access explicit:
paths are segment lists containing string object keys and non-negative integer
list indexes. That keeps the first implementation simple and avoids importing
jq's full query language into XSH. Some common inspection and migration tasks
still want a more compact path spelling and recursive traversal.

**Proposal.** Consider a second JSON layer after the segment-list API has real
usage:

- Dot-string path notation such as `json.get(data, "packages.0.name")`, with a
  specified escaping rule for literal dots and numeric-looking object keys.
- Recursive walk/traversal helpers for applying a block to every object, list,
  scalar, or selected path.
- A small update/remove/map vocabulary that composes with XSH blocks instead of
  embedding jq filters as strings.
- Clear boundaries for when a task should call jq as an external tool instead
  of growing XSH's JSON surface.

This must remain an orchestration helper, not a jq clone.

**Motivating case.** `showcase/jq.xsh` had to hand-roll `getpath`/`setpath`/
`delpaths`/`paths`/`leaf_paths` and recursive `walk`/`..` over its own ordered
`Json` model. A native recursive-traversal + path vocabulary (the second and third
bullets above) would cover the genuinely useful slice of that without an embedded
filter language — and is exactly the "10%" that `showcase/jq-vs-xsh.md` flags as not
yet expressible in native pipelines.

## Rejected Proposals

### Pipeline `reduce` as primary aggregation

**Decision.** Rejected in favour of the for-loop pattern.

The proposal was to promote `reduce(seed) { |acc, x| ... }` as a top-level
pipeline stage for arbitrary aggregation — replacing the for+var accumulation
pattern with a single pipeline expression.

The for-loop is simpler and does not involve pipelines. For cases that need
accumulation (`counts`, `totals`, running state), the explicit `var` + `for`
pattern is clearer and requires no new concepts. `reduce` already exists in the
AST and SPEC for completeness, but it should not be actively encouraged over the
for-loop in documentation or examples.

The `count { key }` feature (now implemented) handles the most common
accumulation pattern (frequency counting) without `reduce`.
