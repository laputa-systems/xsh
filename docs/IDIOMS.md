# XSH Idioms

A reference for idiomatic patterns. These are not rules — they are the forms
that read cleanest and compose most naturally in practice.

## Subcommand dispatch

Use `match` on a string value rather than a chain of `if`/`else if` comparisons.
The `_` arm catches unknown commands with an explicit error.

```xsh
error DispatchError = Unknown(message: Str)

proc main(log: Path, mode: Str) {
  match mode {
    "quick" => {
      append_line(log, "{\"service\":\"quick\",\"event\":\"exit\"}")?
    }
    "heartbeat" => {
      var count = 0

      while count < 200 {
        append_line(log, f"{\"service\":\"heartbeat\",\"event\":\"heartbeat\",\"count\":${count}}")?
        time.sleep(50ms)?
        count += 1
      }
    }
    _ => return Err(DispatchError.Unknown(message: f"unknown mode '${mode}'"))
  }
}
```

Source: `examples/idiom-subcommand-dispatch.xsh`.

When there are three or more string arms and the values form a real enumeration,
consider a tag union instead. The checker issues `lint.stringly-typed-match` as
a prompt.

## Error propagation

Attach `?` to every fallible call. At the top level, an unwrapped `Err` exits
with a non-zero status and prints a traceback.

```xsh
let root = fs.tempdir()?
defer root.remove(missing_ok: true)
let source = fp"${root}/src"
source.mkdir()
```

Inside a proc, `?` short-circuits to the caller:

```xsh
proc epoch_year() [time, error] -> Result[Str] {
  return format_label("year", time.format(0, "%Y", utc: true)?)
}

let label = epoch_year()?
print ${label}
```

Source: `docs/CHAPTER-10-types-records-procs.md` (effect annotations example).

For richer error context, use `.context(kind, message)?` to wrap a failure with
a more specific error kind before it propagates:

```xsh
let argv = process.argv_words(cmd[0]).context("argv-parse", "failed to parse command string")?
```

Source: `showcase/run-retry.xsh`.

## Pipeline vs loop

Use a pipeline when transforming a collection into a new collection:

```xsh
let counts = fs.walk(root)
  |> where .kind == "file" and .ext != ""
  |> map { |entry|
    {ext: entry.ext.lower(), path: entry.path}
  }
  |> group-by .ext
  |> sort-by .key
  |> map { |bucket|
    {count: bucket.items |> count(), ext: bucket.key}
  }
  |> sort-by .count
```

Source: `examples/extension-count.xsh`.

Use `for` when the body has complex side effects, mutates state, or needs an
early `return`:

```xsh
for entry in errors {
  let hits: List[Match] = ip_re.find(entry.message)
  let redacted = ip_re.replace(entry.message, "<IP>")
  print f"error [${entry.module}] IPs found: ${hits.len()}  redacted: ${redacted}"
}
```

Source: `showcase/parse-log.xsh`.

The pipeline form is also natural for `for` when the source needs filtering:

```xsh
for entry in fs.files(src) |> sort-by .path {
  ...
}
```

Source: `showcase/file-report.xsh`.

## Result-propagating map lambdas

`?` inside a `map` block propagates the error from the outer proc. The pipeline
is not materialized — the first failure exits the proc immediately.

```xsh
proc main(root = p".") [fs, error] {
  let entries = fs.files(root)
    |> sort-by .path
    |> map { |entry|
      let p = entry.path
      let sha = hash.sha256(p)?
      let md = hash.md5(p)?
      {name: p.name(), sha256_hex: sha.hex(), md5_hex: md.hex(), size: entry.size}
    }
  ...
}
```

Source: `showcase/dedup.xsh`.

The same applies to `par-map`, but parallel jobs that fail individually produce
an `Err` in the result list — `?` is used after collecting, not inside the
lambda, when parallel failure isolation matters.

## Temporary directories

Always `defer` cleanup immediately after creation so it runs whether the proc
succeeds or errors:

```xsh
let root = fs.tempdir()?
defer root.remove(missing_ok: true)
```

`missing_ok: true` is safe even if a failure inside the proc removed the
directory already. Source: `examples/idiom-temp-dir.xsh`, and throughout the
corpus (`examples/streams.xsh`, `examples/tee.xsh`, `examples/table.xsh`).

## Building maps

Use `group-by` for counting or grouping items by a key:

