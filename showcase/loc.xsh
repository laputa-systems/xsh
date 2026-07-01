#!/usr/bin/env -S xsh --
# Lines Of Code
# Count files and lines by extension using a streaming, per-extension accumulator.
# Usage: xsh showcase/loc.xsh -- [ROOT] [EXT...]
# Example: xsh showcase/loc.xsh -- src rs xsh
proc main(root: Path = p".", ...exts: List[Str]) [fs, error] {
  let ext_set = set.from(exts)

  # Stream into a per-extension {files, lines} accumulator instead of buffering
  # every file with `group-by`: O(distinct extensions) live, and the per-file
  # read+count fans out across cores (associative fold, parallel by default).
  let totals = fs.files(root)
    |> where { |entry|
      exts.len() == 0 or set.has(ext_set, entry.path.ext())
    }
    |> reduce-by --sum { |entry|
      {key: entry.path.ext(), value: {files: 1, lines: entry.path.read_text()?.count_lines()}}
    }

  let counts = totals.keys()
    |> map { |ext|
      let row = totals.get(ext, {files: 0, lines: 0})
      {ext: ext, files: row.files, lines: row.lines}
    }
    |> sort-by --desc .lines

  counts |> table.print(columns: ["ext", "files", "lines"])

  let total_files = counts
    |> map .files
    |> sum

  let total_lines = counts
    |> map .lines
    |> sum

  print f"${total_files} files  ${total_lines} lines"
}
