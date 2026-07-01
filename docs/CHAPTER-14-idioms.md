# Chapter 14: Idioms

This chapter is a cookbook. The earlier chapters teach the language in a
guided order; this chapter answers smaller questions you will hit while writing
real scripts.

Each recipe is intentionally short: when to use the pattern, the runnable
example, and the main judgment call.

## Check A Whole Collection With `any` And `all`

Use this when a later branch depends on whether records contain at least one
match or whether every item satisfies a condition.

```xsh
let entries = [{level: "INFO", timestamp: "2026-01-01"}, {level: "ERROR", timestamp: "2026-01-02"}]
let has_errors = entries |> any .level == "ERROR"
let all_timestamped = entries |> all .timestamp != ""
let has_debug = entries |> any .level == "DEBUG"
print f"${has_errors} ${all_timestamped} ${has_debug}"
```

Prefer this over a manual loop when the answer is a single boolean.

## Build Frequency Maps

Use grouping when you want to keep grouped items available. Use a `Map` when
you need dynamic keys that grow inside a loop.

```xsh
let entries = [{word: "apple"}, {word: "banana"}, {word: "apple"}, {word: "cherry"}, {word: "banana"}, {word: "apple"}]

let groups = entries
  |> group-by .word
  |> sort-by .key

for g in groups {
  print f"${g.key} ${g.items.len()}"
}

let total = entries
  |> map .word.count_chars()
  |> fold(0) { |acc|
    acc + .
  }

print $total
var counts: Map[Int] = {}

for entry in entries {
  counts[entry.word] = counts.get(entry.word, 0) + 1
}

let apple_count = counts.get("apple", 0)
let banana_count = counts.get("banana", 0)
print f"apple=${apple_count} banana=${banana_count}"

var buckets: Map[List[Str]] = {}

for entry in entries {
  buckets = buckets.push(entry.word, entry.word.upper())
}

let apple_bucket = buckets.get("apple")?
print apple_bucket[1]
```

For large inputs where you only need totals, consider `reduce-by` from the
streams chapter.

## Cache Repeated Work

Use `utils.cache` when the same expensive call is made more than once during a
single script run.

```xsh
# Demonstrates utils.cache - memoizes a proc or pure call for the process lifetime.
# The key is derived automatically from the function name and argument values.
proc repo_root() [fs, error] -> Result[Str, Error] {
  let r = fs.gitroot()?
  r.display()
}

# First call runs the proc; subsequent calls return the cached value.
let first = utils.cache(repo_root)?
let second = utils.cache(repo_root)?
print f"root: ${first}"
print (first == second)

# With arguments: each distinct set of args is a separate cache entry.
pure greet(name: Str) -> Str {
  f"hello, ${name}"
}

let a = utils.cache(greet, ["world"])

# cache hit - greet not called again
let b = utils.cache(greet, ["world"])

# cache miss - different key
let c = utils.cache(greet, ["xsh"])
print $a
print (b == a)
print $c
```

Do not use it for values that should change during the same process.

## Add A Dry-Run Mode

Use a `dry_run: Bool` when the same traversal should either report planned
changes or perform them.

```xsh
type Entry = {name: Str}

let sample_entries = [{name: "file-a.txt"}, {name: "_hidden.txt"}, {name: "file-b.txt"}]

proc run_entries(entries: List[Entry], dry_run: Bool) [error] {
  var kept = 0
  var dropped = 0

  for entry in entries {
    if entry.name.starts_with("_") {
      let label = if dry_run { "would drop" } else { "drop" }
      print f"${label}: ${entry.name}"
      dropped += 1
    } else {
      let label = if dry_run { "would keep" } else { "keep" }
      print f"${label}: ${entry.name}"
      kept += 1
    }
  }

  if dry_run {
    print f"${kept} kept  ${dropped} dropped (dry run)"
  } else {
    print f"${kept} kept  ${dropped} dropped"
  }
}

run_entries(sample_entries, true)?
print "---"
run_entries(sample_entries, false)?
```

Keep the decision close to the effect. That makes it harder for the real and
dry-run paths to drift apart.

## Keep Indexes With Items

Use `enumerate()` when output needs human-facing line numbers or stable item
positions.

```xsh
let files = ["alpha.txt", "beta.txt", "gamma.txt"]

for item in files |> enumerate() {
  let marker = if item.index == 0 { "keep" } else { "dup " }
  print f"  [${marker}] ${item.value}"
}

for item in ["line one", "line two"] |> enumerate() {
  print f"${item.index + 1}: ${item.value}"
}
```

If the index is only a loop counter, `range` may be simpler.

## Flatten Nested Lists

Use `flat-map` when each input item produces zero or more output items.

```xsh
let lines = ["the quick", "brown fox"]

for word in lines
  |> flat-map { |line|
    line.words()
  } {
  print $word
}
```

This is the pipeline form of "split every line into words, then continue with
the words."

## Find The Repository Root

Use `fs.gitroot()` when a script should be anchored to the project instead of
the caller's current directory.

```xsh
# Demonstrates fs.gitroot() — walks up from cwd to find the nearest .git directory.
let root = fs.gitroot()?
print f"repo root: ${root.display()}"

# .git exists at the root:
let has_git = fs.exists(fp"${root}/.git")?
print $has_git
```

Use it early as a guard for project-specific scripts.

## Install Files With Parents And Mode

Use `fs.install` when the script needs copy, parent creation, permissions, and
overwrite policy in one explicit operation.

