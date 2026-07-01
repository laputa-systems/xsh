let root = fp"${args[0]}"
let logs = fp"${root}/logs"

let log_texts = fs.walk(logs, gitignore: false)
  |> where .kind == "file" and .ext == "jsonl"
  |> sort-by .path
  |> map { |entry|
    entry.path.read_text()?
  }

let rows = log_texts.join()
  |> json.lines
  |> where .level != "debug"
  |> group-by f"${.service}:${.level}"
  |> map { |bucket|
    {
      key: bucket.key,
      count: bucket.items |> count(),
      duration_ms: bucket.items
        |> map .duration_ms
        |> sum,
    }
  }
  |> sort-by .key

for row in rows {
  print f"${row.key} ${row.count} ${row.duration_ms}"
}
