#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

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
  var paths: List[Str] = []
  var parsing_flags = true
  var all = false
  var dirs_only = false
  var max_depth = 0
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if parsing_flags and arg == "--" {
      parsing_flags = false
    } else if parsing_flags and (arg == "-a" or arg == "--all") {
      all = true
    } else if parsing_flags and (arg == "-d" or arg == "--dirs-only") {
      dirs_only = true
    } else if parsing_flags and (arg == "-I" or arg == "--no-ignore") {
      let _ = arg
    } else if parsing_flags and (arg == "-L" or arg == "--level") {
      index += 1
      max_depth = argv[index].parse_int()?
    } else if parsing_flags and arg.starts_with("-L") and arg.count_chars() > 2 {
      max_depth = arg.replace("-L", "").parse_int()?
    } else if parsing_flags and arg == "--color" {
      index += 1
      let _ = argv[index]
    } else if parsing_flags and arg.starts_with("--color=") {
      let _ = arg
    } else if parsing_flags and arg.starts_with("-") and arg.count_chars() > 1 {
      return Err(reject_unsupported("tree", arg))
    } else {
      paths = paths.push(arg)
    }

    index += 1
  }

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
