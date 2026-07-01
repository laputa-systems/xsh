# Implements fd -tf | awk -F. 'NF > 1 {print tolower($NF)}' | sort | uniq -c | sort -n with zero subprocesses.
pure count_prefix(count: Int) -> Str {
  if count < 10 {
    return f"   ${count}"
  }

  if count < 100 {
    return f"  ${count}"
  }

  if count < 1000 {
    return f" ${count}"
  }

  f"${count}"
}

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
  print f"${count_prefix(row.count)} ${row.ext}"
}
