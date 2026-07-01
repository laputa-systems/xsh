# Implements fd -tf | awk -F. 'NF > 1 {print tolower($NF)}' | sort | uniq -c | sort -n with zero subprocesses.
let root = fs.cwd()?

let stats = fs.files(root, stat: false)
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
  print f"${row.count:>4} ${row.ext}"
}
