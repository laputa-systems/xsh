# Writing XSH

XSH is a checked scripting language. `xsht check main.xsh` parses and type-checks the
whole program without running it; `xsh main.xsh -- ARGS...` checks it again and runs it.
A check error exits 2 and nothing runs; a runtime failure exits 3 with a traceback. Look
up exact signatures with `xsht api api:fs.walk`, `xsht api method:Path.write_atomic`,
`xsht api language:core`, or `xsht api search:WORDS`.

## Values

- `let` is immutable, `var` reassignable, `const` fixed at check time. Types are
  inferred; annotate parameters, empty collections and optionals:
  `var seen: List[Str] = []`, `var counts: Map[Int] = {}` (keys are `Str`),
  `var cursor: Str? = null`.
- Types: `Str Int Float Bool Duration Path Bytes List[T] Map[V] T?` and records
  `{name: Str, size: Int}`. Name a record type with `type Row = {...}` before putting it
  inside `List[...]`, `Map[...]` or `?`: `List[{a: Int}]` does not parse. Build records
  with `Row(name: "a", size: 1)` or `{name: n, size: 1}`; `{name, size}` puns locals.
  Both branches of an `if` must have the same type.
- A binding or parameter cannot take a standard module's name: `path`, `map`, `set`,
  `time`, `user`, `group`, `hash`, `bytes`, `json`, `fs`, `env` and `process` are taken
  (`xsht api summary` lists every module).
- Nothing converts implicitly: `"8" + 1` is an error, `"8" as Int` converts or fails,
  `text.parse_int() ?? 0` falls back.
- Strings use double quotes only. `f"{expr}"` interpolates; plain `"..."` never does.
  Lists and records cannot appear in `f"..."`: use `xs.join(",")` or `json.encode(v)?`.
  `print` takes a literal, an f-string, `$name`, or a parenthesized expression such as
  `print (n + 1)`, never a bare name. `eprint` writes to standard error.
- Paths: `p"/etc/hosts"`, `fp"{dir}/{name}"`; methods include `read_text()`, `lines()`
  (a stream to loop over or pipe), `write(text)`, `write_atomic(text)`, `exists()`,
  `is_dir()`, `name()`, `parent()`, `remove()`, `readlink()`, `relative_to(base)`,
  `display()`. Durations: `100ms`, `2s`.

## Results and optionals

Anything that can fail returns `Result[T]`. `expr?` unwraps it or returns the error to
the caller; at top level a failure stops the script. `expr ?? fallback` recovers.
`try { ... }` and `try run ...` turn failures into a `Result` value. Handle one with
`match r { Ok(v) => ..., Err(e) => ... }`, test it with `r is Err(_)`, or bind it with
`guard let v = r else { continue }`. A `Result[Unit]` call in statement position fails
its function when it fails. Optionals: `x ?? d`, `if x != null { ... }` narrows `x`.

Declare failures as a family, `error FetchError = Busy | Gone(url: Str)`; construct
`FetchError.Busy("message")`, match `Err(FetchError.Busy {..})`, read `.message`. An
imported family is qualified, as in `Err(stock.StockError.Timeout {..})`. `return` works
only inside a function; a script ends with `exit N`, also `exit 4 unless ok`.

## External data

Decoded JSON has type `Any`. Validate it before use:
`json.read(path)?.require(Config)?` or `json.decode(text)?.require(List[Row])?`.
`.require` checks the whole value, allows extra object fields, and names the first
mismatch. `json.encode(v)?` writes compact JSON with record keys sorted.

## Functions and entry points

`pure f(x: Int) -> Int { ... }` cannot touch the host; `proc` can. The last expression is
the value, and a `proc` with a value tail returns `Result[T]`. A script reads its
arguments as `args: List[Str]`, or declares them:
`cli main(dir: Path, keep: Int = 3, dry_run = false) { ... }` parses
`[--keep N] [--dry-run] DIR` and exits 2 with usage text on bad input. `use stock` imports
`stock.xsh` from the script's directory; its names are qualified, `stock.lookup(x)`.

