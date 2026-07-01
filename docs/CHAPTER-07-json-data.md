# Chapter 7: JSON And Data Boundaries

JSON, DNS, and HTTP connect a script to files, services, and the network. Those
are trust boundaries. XSH code should keep data structured while it is being
computed, then cross those boundaries deliberately.

By the end of this chapter, you will have a small metadata workflow: write a
record to JSON, read it back, check the shape before trusting it, write
JSON-lines events, and handle a network error as data.

## Write And Read A Checked JSON File

Records, maps, lists, strings, booleans, integers, finite floats, and encoded
bytes can be written to JSON. Values read from JSON are dynamic until they
cross a schema check boundary such as `.require(Type)?`.

```xsh
type Metadata = {name: Str, root: Str, digest: Str, ok: Bool, error: Str}

type Event = {service: Str, event: Str}

let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let out = fp"${root}/metadata.json"
let lines = fp"${root}/events.jsonl"
let status = run.status false
let error_message = "shown"
let metadata = {name: "demo", root: p"src".display(), digest: b"abc".base64(), ok: status.ok, error: error_message}
json.write(out, metadata, pretty: true)?
let decoded = json.read(out)?.require(Metadata)?
let events = [{service: decoded.name, event: "start"}, {service: decoded.name, event: "stop"}]
json.write_lines(lines, events)?
let decoded_events = lines.read_text()? |> json.lines
let first = decoded_events[0].require(Event)?
print $decoded.name $decoded.root $decoded.digest $decoded.ok $decoded.error
print $first.service decoded_events.len()
```

The script builds a metadata record, writes it to a temporary file, reads it
back, checks it against `Metadata`, writes event records as JSON lines, and
checks the first event against `Event`.

Why XSH shines here: JSON is a boundary format, not the internal language of
the script. The script keeps typed values until `json.write`, validates data
from `json.read` with `.require(Type)?`, and uses JSON-lines helpers only where
line-oriented data is actually useful.

Compared with bash and CLI tools: `jq` is excellent for exploring JSON at a
terminal. XSH is better when parsed JSON feeds filesystem work, process
decisions, typed records, retries, or tests in the same script.

Common trap: do not trust JSON fields just because the file was valid JSON.
Valid JSON means the syntax parsed. `.require(Type)?` is the point where the
script says which shape it needs.

Do not add a schema type for every throwaway JSON value. Add one where the data
crosses a trust boundary or where later code depends on fields having a stable
shape.

For tools that intentionally operate on unknown JSON shapes, use type patterns
inside `match`:

```xsh
match value {
  i is Int => print ${i.float()}
  f is Float => print ${f}
  s is Str => print ${s}
  _ is Null => print "null"
  _ => print "container"
}
```

That is the dynamic-data escape hatch. It tests and narrows a value of type
`Any`. For ordinary application data, prefer `.require(Type)?`.

## Treat Network Failures As Data

Network checks should keep lookup data, pool configuration, request records,
and errors separate. This example resolves `localhost`, creates a named HTTP
pool, and handles an intentionally unsupported URL scheme as data.

```xsh
let hosts = dns.resolve_host("localhost")?
let pool = net.pool("docs", 2, 5s)?
let refused = net.request({method: "GET", url: "ftp://example.invalid/file", pool: "docs"})

match refused {
  Err(error) => print (hosts.len() > 0) $pool.name $pool.max_idle_per_host $error.message
}

let _closed = net.close_pool("docs")?
```

Why XSH shines here: DNS answers and network responses are records, and
expected failures such as `net-scheme` remain inspectable `Err` values.

Do not hide network errors behind a boolean unless the caller truly only needs
yes or no. Keep the error value when the next step should distinguish DNS,
scheme, timeout, or response failures.

## What You Know Now

At a data boundary, make the conversion visible. Use JSON for persistence or
interchange, `.require(Type)?` before trusting dynamic data, and `match` when an
expected network failure should guide the next step instead of aborting the
script.
