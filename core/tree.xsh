#!/bin/xsh
type Counts = {dirs: Int, files: Int}

pure empty_counts() -> Counts {
  return {dirs: 0, files: 0}
}

pure add_counts(left: Counts, right: Counts) -> Counts {
  return {dirs: left.dirs + right.dirs, files: left.files + right.files}
}

pure plural(count: Int, singular: Str, multiple: Str) -> Str {
  return if count == 1 { singular } else { multiple }
}

proc print_entry(target: Path, name: Str, prefix: Str, is_last: Bool, kind: Str) [fs, error] {
  let connector = if is_last { "`-- " } else { "|-- " }
  var suffix = ""

  if kind == "symlink" {
    suffix = f" -> ${target.readlink()?.display()}"
  }

  print f"${prefix}${connector}${name}${suffix}"
}

proc print_children(
  target: Path,
  prefix: Str,
  all: Bool,
  dirs_only: Bool,
  max_depth: Int,
  depth: Int,
) [fs, error] -> Result[Counts] {
  if max_depth > 0 and depth > max_depth {
    return empty_counts()
  }

  let entries = fs.ls(target)?
    |> where all or ! .name.starts_with(".")
    |> where ! dirs_only or .kind == "dir"
    |> sort-by .name

  let count = entries.len()
  var totals = empty_counts()

  for item in entries |> enumerate() {
    let entry = item.value
    let is_last = item.index + 1 == count
    print_entry(entry.path, entry.name, prefix, is_last, entry.kind)?

    if entry.kind == "dir" {
      totals = add_counts(totals, {dirs: 1, files: 0})
      let child_prefix = if is_last { f"${prefix}    " } else { f"${prefix}|   " }
      totals = add_counts(totals, print_children(entry.path, child_prefix, all, dirs_only, max_depth, depth + 1)?)
    } else if entry.kind == "file" or entry.kind == "symlink" {
      totals = add_counts(totals, {dirs: 0, files: 1})
    }
  }

  return totals
}

proc print_target(raw: Str, all: Bool, dirs_only: Bool, max_depth: Int) [fs, error] -> Result[Counts] {
  let target = fp"${raw}"
  let meta = target.metadata()?
  print $raw

  if meta.kind == "dir" {
    return print_children(target, "", all, dirs_only, max_depth, 1)?
  } else if meta.kind == "symlink" {
    print f" -> ${target.readlink()?.display()}"
  }

  if meta.kind == "file" or meta.kind == "symlink" {
    return {dirs: 0, files: 1}
  }

  return empty_counts()
}

proc main(...argv: List[Str]) [fs, error] {
  let options = cli.applet(
    argv,
    {
      all: {
        form: "-a --all",
        default: false,
      },
      dirs_only: {
        form: "-d --dirs-only",
        default: false,
      },
      no_ignore: {
        form: "-I --no-ignore",
        default: false,
      },
      max_depth: {
        form: "-L --level N",
        kind: "Int",
        default: 0,
      },
      color: {
        form: "--color[=WHEN]",
        default: "",
      },
      paths: {
        form: "...PATH",
      },
    },
  )?
  let all = options.all
  let dirs_only = options.dirs_only
  let max_depth = options.max_depth
  var paths = [target for target in options.paths]

  if paths.len() == 0 {
    paths = paths.push(".")
  }

  var totals = empty_counts()

  for item in paths |> enumerate() {
    if item.index > 0 {
      print ""
    }

    totals = add_counts(totals, print_target(item.value, all, dirs_only, max_depth)?)
  }

  print ""

  print f"${totals.dirs} ${plural(totals.dirs, "directory", "directories")}, ${totals.files} ${plural(
    totals.files,
    "file",
    "files",
  )}"
}
