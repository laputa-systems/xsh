# Chapter 10: Types, Records, And Procs

Small XSH scripts can stay mostly top-level. As a script grows, repeated data
shapes deserve names and repeated behavior deserves a pure function or proc.

By the end of this chapter, you should know when a type helps, how to parse a
CLI into a checked record, how tag unions remove stringly-typed branches, and
how effect annotations make host boundaries visible.

## Name Repeated Shapes

Type aliases and record schemas make scripts easier to read as they grow.
Procs can take typed parameters, defaults, and rest arguments while still being
called in a command-shaped style.

```xsh
type PackageName = Str

type Package = {name: PackageName, root: Path, files: List[Path]}

let demo_pkg: Package = {name: "demo", root: p"src", files: [p"src/main.c"]}

proc describe(pkg: Package, prefix: Str = "pkg", ...labels: List[Str]) {
  print $prefix $pkg.name $pkg.root.name

  for label in labels {
    print $label
  }
}

describe(demo_pkg)
describe(demo_pkg, "named", "extra")
```

The record type names the package shape once. The proc then uses that shape at
every call and can still accept command-like optional arguments.

Why XSH shines here: a script can stay lightweight at the top level and gain
explicit contracts exactly where repeated data shapes appear.

Compared with bash and CLI tools: shell functions pass strings and rely on
calling conventions. XSH procs can still read like commands, but records and
types make the contract visible to the checker and reviewer.

Common trap: do not annotate every local binding just to look serious. Add a
named type when the shape crosses a boundary or appears in more than one place.

## Parse A CLI Into Records

Automation often starts as a script and grows into a command. The `args` module
parses option records and command-shaped input without leaving XSH.

```xsh
type BuildOptions = {root: Path, jobs: Int, define: List[Str], verbose: Bool}

type Cli = {command: Str, action: Str, root: Path, raw: List[Str]}

let opts: BuildOptions = cli.parse(
  args,
  {
    root: {
      form: "--root PATH",
      default: p"dest",
    },
    jobs: {
      form: "-j --jobs N",
      default: cpu.count(),
    },
    define: {
      form: "-D --define NAME=VALUE",
      repeated: true,
    },
    verbose: {
      form: "-v --verbose",
      default: false,
    },
  },
)?

let line = "WARN build.rs: unused value"
let word_re = regex.compile("unused|missing")?
let capture_re = regex.compile("^(\\w+) ([^:]+): (.*)$")?
let whitespace_re = regex.compile("\\s+")?
let warn_re = regex.compile("WARN.*unused")?
let matches = word_re.find(line)
let captures = capture_re.captures(line)
let rewritten = whitespace_re.replace(line, "|")

let command_specs = {
  build: {
    positionals: [
      "root",
    ],
    types: {
      root: "Path",
    },
    rest: "raw",
  },
  clean: {
    positionals: [
      "root",
    ],
    types: {
      root: "Path",
    },
    rest: "raw",
  },
}

let parsed_cli: Cli = cli.commands(
  ["deploy", "target/demo", "--dry-run"],
  rootless_default: "build",
  commands: command_specs,
  fallback_command: {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw", command_like: true},
)?

print $opts.root.name $opts.jobs opts.define.len() $opts.verbose
print warn_re.matches(line) captures[1] captures[2] matches[0].text $rewritten
print $parsed_cli.command $parsed_cli.root.name parsed_cli.raw.len() $parsed_cli.action
```

The example parses build options, uses regexes for extraction and replacement,
and parses command-shaped input into a typed `Cli` record.

Why XSH shines here: CLI parsing, regex extraction, replacement, and typed
command records can live beside the workflow that consumes them.

Output note: this example is run with cataloged arguments that set `--root`,
`--jobs`, repeated `--define` values, and `--verbose`.

Do not keep passing raw `args` after parsing succeeds. Convert once into a
record, then pass that record through the script.

## Use Tag Unions For Real Cases

A record type is right when values always have the same fields. A tag union is
right when a value is one of several distinct cases, each with its own shape.

```xsh
type Level =
    Info
  | Warn
  | Error
  | Debug
  | Trace

type Shape =
    Circle(Int)
  | Rect(Int, Int)
  | Point
  | Square(Int)
  | Triangle(Int, Int)

pure level_label(l: Level) -> Str {
  if l == Info {
    return "INFO"
  }

  if l == Warn {
    return "WARN"
  }

  if l == Error {
    return "ERROR"
  }

  if l == Trace {
    return "TRACE"
  }

  "DEBUG"
}

pure area(s: Shape) -> Int {
  match s {
    Circle(r) => r * r * 3
    Rect(w, h) => w * h
    Point => 0
    Square(side) => side * side
    Triangle(base, height) => base * height / 2
  }
}

print level_label(Info) level_label(Error)
print area(Circle(4)) area(Rect(3, 5)) area(Point)
```

The checker warns when a `match` does not cover all declared variants. That is
the main benefit: adding a variant later produces a check-time notice at every
match that needs updating.

Stringly-typed patterns with three or more string-literal match arms are flagged
by `lint.stringly-typed-match` as candidates for a tag union.

Why XSH shines here: the type system catches the "forgot to handle the new
case" bug before the script reaches production data.

Do not use a tag union for a value that is truly open-ended host data. Use a tag
union when the script owns the set of cases or when the set is part of a stable
contract.

## Put Effects In The Signature

A proc signature can declare exactly which side effects its body is allowed to
produce:

```xsh
proc read_config() [fs, error] -> Result[Config] { ... }
proc build() [fs, process, error] -> Result[Status] { ... }
proc get_time() [time] -> Int { ... }
```

The named effects are `fs`, `net`, `process`, `env`, `time`, `io`, and
`error`. Specific effects are preferred over broad `io` when they keep the
contract readable.

```xsh
pure format_label(kind: Str, value: Str) -> Str {
  return f"${kind}: ${value}"
}

proc epoch_year() [time, error] -> Result[Str] {
  return format_label("year", time.format(0, "%Y", utc: true)?)
}

let label = epoch_year()?
print $label
```

The checker enforces the annotation. A `[time, error]` proc cannot read files
or start processes. A `pure` function has no effects at all and can be called
from any annotated proc.

Do not add effect annotations to hide broad behavior behind `[io]` everywhere.
Use specific effects when the signature is meant to help review.

## Keep Pure Code Practical

A pure function can still use local scratch mutation when the computation is
deterministic and effect-free.

```xsh
error ParseError = Invalid(message: Str)

pure octal_mode(raw: Str) -> Result[Int] {
  var mode = 0

  for ch in raw.split("") {
    var digit = 0

    match ch {
      "0" => digit = 0
      "1" => digit = 1
      "2" => digit = 2
      "3" => digit = 3
      "4" => digit = 4
      "5" => digit = 5
      "6" => digit = 6
      "7" => digit = 7
      _ => return Err(ParseError.Invalid(message: f"invalid octal digit '${ch}'"))
    }

    mode = mode * 8 + digit
  }

  return mode
}

let mode = octal_mode("755")?
print $mode
```

Only `var` bindings declared inside the same pure function can be assigned.
Parameters, `let` bindings, top-level values, imported values, record fields,
and indexed containers remain immutable from pure code.

## What You Know Now

Add types where they document a boundary: CLI records, JSON schemas, metadata
passed between helpers, or proc inputs that belong together. Use `pure` for
effect-free computation, `proc` for orchestration, tag unions for real cases,
and effect annotations when a script's host boundaries should be reviewable.
