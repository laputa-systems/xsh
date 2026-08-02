This is the user-facing reference for the isolated XSH gym. The agent runs as
`root` in a minimal Alpine Linux container with its task workspace mounted at
`/work`.

The base image has BusyBox utilities, `xsh`, `xsht`, `curl`, and CA
certificates. A task image may add only the utilities named by that task (the
`ecount` image adds `fd`). There are no compilers, toolchains, Git checkout,
or other language runtimes. Use HTTPS through `curl` only when the task allows
network access; do not depend on the host or on the XSH repository being
present.

The stable data tree used by the ecount task is `/usr/share`. It belongs to the
container image, so the task does not depend on the host checkout path.

The available program tools are:

    xsh SCRIPT [ARGUMENT...]
    xsht check SCRIPT
    xsht fmt SCRIPT
    xsht lint SCRIPT
    xsht api [QUERY...]

You may use the available BusyBox utilities for editing files, inspecting task
inputs, and running an evaluator’s oracle. Whether a utility may be used in
the submitted XSH solution is specified by the task.

## Source and entry points

An XSH file can contain top-level values and procedures. A command-line
program commonly exposes a main procedure:

    proc main(...argv: List[Str]) [effects] {
      ...
    }

The spread parameter receives the script arguments as a list. A task may
define a more specific procedure signature when it needs one.

Bind values with let:

    let name = "world"
    let answer = 40 + 2

Values have explicit types. Common types include Str, Int, Bool, Path,
List[T], Map[T], and Result[T]. Records have named fields, accessed with dot
syntax:

    let name = entry.name
    let path = entry.path

## Effects and errors

Host operations declare effects on the procedure that uses them. Filesystem
work normally requires fs. An operation that can return an expected failure
returns Result data; postfix ? propagates that failure from a procedure whose
effects include error:

    proc read_name(path: Path) [fs, error] -> Result[Str] {
      let entry = fs.metadata(path)?
      return entry.name
    }

Use the exact return type and effect information shown by `xsht api`. Do not
turn an expected host failure into an unchecked assumption.

## Paths and filesystem values

Path literals use the p prefix:

    let root = p"/tmp"

Filesystem APIs accept Path values. For recursive file discovery, inspect:

    xsht api api:fs.files
    xsht api api:fs.walk

The filesystem stream entries expose structured fields such as kind, ext,
name, and path. A regular-file filter is normally expressed by checking the
kind field. The API contract, not a guessed field name or string convention,
is authoritative.

## Streams and collections

Filesystem discovery returns a lazy stream. Stream stages compose with the
pipeline operator:

    let files = fs.files(root)
      |> where .kind == "file"
      |> map { |entry| entry.path }

List values pipe into the same stages directly. The pipeline result is a
lazy stream until a terminal such as collect is applied, and print rejects an
unconsumed stream:

    let lowered = argv
      |> map { |a| a.lower() }
      |> collect()
    let joined = lowered.join(", ")

Common stages include where, map, sort-by, and terminals such as collect and
count. Query their language references when the stage’s block or ordering
semantics matter:

    xsht api language:stream
    xsht api language:stream.sort-by
    xsht api language:stream.fold

Maps and lists are values. Map.set returns an updated map value, and Map.get
has a fallback overload:

    let next = counts.set("x", 1)
    let current = next.get("x", 0)

Use `xsht api method:Map.set`, `method:Map.get`, `method:Map.keys`, and the
corresponding List queries for exact signatures.

## Text and output

Text methods are explicit. For example, Str.lower returns a new lowercase
string, and Path.ext reads a path extension without reading file contents:

    let lower = text.lower()
    let extension = path.ext()

print writes values to standard output. Use explicit value interpolation or
print separate values when the output contract requires a particular layout:

    print "count" $count

For an exact-output task, preserve required spaces, leading padding, and final
newlines. Do not add explanatory output.

## Process boundary

XSH has explicit process APIs, but a task may forbid subprocesses. When it
does, perform the work through typed XSH values and host modules only. A
subprocess prohibition includes run, process APIs, spawn, and external shell
commands.

## Development loop and tooling

`xsht api` is the live reference available inside the gym. Start with the
onboarding guide:

    xsht api

Then query an exact module function, method, or language rule:

    xsht api api:fs.files
    xsht api method:Path.ext
    xsht api method:Str.lower
    xsht api language:stream.sort-by

Use module and language prefixes for an overview:

    xsht api module:fs
    xsht api language:stream

Exact results show the purpose, contract, effects, signature, tags, and an
example when one is useful. Treat the displayed signature and contract as the
source of truth for a task.

The pinned gym image may predate the `api` subcommand, in which case
`xsht api` reports `unknown command 'api'`. Confirm what the installed build
actually supports with `xsht --help` before relying on any command. When `api`
is absent, discover method names by trial and error: write the smallest script
that calls the candidate method and run `xsht check` until it accepts. For
example, the verified list-length method in this image is `List.len()`
(`argv.len() == 0`), while `length()`, `size()`, `count()`, and `is_empty()`
are rejected as unknown methods.

After creating or changing a script, use this loop:

    xsht check SCRIPT
    xsht fmt SCRIPT
    xsht lint SCRIPT
    xsh SCRIPT ARGUMENT...

Run `xsht check` after any substantive edit. Formatting is part of the normal
source workflow; lint reports likely mistakes but does not replace checking.

When output is an acceptance contract, capture the script’s stdout and compare
it with the task’s oracle. Keep diagnostics and explanatory text off stdout.

Use BusyBox tools only within the permissions and restrictions stated by the
task. They may help edit a file or inspect an oracle, but using a utility from
inside the XSH solution is a separate design choice and may be forbidden. Do
not search for hidden source, repository examples, or implementation details.
The intended path is this handbook, xsht api discovery, xsht feedback, and a
small XSH program.
