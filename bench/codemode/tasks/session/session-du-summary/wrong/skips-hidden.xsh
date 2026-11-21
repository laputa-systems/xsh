type Usage = {path: Str, total: Int}

## Summarize disk usage by directory down to --depth.
cli main(dir: Path, depth: Int = 1, min_bytes: Int = 0) {
  if depth < 0 or min_bytes < 0 {
    eprint "usage: du-summary [--depth N] [--min-bytes N] DIR (values at least 0)"
    exit 2
  }

  if ! (dir.is_dir() ?? false) {
    eprint f"{dir} is not a directory"
    exit 2
  }

  let root = dir.resolve()?
  var totals: Map[Str, Int] = {".": 0}

  for entry in fs.walk(root)? {
    if entry.path == root {
      continue
    }

    let parts = entry.path.relative_to(root).display().split("/")
    if entry.kind == "dir" and parts.len() <= depth {
      let key = parts.join("/")
      totals[key] = totals.get(key) ?? 0
    } else if entry.kind == "file" {
      totals["."] = (totals.get(".") ?? 0) + entry.size
      for i in range(parts.len() - 1) {
        if i + 1 <= depth {
          let key = parts[..i + 1].join("/")
          totals[key] = (totals.get(key) ?? 0) + entry.size
        }
      }
    }
  }

  let rows = [Usage(path: key, total: value) for {key, value} in totals if value >= min_bytes]
  for row in rows |> sort-by { |r| {first: 0 - r.total, second: r.path} } {
    print f"{row.total}\t{row.path}"
  }
}
