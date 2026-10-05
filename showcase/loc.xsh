#!/usr/bin/env -S xsh --
# Lines Of Code
# Count files and lines by extension using a streaming, per-extension accumulator.
# Usage: xsh showcase/loc.xsh -- [ROOT] [EXT...]
# Example: xsh showcase/loc.xsh -- src rs xsh
proc main(root = p".", ...exts: List[Str]) [fs, error] {
  let ext_set: Set[Str] = set.from(exts)

  # Stream into a per-extension {files, lines} accumulator instead of buffering
  # every file with `group-by`: O(distinct extensions) live, and the per-file
  # read+count runs in source order; reduce-by folds one item at a time.
  let totals = fs.files(root)
    |> where { |entry|
      exts.is_empty() or entry.path.ext() in ext_set
    }
    |> reduce-by(sum: true) { |entry|
      {key: entry.path.ext(), value: {files: 1, lines: entry.path.read_text()?.count_lines()}}
    }

  let counts = totals.keys()
    |> map { |ext|
      let row = totals.get(ext) ?? {files: 0, lines: 0}
      {ext: ext, files: row.files, lines: row.lines}
    }
    |> sort-by(desc: true) .lines

  counts |> table.print(columns: ["ext", "files", "lines"])

  let total_files = counts
    |> map .files
    |> sum

  let total_lines = counts
    |> map .lines
    |> sum

  print f"{total_files} files  {total_lines} lines"
}