```xsh
let freq = source.lines()
  |> where .trim() != ""
  |> flat-map { |line|
    line.words()
  }
  |> group-by .
  |> sort-by 0 - .items.len()
```

Each bucket has `.key` and `.items`.

Use `fold` for custom accumulation that does not fit `sum`, `min`, or `max`:

```xsh
let total_chars = entries
  |> map .message.count_chars()
  |> fold(0) { |acc|
    acc + .
  }
```

Use a mutable local `Map[V]` with `{}`, indexed assignment, and
`.get(k, default)` when the keys are dynamic and the map grows inside a loop:

```xsh
var counts: Map[Int] = {}

for entry in entries {
  counts[entry.level] = counts.get(entry.level, 0) + 1
}
```

Source: `examples/idiom-building-maps.xsh`.

For grouped lists, use `Map[List[T]].push(key, value)` to create missing
buckets and append to existing buckets without spelling out the `get`/`set`
cycle:

```xsh
var groups: Map[List[Entry]] = {}

for entry in entries {
  groups = groups.push(entry.level, entry)
}
```

When the map is a direct derivation from a collection or stream, prefer a map
comprehension:

```xsh
let names_by_id = {entry.id: entry.name for entry in entries}
```

## Reading external data

For a JSON file, use `json.read(path)?`. For a JSON string, use
`json.decode(str)?`. Check dynamic data with `.require(Type)?` before
trusting fields:

```xsh
type Package = {name: Str, version: Str}

let raw = if input.display() == "" { json.decode(sample)? } else { json.read(input)? }
let package = raw.require(Package)?
print f"${package.name} ${package.version}"
```

Source: `examples/idiom-reading-data.xsh`.

For structured log or record lines, compile patterns once outside any loop, then
use them inside the pipeline:

```xsh
let log_re = regex.compile("^(\\S+)\\s+(INFO|WARN|ERROR|DEBUG)\\s+\\[(\\w+)\\]\\s+(.+)$")?

let entries = source.lines()
  |> where .trim() != ""
  |> where log_re.captures(.).len() >= 5
  |> map { |line|
    let caps = log_re.captures(line)
    {timestamp: caps[1], level: caps[2], module: caps[3], message: caps[4]}
  }
```

Source: `showcase/parse-log.xsh`.

## Verbose/debug pipeline tracing

Use `tee` to add a side-effect — a `print`, a write, a counter — in the middle
of a pipeline without breaking the flow of items. The `tee` block must return
`Unit`; the stream passes through unchanged.

```xsh
let results = items
  |> tee { |item|
    print f"processing ${item.name}"
  }
  |> map { |item|
    transform(item)
  }
```

`tee` is useful during development to inspect intermediate values. Remove it
when the script is stable; the surrounding pipeline does not need to change.
Source: `examples/tee.xsh`.

## Installing files

Use `fs.install` to copy a file, set its permissions, and create parent
directories in a single call. It replaces the common pattern of `parent().mkdir()`
+ `copy` + `chmod` + `touch`:

```xsh
proc deploy(src: Path, dest: Path) [fs, error] {
  fs.install(src, dest, 0o755, parents: true, overwrite: true)?
}
```

`parents: true` creates any missing ancestor directories. `overwrite: true`
replaces an existing destination. The mode is a Unix permission octal. `install`
sets the mtime implicitly, so no separate `touch` is needed.

Source: `examples/idiom-install-files.xsh`.

## Retry transient work

Use a `retry` block when a fallible operation may fail transiently. The retry
expression returns `Result[T]`, so callers still use `?` explicitly when final
failure should propagate:

```xsh
let body = retry [1s, 2s, 4s] {
  fetch_remote_index()?
}?
```

Inside the block, `?` fails the current attempt rather than returning from the
enclosing proc. A non-empty delay list requires the `time` effect; `retry []`
performs one attempt with no sleep.

Source: `examples/idiom-retry.xsh`.

## guard let

`guard let` binds a result to a name and short-circuits on `Err`, running an
`else` block with access to the error value. The rest of the proc uses the
unwrapped name.

```xsh
proc describe(s: Str) {
  guard let n = to_positive(s) else |e| {
    print f"skipped: ${e.message}"
    return
  }

  print f"${s} -> ${n * 2}"
}
```

Source: `examples/guard.xsh`. Use `guard let` when the error case is a bail-out
and the success case is the main path. Prefer `?` when no side-effect is needed
on failure.

## continue when / break when