```xsh
let src_root = fs.tempdir()?
defer fs.close_root(src_root)?
let src_dir = fs.root_path(src_root)?
let dest_root = fs.tempdir()?
defer fs.close_root(dest_root)?
let dest_dir = fs.root_path(dest_root)?

fs.write(
  fp"${src_dir}/script.sh",
  """#!/bin/sh
echo hello
""",
)?

let src = fp"${src_dir}/script.sh"
let dest = fp"${dest_dir}/bin/script.sh"
fs.install(src, dest, 0o755, overwrite: true)?
print (dest.exists()?)
```

Prefer this over open-coded copy-plus-chmod sequences when installing files is
the actual job.

## Write List-Comprehension Style Pipelines

Use comprehension syntax for compact list transforms, especially when
destructuring records makes the result clearer.

```xsh
type Package = {name: Str, ver: Str, optional: Bool}

let pkgs: List[Package] = [
  {name: "curl", ver: "8.0", optional: false},
  {name: "jq", ver: "1.7", optional: true},
  {name: "git", ver: "2.40", optional: false},
]

# basic transform
let names = [pkg.name for pkg in pkgs]
print names[0] names[1] names[2]

# with guard
let required = [pkg.name for pkg in pkgs if ! pkg.optional]
print required[0] required[1]

# record destructuring — bind fields directly
let labels = [f"${name}@${ver}" for {name, ver, ..} in pkgs]
print labels[0] labels[1] labels[2]

# destructuring + guard
let optional_names = [name for {name, optional} in pkgs if optional]
print optional_names[0]
```

Use a normal pipeline block when the transform needs multiple statements.

## Continue Through Recoverable Parse Errors

Use `match` on a `Result` when an error should skip one item instead of aborting
the whole script.

```xsh
let inputs = ["1", "bad", "2", "also-bad", "3"]

error ParseError = InvalidDigit(message: Str) : InvalidData

pure to_int(s: Str) -> Result[Int] {
  if s == "1" {
    return 1
  }

  if s == "2" {
    return 2
  }

  if s == "3" {
    return 3
  }

  return Err(ParseError.InvalidDigit(message: f"not a digit: ${s}"))
}

var total = 0

for item in inputs {
  var n = 0

  match to_int(item) {
    Ok(v) => n = v
    Err(_) => continue
  }

  total += n
}

print $total
```

Use `?` instead when the first failure should stop the script.

## Read Structured Data Safely

Use `.require(Type)?` after decoding JSON or other dynamic data.

```xsh
type Package = {name: Str, version: Str, tags: List[Str]}

let sample = "{\"name\":\"demo\",\"version\":\"1.0\",\"tags\":[\"alpha\",\"beta\"]}"
let package = json.decode(sample)?.require(Package)?
print "all fields present"
print f"${package.name} v${package.version}"
```

The JSON syntax being valid does not prove the fields have the shape your script
needs.

## Retry Transient Work

Use a bounded retry loop when the operation is known to fail transiently and the
caller can tolerate waiting.

```xsh
var attempts = 0

error RetryExampleError = Transient(message: Str)

proc fetch_index() -> Result[Str] {
  attempts += 1

  if attempts < 3 {
    return Err(RetryExampleError.Transient(message: f"attempt ${attempts}"))
  }

  "index"
}

let body = retry [0ms, 0ms] {
  fetch_index()?
}?

print f"${body} after ${attempts} attempts"
```

Keep the retry count visible. Hidden infinite retries are bad orchestration.

## Sort Descending

Use a descending key expression when highest values should come first.

```xsh
let files = [{name: "small.txt", size: 1}, {name: "large.txt", size: 100}, {name: "mid.txt", size: 50}]

for f in files |> sort-by --desc .size {
  print $f.name
}

let words = ["banana", "apple", "cherry"]

for w in words |> sort-by --desc . {
  print $w
}
```

For compound ordering, keep the key expression simple enough that a reviewer can
predict the result.

## Dispatch Subcommands

Use `match` on the command string when a script has a small subcommand surface.

```xsh
error DispatchError = Unknown(message: Str)

proc handle(mode: Str) [error] {
  match mode {
    "quick" => print "quick"
    "verbose" => print "verbose"
    _ => return Err(DispatchError.Unknown(message: f"unknown: ${mode}"))
  }
}

handle("quick")?
handle("verbose")?

match handle("unknown") {
  Err(e) => print f"error: ${e.message}"
  Ok(_) => {}
}
```

When the command set becomes a real data model, introduce a tag union or typed
CLI record.

## Clean Up Temporary Directories

Create temp directories with immediate `defer` cleanup.

```xsh
let root = fs.tempdir()?
fs.root_write(root, p"data.txt", "hello")?
let content = fs.root_read_text(root, p"data.txt")?
print $content
fs.close_root(root)?
```

Put the `defer` next to the allocation so later edits do not accidentally place
fallible work before cleanup is registered.

## Parse A Typed CLI

Use typed CLI parsing when options and positionals are part of the script's
public contract.

```xsh
type Opts = {pattern: Str, verbose: Bool, limit: Int}

let argv = ["--pattern", "proc", "--verbose", "--limit", "20"]

let opts: Opts = cli.parse(
  argv,
  {
    pattern: {form: "--pattern PATTERN", required: true},
    verbose: {form: "--verbose", default: false},
    limit: {form: "--limit N", default: 50},
  },
)?

print f"${opts.pattern} ${opts.verbose} ${opts.limit}"
```

Once parsed, pass the typed record through the rest of the script instead of
passing raw `args` around.
