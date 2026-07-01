type Row = {service: Str, level: Str, duration_ms: Int}

let rows: List[Row] = [
  {service: "api", level: "info", duration_ms: 12},
  {service: "api", level: "warn", duration_ms: 31},
  {service: "worker", level: "error", duration_ms: 44},
  {service: "worker", level: "info", duration_ms: 18},
  {service: "scheduler", level: "warn", duration_ms: 27},
]

let summary = rows
  |> where .level != "info"
  |> group-by .service
  |> sort-by .key
  |> map { |bucket|
    let total = bucket.items |> map .duration_ms |> sum()
    {service: bucket.key, weighted: total * 2}
  }

for item in summary {
  print f"${item.service}:${item.weighted}"
}
