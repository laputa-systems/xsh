let root = fp"${args[0]}"
let stats = fs.files(root, gitignore: false, stat: false)
  |> where .ext != ""
  |> count { |entry|
    entry.ext.lower()
  }

let counts = stats.keys()
  |> map { |ext|
    {count: stats.get(ext, 0), ext: ext}
  }
  |> sort-by .count

for row in counts {
  print f"${row.count} ${row.ext}"
}
