let root = fp"${args[0]}"
let log_texts = fs.walk(root, gitignore: false)
  |> where .kind == "file" and .ext == "jsonl"
  |> sort-by .path
  |> map { |entry|
    entry.path.read_text()?
  }

let rows = log_texts.join()
  |> json.lines
  |> where .level != "debug"
  |> group-by .service
  |> map { |bucket|
    {
      service: bucket.key,
      count: bucket.items |> count(),
      total: bucket.items
        |> map .duration_ms
        |> sum,
    }
  }
  |> sort-by .service

for row in rows {
  print f"${row.service} ${row.count} ${row.total}"
}