One-line loop guards. Both take a boolean expression as a suffix and are
equivalent to `if { continue }` / `if { break }` without the extra nesting.

```xsh
for entry in files |> enumerate() {
  continue when entry.value.size == 0
  break when entry.index >= limit
  # process entry...
}
```

```xsh
loop {
  tries += 1
  break when tries >= max
}
```

`continue when` is also useful for skipping blank lines or comments when
building a map from a text file:

```xsh
for line in text.lines() {
  let t = line.trim()
  continue when t == "" or t.starts_with("#")
  # ...
}
```

Source: `examples/loop.xsh`, `showcase/env-diff.xsh`, `showcase/backup-rotate.xsh`.

## Typed CLI with cli.parse

All scripts with user-facing flags share a consistent structure: a typed record
from `cli.parse`, local usage helpers, and a usage check on empty argv.

```xsh
proc main(...argv: List[Str]) [fs, error] {
  if argv.len() == 0 {
    print "usage: xsh script.xsh -- --pattern PATTERN [--root DIR] [--limit N]"
    return
  }

  let opts = cli.parse(
    argv,
    {
      pattern: {kind: "Str", required: true},
      root: {kind: "Path", default: p"."},
      ext: {kind: "Str", repeated: true},
      verbose: {kind: "Bool", short: "v", default: false},
      limit: {kind: "Int", default: 50},
    },
  )?

  # opts.ext is a List[Str]; opts.verbose is Bool; opts.limit is Int
}
```

Key spec fields: `repeated: true` accumulates all occurrences into a list.
`positional: true` fills the field from positional arguments rather than a flag.
`required: true` errors if the flag is absent. A bare `"Bool"` string is
shorthand for `{kind: "Bool", default: false}`.

Source: `examples/idiom-typed-cli.xsh`.

For Unix-style applets, use `cli.applet` with the same schema shape. It accepts
the compact option forms used by applets and uses the last occurrence for a
non-repeated scalar option; `cli.parse` remains strict and reports duplicate
scalar options. The focused compatibility cases live in
`tests/xsh/stdlib/args.xsh` under `test_cli_applet_*`.

## enumerate for indexed iteration

`enumerate()` wraps each stream item in a record with `.index` (zero-based) and
`.value`. Use it when the loop body needs both the item and its position.

```xsh
for item in files |> enumerate() {
  let marker = if item.index == 0 { "keep" } else { "dup " }
  print f"  [${marker}] ${item.value.path}"
}
```

For one-based line numbers in search output:

```xsh
for item in text.lines() |> enumerate() {
  if re.matches(item.value) {
    print f"${rel}:${item.index + 1}: ${item.value.trim()}"
  }
}
```

Source: `examples/idiom-enumerate.xsh`.

## list comprehensions

Use a list comprehension when a loop only builds another list. It keeps the
result shape at the binding site and avoids a mutable accumulator.

```xsh
let rows = [{stack: key, count: counts.get(key, 0)} for key in counts.keys()]
let names = [pkg.name for pkg in pkgs if ! pkg.optional]
```

Use `for` instead when the body has side effects, multiple branches, or an early
`return`. Source: `examples/idiom-list-comprehension.xsh`,
`showcase/px.xsh`.

## flat-map for tokenization

`flat-map` maps each element to a list and flattens the results into a single
stream. The canonical use is splitting lines into words:

```xsh
let freq = source.lines()
  |> where .trim() != ""
  |> flat-map { |line|
    line.words()
  }
  |> group-by .
  |> sort-by --desc .items.len()
```

Source: `examples/idiom-flat-map.xsh`. Use `flat-map` when the outer element needs to
dissolve into multiple inner items — it replaces a nested `for` loop inside a
`map`.

## any / all predicates

`any` and `all` are pipeline terminals that return a `Bool`. Both accept a
shorthand predicate expression or a block.

```xsh
let has_errors = entries |> any .level == "ERROR"
let all_timestamped = entries |> all .timestamp != ""
```

`any` short-circuits on the first match; `all` short-circuits on the first
mismatch. Source: `examples/idiom-any-all.xsh`.

## sort-by --desc

Add `--desc` to reverse sort order without a trailing `|> reverse`:

```xsh
let newest_first = all_files |> sort-by --desc .path
let largest_first = entries  |> sort-by --desc .size
```

Arithmetic in the key expression lets you compose multi-field keys inline:

