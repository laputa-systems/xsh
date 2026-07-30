#!/usr/bin/env -S xsh --
# CSV Query
# Query simple CSV files with filtering, sorting, grouping, limiting, and counts.
# Usage: xsh showcase/csv-query.xsh -- FILE [--filter COL=VAL] [--sort COL] [--group COL]
# Example: xsh showcase/csv-query.xsh -- data.csv --filter team=core --count
# Basic CSV querying: filter, sort, group, limit.
# Assumes simple CSV with no embedded commas in quoted fields.
type Opts = {file: Path, filter: Str, sort: Str, group: Str, limit: Int, count: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      file: {
        form: "FILE",
        kind: "Path",
        file: true,
      },
      filter: {
        form: "--filter COL=VAL",
        default: "",
      },
      sort: {
        form: "--sort COL",
        default: "",
      },
      group: {
        form: "--group COL",
        default: "",
      },
      limit: {
        form: "--limit N",
        kind: "UInt",
        default: 0,
      },
      count: {
        form: "--count",
        default: false,
      },
    },
  )?

  let content = opts.file.read_text()?
  let lines = content.lines() |> where . != ""

  if lines.len() == 0 {
    print "empty file"
    return
  }

  let header = lines[0].split(",") |> map .trim()
  let col_count = header.len()
  print f"columns (${col_count}): ${header.join(", ")}"
  var rows: List[Map[Str]] = []

  for item in lines |> enumerate() {
    continue when item.index == 0
    let fields = item.value.split(",") |> map .trim()
    var row: Map[Str] = {}

    for col in header |> enumerate() {
      let val = if col.index < fields.len() { fields[col.index] } else { "" }
      row[col.value] = val
    }

    rows = rows.push(row)
  }

  print f"${rows.len()} row(s) loaded"

  if opts.filter != "" {
    let parts = opts.filter.split("=", maxsplit: 1)

    if parts.len() < 2 {
      print "error: --filter must be COL=VAL"
      return
    }

    let filter_col = parts[0].trim()
    let filter_val = parts[1].trim()
    rows = rows |> where .get(filter_col, "") == filter_val
    print f"  ${rows.len()} row(s) match ${filter_col}=${filter_val}"
  }

  if opts.sort != "" {
    let sort_col = opts.sort
    rows = rows |> sort-by .get(sort_col, "")
    print f"  sorted by ${sort_col}"
  }

  if opts.group != "" {
    let group_col = opts.group

    let groups = rows
      |> group-by .get(group_col, "")
      |> sort-by --desc .items.len()

    print f"  ${groups.len()} group(s) by ${group_col}"

    for grp in groups {
      print f"    ${grp.key}  ${grp.items.len()}"
    }

    return
  }

  if opts.limit > 0 {
    rows = rows |> take(opts.limit)
    print f"  limited to ${rows.len()} row(s)"
  }

  if opts.count {
    print f"total: ${rows.len()} row(s)"
    return
  }

  print header.join("\t")

  for row in rows {
    let values = header |> map row.get(., "")
    print values.join("\t")
  }
}
