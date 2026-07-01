# Chapter 8: Structured Streams

Structured streams are for data that already has shape. They are separate from
byte pipelines that run external commands, and they let a script transform
lists, records, paths, JSON values, text lines, and byte chunks without
round-tripping everything through untyped text.

By the end of this chapter, you should be able to build a report pipeline,
choose when to materialize a list, aggregate large inputs without keeping every
item, batch argv for process boundaries, and add progress output without
rewriting the pipeline.

## The Pipeline Model

A structured pipeline uses `|>`. Each stage receives the output of the previous
stage, transforms or filters it, and passes the result forward.

```text
source |> stage |> stage
```

A pipeline expression evaluates to a `List[T]` unless it ends in a terminal
stage. Terminal stages produce scalar values: `count()`, `sum()`, `min()`,
`max()`, `first()`, `last()`, `any`, `all`, `fold(init) { ... }`, and
`reduce-by`.

When items are consumed one-by-one and no intermediate list is needed, write a
`for` loop over the pipeline. That lets live sources such as `fs.walk(root)` run
lazily: items are produced and processed as needed, and short-circuiting stages
stop early.

Live sources include filesystem walks, file/text/byte line streams, process
output streams, and user-defined `stream` producers. `where`, `map`,
`flat-map`, `take`, and `drop` preserve laziness when a live stream is consumed
directly by a `for` loop or by a terminal stage. A plain `let rows = source |>
map ...` materializes a reusable list at the binding boundary; use `collect()`
as a pipeline terminal when you want that materialization to be explicit, or
`stream.collect()` when you already have a stream value outside pipeline syntax.

## Define A Lazy Source

Use `stream` when a script needs a named source that produces items on demand.
The signature returns `Stream[T]`, and each `yield` emits one item of that type.

```xsh
type Package = {name: Str, enabled: Bool}

stream manifest_packages(path: Path) [fs, error] -> Stream[Package] {
  for line in path.lines()? {
    let trimmed = line.trim()

    if trimmed != "" {
      let parts = trimmed.split(":")
      yield {name: parts[0], enabled: parts.get(1, "enabled") == "enabled"}
    }
  }
}

let names = manifest_packages(p"packages.txt")
  |> where .enabled
  |> map .name
```

`yield` is valid only inside a `stream` body. Use `return` without a value to
stop early. To delegate another stream, write an explicit loop:
`for item in source { yield item }`.

## Build A First File Report

This script creates a small source tree, walks it, sorts file records, maps
them to names, and then builds parallel summaries.

```xsh
let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let src = fp"${root}/src"
src.mkdir()

fs.write(
  fp"${src}/main.xsh",
  """print "hi"
""",
)?

fs.write(
  fp"${src}/lib.xsh",
  """pure id(value: Str) -> Str { value }
""",
)?

let files = fs.files(root) |> sort-by .name
let names = files |> map .name

let reports = files
  |> par-map { |value|
    f"${value.name}:${value.size}"
  }

print names[0] names[1]
print reports[0] reports[1]
```

The first pipeline returns a `List[Str]`; no explicit collection step is
needed. You can end a pipeline with `collect()` when making that list boundary
visible helps readability. The second uses `par-map`, which defaults to one
worker per CPU. Add `--jobs=N` when the script needs an explicit cap.

Why XSH shines here: stream stages describe data flow without forcing every
transformation through line-oriented text.

Compared with bash and CLI tools: a text pipeline is compact, but every stage
agrees only on bytes or lines. A structured stream lets each stage receive
records, paths, numbers, or JSON values without reparsing the previous stage's
output.

Common trap: use a pipeline when you are transforming data into new data. Use a
`for` loop when each item is mainly being handled for side effects.

## Count, Iterate, And Find Extremes

`range(n)` and `range(start, n)` produce streams of consecutive integers.
Combined with `for`, they replace counter-variable loops for index-based
iteration:

```xsh
for i in range(5) {
  print f"step ${i}"
}

let squares = range(1, 6) |> map { . * . }
print ${squares[4]}
```

To avoid materializing a large file list, iterate directly:

```xsh
let root = fs.cwd()?
for entry in fs.files(root) {
  print f"${entry.size}  ${entry.path.display()}"
}
```

Use terminal stages when the final answer is a scalar:

```xsh
let sizes = fs.files(fs.cwd()?)
  |> map .size

let smallest = sizes |> min()?
let largest = sizes |> max()?
print f"smallest ${smallest}  largest ${largest}"
```

For keyed comparisons, combine `sort-by` with `first()` or `last()`:

```xsh
let oldest = fs.files(root) |> sort-by .modified |> first()?
```

## Aggregate Without Keeping Every Item

`group-by` is useful when you need each group and its items. For large inputs
where you only need totals, use `reduce-by`. It keeps one accumulator per key
instead of holding every item in every group.

```xsh
let by_ext = fs.walk(root)
  |> where .kind == "file" and .ext != ""
  |> reduce-by --sum { |e| {key: e.ext.lower(), value: 1} }

print f"${by_ext.get("rs", 0)} .rs files"
```

`--sum` adds `Int`s and `Float`s, and it adds records field-by-field:

