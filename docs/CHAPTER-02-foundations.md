# Chapter 2: Language Foundations

This chapter starts with the smallest useful pieces of the language. It assumes
only that you can put text in a file and run it with `xsh`.

XSH scripts are ordinary source files. Top-level statements run in order.
Commands perform effects, expressions compute values, and the language keeps
argv, paths, and failures visible instead of smearing them into strings.

The chapter is still introductory, but it is deliberately not tiny. A first
script needs more than `hello world`: it needs values, lists, records,
arguments, functions, failures, and ordinary control flow.

## A Complete Script

The smallest useful XSH program can be one command:

```xsh
print "hello"
```

`print` is a core command. It does not need an import, a `main` function, or a
special file header.

That is already a complete program. Larger scripts use the same rule:
statements at the top level run from top to bottom.

## Commands, Words, And Values

XSH keeps command-shaped code for effects because commands are a natural way to
describe system work:

```xsh
print "hello"
```

The quoted word is passed as an argument to `print`.

When a command argument should come from an expression, write the expression in
`${...}`:

```xsh
let name = "world"
print ${name}
```

The braces mean "evaluate this XSH expression here." They do not mean "split
this text like a shell would." The value becomes one argument unless a later
chapter introduces an explicit argv-splicing form.

Compared with bash and CLI tools: shell interpolation usually produces text
that is then split again by shell rules. XSH interpolation inserts one value at
one boundary. If you want list expansion, you write the explicit `@` form later.

Expression strings and command words are intentionally different. Inside an
expression, `"hello ${name}"` is just a string containing those characters. Use
an `f"..."` display string when an expression needs interpolation:

```xsh
let name = "world"
let message = f"hello ${name}"
```

Display strings can be nested because interpolation scans balanced
expressions:

```xsh
let wrapped = f"outer ${f"inner ${name}"}"
```

Use `\${` when a display string or quoted command word needs a literal
interpolation marker:

```xsh
let template = f"prefix=\${prefix}"
```

Use triple quotes for strings that need real newlines. Triple-quoted strings
work for ordinary strings and display strings:

```xsh
let plain = """first line
second line"""
let shown = f"""package ${name}
ready"""
```

Inside a command word, `${name}` already means "insert this value as one argv
argument":

```xsh
print ${message}
```

## Simple Values

XSH has ordinary scalar values:

```xsh
let ok = true
let count = 3
let ratio = 2.5
let delay = 250ms
let text = "alpha"
let root = p"/usr/local"
```

`Bool` values are `true` and `false`. `Int` values are integers. `Float`
values are written with a decimal point or exponent and are used for measured
quantities such as ratios and rates. `Duration` values are written with units
such as `ms`, `s`, `m`, and `h`. `Str` is UTF-8 text. `Path` is a filesystem
path value.

Paths are not strings with a convention. A path literal uses `p"..."`, and
obvious path literals such as `/usr/bin` or `./target` can be written without
the `p` prefix. Keeping paths distinct lets filesystem APIs reject invalid path
data instead of discovering it after a command has already been built.

At trust boundaries such as CLI parsing, `path.absolute(path)?` makes a path
absolute against the current XSH cwd and normalizes `.` and `..` components
without requiring the target to exist.

```xsh
let cwd = fs.cwd()?
let dest = path.absolute(p"target/../target/package")?
let same = dest == fp"${cwd}/target/package"
print $same
```

## Lists And Records

Lists keep ordered values:

```xsh
let names = ["base", "core", "extra"]
print ${names[0]}
```

Indexes start at zero. A list type is written as `List[T]`, so `List[Str]`
means "a list whose items are strings." Most local lists do not need an
annotation because the checker can infer the item type from the literal.

Records keep named fields:

```xsh
let pkg = {name: "xsh", root: p"/usr/local", enabled: true}
print ${pkg.name} ${pkg.root.name}
```

This is only a record value. Named record types and reusable record schemas
come later, after the basic shape is familiar.

```xsh
let name = "world"
let greeting = f"hello ${name}"

let banner = f"""${greeting}
from xsh"""

let roots = [/usr, /usr/local]
let scores = [2, 3, 5]
let ratio = scores[2].float() / 2.0
let release = {name: "xsh", root: roots[1], enabled: true}
print banner.lines().collect()[0]
print scores.len() ${scores[0] + scores[1] + scores[2]}
print ratio.format(precision: 2) (ratio.floor()?)
print roots[0].name $release.root.name $release.enabled
```

The example combines nested display strings, a multiline display string, a
list of paths, a list of integers, a float conversion, a record value, indexing
with `[]`, field access with `.`, and the `.len()` method on lists. The `roots`
binding is annotated as `List[Path]`: `/usr` and `/usr/local` are obvious path
literals, not strings. Standard helpers are available without imports; the
guide introduces them as they become useful.

