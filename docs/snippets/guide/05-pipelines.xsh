type Job = {name: Str, kind: Str, seconds: Int}

let jobs = [
  Job(name: "lint", kind: "check", seconds: 4),
  Job(name: "unit", kind: "test", seconds: 30),
  Job(name: "e2e", kind: "test", seconds: 90),
  Job(name: "fmt", kind: "check", seconds: 2),
]

let slow = jobs |> where .seconds > 3 |> sort-by(desc: true) .seconds |> map .name
print f"slowest first: {slow.join(" ")}"

for bucket in jobs |> group-by .kind {
  let total = bucket.items |> map .seconds |> sum()
  print f"{bucket.key}: {bucket.items.len()} jobs, {total}s"
}

let doubled = jobs |> par-map(jobs: 2) { |job| job.seconds * 2 }
print f"doubled in input order: {json.encode(doubled)?}"
print f"any over a minute: {jobs |> any { .seconds > 60 }}"