```xsh
let stats = fs.walk(root)
  |> where .kind == "file"
  |> reduce-by --sum { |e| {key: e.ext.lower(), value: {count: 1, size: e.size}} }
```

Add `--jobs=N` only when parallel aggregation is worth the extra complexity.
When tracing is off, XSH can fuse adjacent `par-map |> reduce-by` into one
worker-local aggregation. If `where`, `map`, or `flat-map` stages sit between
them, the pipeline uses the ordinary materialized path.

Do not use `group-by` for large inputs when you only need totals. It keeps every
item in each group. Use `reduce-by` to keep one accumulator per key.

## Keep Simple Stages Simple

Most stages accept a bare expression after the stage name, with `.` standing
for the current item:

```xsh
|> where .kind == "file" and .size > 1024
|> sort-by 0 - .size
|> map .name
```

Use a block when the transformation needs local bindings or more than one
statement:

```xsh
|> map { |entry|
  {name: entry.path.name(), size: entry.size}
}
```

## Batch Work At Process Boundaries

Batching groups stream items before a later stage handles them. This is useful
when work naturally happens in chunks or when an external command has argv
limits.

```xsh
let objects = [p"build/main.o", p"build/lib.o", p"build/cli.o", p"build/test.o", p"build/doc.o"]
let pairs = objects |> batch --count=2
print pairs[0][0].name pairs[0][1].name
print pairs[2][0].name

let _linked = objects
  |> batch --max-argv
  |> each { |chunk|
    run true @chunk
  }

print "link argv ok"
```

The `@chunk` form is explicit argv splicing: take the list value named `chunk`
and pass each item as its own command argument. XSH uses the `@` marker because
expanding a list into argv is a process-boundary operation that should be
visible in code review.

Why XSH shines here: batch size and argv expansion are declared in the pipeline
instead of hidden in manual list bookkeeping.

Compared with bash and CLI tools: `xargs` solves batching by converting text
back into argv. XSH batches values directly and makes the `@` argv expansion
visible at the process boundary.

## Adapt Text, Bytes, And JSON

Adapters bridge common boundaries into structured streams. Text can become
lines, bytes can become fixed-size chunks, and JSON-lines input can become
records.

```xsh
let files_text = run.text printf "%s\n" "src/main.xsh" "src/lib.xsh" ?

let paths = files_text
  |> text.lines
  |> map { |line|
    fp"${line}"
  }

print paths[0].ext paths[1].name
let chunks = b"print ok\n" |> bytes.chunks(2)
print (chunks[0] == b"pr") (chunks[3] == b"ok")

let records = """{"name":"small","size":1}
{"name":"large","size":4}
"""
  |> json.lines
  |> sort-by .size

print records[1].name records[0].size
```

Why XSH shines here: boundary conversions are explicit stages, so later stages
can work with typed values.

## Add Progress Without Changing The Data

`tee` inserts a side-effect block into a pipeline without changing the items
flowing through it.

```xsh
let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
fs.write(fp"${root}/alpha.txt", "aaa")?
fs.write(fp"${root}/beta.txt", "bb")?
fs.write(fp"${root}/gamma.txt", "c")?

let sizes = fs.children(root)
  |> where .kind == "file"
  |> sort-by .path
  |> tee { |entry|
    print f"visit ${entry.path.name()}"
  }
  |> map .size

print sizes[0] sizes[1] sizes[2]
```

The block receives each item, prints progress, and returns `Unit`. The stream
continues unchanged and the final `map` still builds the size list.

Use `tee` for progress reporting during development or long-running scripts.
Remove it when the script no longer needs that output; the surrounding pipeline
does not need to change.

Do not leave noisy `tee` output in scripts whose stdout is a machine-readable
contract. Send progress somewhere explicit or remove the tap.

## Table Output And Surface Survey

`table.print` is a terminal stage that renders a record stream as a
terminal-width UTF-8 table. Columns are named explicitly to control order and
visibility.

```xsh
let scratch_handle = fs.tempdir()?
defer fs.close_root(scratch_handle)?
let scratch = fs.root_path(scratch_handle)?
let mode = 0o755
let label = f"mode ${mode}"
let raw_lines = run.stream --text printf "%s\n" alpha beta gamma

let lines = raw_lines
  |> drop(1)
  |> take(1)

let shuffled = [1, 2, 3] |> shuffle(7)

error FsError = NotFound(message: Str) : NotFound

let _ = process.command {
  timeout = 2s
  run --timeout=1s echo ok
}

match Err(FsError.NotFound(message: "missing")) {
  Err(FsError.NotFound {message: _}) => print ${mode == 493} ${"493" in label} lines[0] ${shuffled |> count()}
}
```

The example demonstrates table output alongside the compact stream surface:
filtering, mapping, sorting, grouping, terminal checks, and record-shaped
results.

The reference manual lists every stage. In tutorial code, prefer the smallest
stage vocabulary that makes the data flow obvious.

## What You Know Now

Structured streams are the natural form for transforming shaped data. Materialize
a list when later code needs a list. Iterate lazily when each item is handled
once. Use terminal stages for scalar answers, `reduce-by` for large
aggregations, `batch` and `@` at process boundaries, and `tee` for side-effect
taps.