## Processes

There is no shell. A value is exactly one argument; words are never split or globbed.
Splice a list with `@name`, binding it first (`let argv = job.argv`, then `@argv`).
Inside a command word `$name` interpolates even within quotes.

| Form | Result |
|---|---|
| `run cmd ...` | statement; fails on a nonzero exit |
| `run.text cmd ...` | stdout as `Str`; fails on a nonzero exit |
| `run.capture --text cmd ...` | `{status, stdout, stderr}`; fails only if it cannot start |
| `try run.capture --text cmd ...` | the same as a `Result` |

Use `out.status.ok` and `out.status.exit_code()?`. Options come first:
`run.text --timeout=2s --accept=[0, 1] grep -c x $file`. Bind a run result with `let`
before using it in an expression. Scopes: `cd (dir) { ... }`, `env ({KEY: "v"}) { ... }`
(a `Map[Str]` works too), `tempdir dir { ... }` (removed afterwards).

## Collections and pipelines

`|>` passes a list or stream through stages and yields a `List`: `where .size > 1`,
`map .name`, `sort()`, `sort-by(desc: true) .date`, `take(n)`, `drop(n)`,
`unique-by .id`, `count()`, `sum()`, `all { .ok }`, `any { . > 8 }`, `group-by .kind`,
`fold(init) { |acc, x| ... }`. A stage block reads its item as `.`; name it
(`{ |row| ... }`) when nesting. `par-map(jobs: 3) { |x| ... }` runs at most 3 blocks at
once and keeps input order. Also: `[x.id for x in xs if x.ok]`, `x in xs`, `xs[1..]`,
`xs += [y]`, `m[k] = v`, `m.get(k) ?? 0`, and `for {key, value} in m` in key order.

## Cleanup, retries, timeouts

`defer { ... }` runs when its block exits, failures included. `path.write_atomic(text)`
writes a temporary sibling and renames it into place. `retry [100ms, 200ms] on
(FetchError.Busy {..} | FetchError.Gone {..}) { fetch()? }` makes at most three attempts,
retries only matching errors, and yields a `Result`; without `on` it retries every
failure. `--timeout=` stops a run form with `ProcessError.Timeout`.

## Filesystem

`fs.children(dir)?` lists one directory, hidden entries included, as records with
`name`, `path`, `kind` (`"file"`, `"dir"`, `"symlink"`) and `size`.
`fs.walk(root, gitignore: false, hidden: true)?` walks a tree without following links;
by default it skips hidden entries and obeys `.gitignore` files. Entry paths begin with
the root you pass: `entry.path.relative_to(root)`.

## Example

```xsh
type Host = {name: Str, port: Int}

pure label(host: Host) -> Str {
  f"{host.name}:{host.port}"
}

let raw = json.decode("""[{"name": "web", "port": 80}, {"name": "db", "port": "5432"}]""")?
match raw.require(List[Host]) {
  Ok(hosts) => print f"{hosts.len()} hosts"
  Err(error) => print f"rejected: {error.message}"
}

tempdir dir {
  let file = fp"{dir}/hosts.txt"
  file.write_atomic("web 80\nmail 25\n\ndb 5432\n")
  var hosts: List[Host] = []
  for line in file.lines()? {
    let fields = line.fields()
    if fields.len() == 2 {
      hosts += [Host(name: fields[0], port: fields[1] as Int)]
    }
  }

  let labels = [label(h) for h in hosts |> sort-by .port]
  let listed = run.text printf "%s\n" @labels
  print (listed.trim())
  print (json.encode(hosts |> where .port < 1000 |> map .name)?)
}
```

```text
rejected: schema check failed at $[1].port: expected Int, found Str
mail:25
web:80
db:5432
["web","mail"]
```