## Script Arguments

Arguments passed after `--` are available as `args: List[Str]`. A list is an
ordered value, not a string that needs to be split again.

```xsh
for arg in args {
  print $arg
}
```

Run the script with arguments such as `one` and `two`. The loop receives exactly
those two strings and prints them in order.

This is one of the first differences from traditional shell: arguments are
already data.

Compared with bash and CLI tools: `"$@"` is a convention you must remember.
`args: List[Str]` is the default shape, so a script starts with argument
boundaries preserved.

## Bindings

Use `let` for a binding that will not change:

```xsh
let root: Path = p"/tmp"
let names: List[Str] = ["base", "core"]
```

Use `var` when a value is meant to change:

```xsh
var count = 0
count += 1
```

A binding can include an explicit type after `:`. Use that when the type is
part of the contract you want the reader or checker to see. For short local
values, inference is usually enough.

## Pure Functions

XSH has `pure` functions, not a separate `func` keyword.

A `pure` function is called from an expression:

```xsh
pure identity(value: Str) -> Str {
  value
}

let value = identity("ok")
```

It must declare its return type, and it is effect-free by contract. A pure
function can compute values and call other pure functions. It cannot run
external commands, call effectful procs, mutate variables, read ambient process
state, glob the filesystem, or perform host operations that the checker marks
as effectful.

That restriction is useful in system scripts. When code is marked `pure`, a
reader knows it is not secretly changing directories, changing the environment,
starting a process, or touching the filesystem.

Do not use `pure` for work that needs host state. A helper that reads files,
looks at environment variables, or starts processes should be a `proc` so the
effect is visible.

`proc` is the other abstraction form. Procs are for command-shaped or
effectful work and are introduced later, after external process execution is
clear. If you are coming from a language with `func`, read XSH's `pure` as
"function, with an explicit no-effects contract."

## Results And `?`

Expected failures are represented with `Result`. A `Result[T]` is either an
`Ok` value of type `T` or an `Err` value describing the failure.

The postfix `?` operator unwraps success. If the value is an error, the current
script, pure function, or proc returns that error.

```xsh
pure label(value: Str) -> Result[Str] {
  value
}

let value = label("ok")?
print $value
```

The `label` function is pure and returns `Result[Str]`. The final expression in
the function body is `value`; because the declared return type is a `Result`,
XSH wraps that successful value as `Ok(value)`.

The call `label("ok")?` means: continue with the string if the call succeeds,
or stop here with the error if it fails.

Failure flow is visible at the call site, but the common path stays small.

## Variables And Control Flow

Loops and conditionals use braces. The same block shape appears throughout the
language.

```xsh
var tries = 0

while tries < 4 {
  tries += 1

  if tries == 2 {
    print "two"
    continue
  }

  break when tries == 4
  print $tries
}
```

This example is intentionally small. `while` repeats while its condition is
true, `if` chooses a block, `continue` starts the next loop iteration, and
`break` leaves the loop.

## Loop Expressions And `guard let`

`loop` is an unconditional loop. It can return a value when `break` carries
an argument, making the whole `loop { ... }` an expression. Inline `break when`
and `continue when` are one-line guards that keep the body flat.

```xsh
var tries = 0

loop {
  tries += 1
  break when tries >= 3
}

print $tries
var i = 0

while i < 5 {
  i += 1
  continue when i == 3
  print $i
}

let found = loop {
  i += 1

  if i >= 8 {
    break i
  }
}

print $found
```

`guard let` binds the success value of a `Result` expression to a name. On
`Err`, it runs an `else` block with the error and then returns from the
enclosing proc. On `Ok`, execution continues with the bound name in scope.

```xsh
error ParseError = InvalidPositive(message: Str) : InvalidData

pure to_positive(s: Str) -> Result[Int] {
  if s == "5" {
    return 5
  }

  if s == "10" {
    return 10
  }

  return Err(ParseError.InvalidPositive(message: f"not a known positive: ${s}"))
}

proc describe(s: Str) {
  guard let n = to_positive(s) else |e| {
    print f"skipped: ${e.message}"
    return
  }

  print f"${s} -> ${n * 2}"
}

describe("5")
describe("bad")
describe("10")
```

`guard let` is an alternative to a `match`-plus-`return` when the error case
is a bail-out and the rest of the proc needs the unwrapped value.

## Summary

XSH starts with top-level commands, real scalar values, lists, records, visible
expression insertion, pure functions, `Result` flow, and ordinary block-based
control flow.

The next chapter explains the command-line tools used while writing, checking,
formatting, running, and tracing scripts.