```xsh
|> sort-by .parent_pid * 100000000 + .pid
```

Source: `examples/idiom-sort-by-desc.xsh`.

## table.print for columnar output

`table.print` renders a list of records as a fixed-width table. Pass the column
names to show, in display order.

```xsh
process.port(port)?
  |> sort-by .pid * 1000 + .fd
  |> table.print(columns: ["pid", "fd", "user", "command", "protocol", "local", "state"])
```

As a pipeline terminal after collecting:

```xsh
counts |> table.print(columns: ["ext", "files", "lines"])
```

Source: `examples/table.xsh`. Prefer `table.print` over
hand-formatted loops when records already carry named fields.

## f-string field widths

Use `:<N` for left-alignment and `:>N` for right-alignment inside f-string
interpolations. This is the primary tool for tabular output when `table.print`
does not apply.

```xsh
print f"${"file":<48} ${"bytes":>8}  sha256"
print f"${"-":<48} ${"-":>8}  ------"

for e in entries {
  print f"${e.path:<48} ${e.size:>8}  ${e.sha256}"
}
```

Source: `showcase/file-report.xsh`, `showcase/hosts-ping.xsh`. Use this when the
output mixes a free-form column (path, hostname) with numeric columns.

## Dry-run flag

Use a `Bool` `dry_run` option so users can preview what the script would do
without committing changes. The idiom is to compute a label and gate the
mutation behind the flag.

```xsh
let action = if opts.dry_run { "would delete" } else { "delete" }
print f"${action}: ${name}"

if ! opts.dry_run {
  entry.path.remove(missing_ok: true)?
}
```

At the end, split the summary:

```xsh
if opts.dry_run {
  print f"${renamed} files would be renamed  ${skipped} unchanged (dry run)"
} else {
  print f"${renamed} files renamed  ${skipped} unchanged"
}
```

Register it in `cli.parse` as `dry_run: "Bool"`. Source:
`examples/idiom-dry-run.xsh`.

## Matching Result in a loop

When reading files inside a loop, `match` on the result and `continue` on `Err`
to skip unreadable entries without aborting the whole script:

```xsh
for entry in files {
  var src = ""
  match entry.path.read_text() {
    Ok(t) => src = t
    Err(_) => continue
  }
  # process src...
}
```

Use this instead of `?` when a per-item failure should be silent and the loop
should keep going. Source: `examples/idiom-match-result-loop.xsh`.

## Tag unions and structural match

Define a closed enumeration with `type` and a `|`-separated list. Payload
variants carry typed values destructured directly in `match` arms.

```xsh
type Shape = Circle(Int) | Rect(Int, Int) | Point

pure area(s: Shape) -> Int {
  match s {
    Circle(r) => r * r * 3
    Rect(w, h) => w * h
    Point => 0
  }
}
```

Use tag unions when a `match` on a `Str` starts to feel stringly-typed. The
linter emits `lint.stringly-typed-match` as a nudge. Source: `examples/tags.xsh`.

## loop as expression

`loop` can return a value by passing it to `break`:

```xsh
let found = loop {
  i += 1
  if i >= 8 { break i }
}
```

The type of the `loop` expression is inferred from the `break` argument. Use
this when a search needs to return a value without a mutable accumulator and an
early `return`. Source: `examples/loop.xsh`.

## Scoped directory change with cd {}

`cd path { ... }` changes the working directory for the body only, then
restores it when the block exits — whether normally or on error.

```xsh
let before = run.text pwd ?

cd examples {
  run build.sh
}

let after = run.text pwd ?
# after == before
```

Use `cd {}` instead of manual `chdir` calls to avoid leaking the changed
directory. Source: `examples/cd.xsh`.

## Filesystem lock

Use `fs.lock` to serialize concurrent access to a shared work directory.
Always `defer` the unlock immediately after acquiring it so the lock is
released whether the proc returns normally or on error.

```xsh
let lock = fs.lock(fp"${work}/pm.lock")?
defer fs.unlock(lock)?
```

Source: package-manager scripts.

## Finding the git repository root

`fs.gitroot()` walks up from the current working directory until it finds `.git`
and returns that directory as a `Path`. Project-aware scripts use it to anchor
operations to the repository boundary regardless of which subdirectory the user
invoked the script from.

```xsh
let root = fs.gitroot()?
let cfg = fs.read_text(fp"${root}/pyproject.toml")?
```

As a guard that fails fast with a `not-a-git-repo` error when run outside a
repository:

