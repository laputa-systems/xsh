type Outcome = {input: Str, status: Str, error: Str?}

proc encode(name: Str) -> Result[Outcome] {
  let source = fp"/work/incoming/{name}"
  let target = fp"/work/out/{name.byte_slice(0, name.byte_len() - 4)}.flac"
  if target.exists()? {
    return Ok(Outcome(input: name, status: "skipped", error: null))
  }

  let result = run.capture --text /work/bin/encode $source $target
  if (result.status.exit_code() ?? 255) == 0 {
    return Ok(Outcome(input: name, status: "encoded", error: null))
  }

  target.remove()?
  let lines = result.stderr.lines()
  let first = if lines.is_empty() { "" } else { lines[0] }
  Outcome(input: name, status: "failed", error: first)
}

p"/work/out".mkdir(parents: true)?

let names = fs.children(p"/work/incoming")?
  |> where { |entry| entry.kind == "file" and entry.name.ends_with(".wav") }
  |> map { |entry| entry.name }
  |> sort

let pending = [n for n in names if ! fp"/work/out/{n.byte_slice(0, n.byte_len() - 4)}.flac".exists()?]
let encoded = pending |> par-map(jobs: 8) { |name| encode(name)? }

var failed = false
for name in names {
  let done = [o for o in encoded if o.input == name]
  let outcome = if done.is_empty() { Outcome(input: name, status: "skipped", error: null) } else { done[0] }
  if outcome.status == "failed" {
    failed = true
  }

  print (json.encode(outcome)?)
}

if failed {
  exit 4
}
