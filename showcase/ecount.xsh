#!/usr/bin/env -S xsh --
# Extension Count
# Count files by extension, optionally summing byte sizes, using a streaming accumulator.
# Unlike fd | awk extension counters, extensionless files are counted as (none).
# Usage: xsh showcase/ecount.xsh -- [--size] [ROOT]
# Example: xsh showcase/ecount.xsh -- --size src
let argv = args
var show_size = false
var path_arg = ""

for arg in argv {
  match arg {
    "-s" | "--size" => show_size = true
    _ => path_arg = arg
  }
}

let root = if path_arg != "" { fp"${path_arg}" } else { fs.cwd()? }

# The cheap path uses keyed count so it avoids stat and per-file accumulator
# records; the size path keeps one {count, size} record per extension.
if show_size {
  let stats = fs.files(root, stat: true)
    |> reduce-by --sum { |entry|
      {key: entry.ext.lower(), value: {count: 1, size: entry.size}}
    }

  let rows = stats.keys()
    |> map { |ext|
      let totals = stats.get(ext, {count: 0, size: 0})
      let label = if ext == "" { "(none)" } else { ext }
      {ext: label, count: totals.count, size: totals.size}
    }
    |> sort-by .size

  for row in rows {
    print f"${row.count:>4} ${row.size:>12} ${row.ext}"
  }
} else {
  let counts = fs.files(root, stat: false)
    |> count { |entry|
      entry.ext.lower()
    }

  let rows = counts.keys()
    |> map { |ext|
      let label = if ext == "" { "(none)" } else { ext }
      {ext: label, count: counts.get(ext, 0)}
    }
    |> sort-by .count

  for row in rows {
    print f"${row.count:>4} ${row.ext}"
  }
}