```xsh
let _ = fs.gitroot()?
```

Source: `examples/idiom-gitroot.xsh`, `showcase/secret-scan.xsh`.

## Memoizing expensive calls

`utils.cache(fn, [args...])` calls `fn(args...)` on the first invocation and
caches the result for the lifetime of the process. On every subsequent call with
the same `fn` and argument values, it returns the cached value without
re-evaluating the function.

The cache key is derived automatically from the function name and a
collision-free serialization of the argument values — no key string is needed.

```xsh
proc project_root() [fs, error] -> Str {
  fs.gitroot()?.display()
}

# Only runs project_root once, however many times this appears:
let root = utils.cache(project_root)

# With arguments — key includes both function name and argument values:
pure greet(name: Str) -> Str {
  f"hello, ${name}"
}

let a = utils.cache(greet, ["world"])   # runs greet
let b = utils.cache(greet, ["world"])   # cache hit
let c = utils.cache(greet, ["xsh"])     # different key, runs greet again
```

Use `utils.cache` when the same expensive operation — a filesystem traversal, a
subprocess call, a config file read — is invoked more than once with identical
inputs in the same process. The cache is per-process and not shared across
subprocesses.

Source: `examples/idiom-cache.xsh`.

## Applet-style scripts

For command-like scripts, use `proc main(...argv: List[Str])` when the script
needs BusyBox-style positional parsing or custom flag handling. Core applets are
self-contained by default: keep usage errors and small input helpers local to
the applet. Use a shared `core/lib/*.xsh` helper only for audited applet
families where duplicated parsing or policy would create real drift, such as
auth account and shadow-file handling.

```xsh
proc main(...argv: List[Str]) [error] {
  if argv.len() != 1 {
    return Err(AppletError.Usage("usage: basename PATH"))
  }

  let target = fp"${argv[0]}"
  print ${target.name()}
}
```

Avoid local names that shadow standard modules such as `path`; use names like
`target`, `root`, or `entry` instead. Source: `core/basename.xsh`.

## Testing applet scripts

Native tests for core utility scripts should run the utility through the built
`xsh` binary and assert stdout plus observable filesystem effects. Keep a small
local helper for the binary path so every test uses the same invocation shape.

```xsh
pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_touch(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"${root}/created.txt"
  run.text (xsh_bin()) touch.xsh -- (target.display()) ?
  test.ok(target.exists()?)?
}
```

Prefer temp directories and temp files from `TestContext` for utilities that
mutate the filesystem. Source: `core/tests/test-touch.xsh`.

## Stdin-capable stream applets

When a core utility accepts stdin, keep the `-` and no-file cases in a small
local helper. Utilities can still use native `Str` and structured stream
operations without special cases at each call site.

```xsh
let input = read_text_inputs(paths)?
for line in input.lines() |> sort {
  print ${line}
}
```

Source: `core/sort.xsh`.

## Short flag parsing

Use `cli.parse` for utilities that need BusyBox-style short clusters. Add a
`short` alias to each descriptor; value-bearing options consume the remainder of
the cluster or the next argv item, so `-w0` and `--width=0` both populate
`width`, while `-dc` becomes two boolean fields.

```xsh
let opts = cli.parse(argv, {
  delete: {kind: "Bool", short: "d", default: false},
  squeeze: {kind: "Bool", short: "s", default: false},
  width: {kind: "Int", short: "w", default: 80},
  paths: {kind: "Str", positional: true, repeated: true}
})?
```

This pattern is useful for future core utilities with compact option syntax.

## Guarded process applets

Process-inspection utilities can read from `process.list()`, `system.uname()`,
`unix.id()`, and related modules directly. Process-signaling applets should
default to dry-run output and require an explicit `--apply` before calling
mutation APIs.

```xsh
if apply {
  process.kill(p.pid)?
} else {
  print f"would signal ${p.pid} ${p.command}"
}
```

This pattern is useful for future process-control utilities.

## Guarded Admin Applets

Host-global utilities should parse normal operands, print a dry-run action by
default, and require `--apply` before calling `linux`, `unix`, `user`, or
`group` mutation APIs. Keep read-only display paths runnable on ordinary
development hosts by using stable fallbacks when privileged Linux data is not
available.

```xsh
if wants_apply(argv) {
  linux.reboot()?
} else {
  print "would reboot"
}
```

This pattern is useful for future admin-oriented utilities.
