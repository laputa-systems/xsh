#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type FdOptions = {
  hidden: Bool,
  ignore: Bool,
  glob: Bool,
  ignore_case: Bool,
  absolute: Bool,
  print0: Bool,
  max_depth: Int,
  kind: Str,
  ext: Str,
  excludes: List[Str],
  operands: List[Str],
}

pure glob_match(pattern: Str, text: Str) -> Bool {
  if pattern == "*" {
    return true
  }

  let parts = pattern.split("*")

  if parts.len() == 1 {
    return text == pattern
  }

  if pattern.starts_with("*") and pattern.ends_with("*") {
    return parts[1] in text
  }

  if pattern.starts_with("*") {
    return text.ends_with(parts[1])
  }

  if pattern.ends_with("*") {
    return text.starts_with(parts[0])
  }

  return text.starts_with(parts[0]) and text.ends_with(parts[1])
}

pure hidden_path(path_text: Str) -> Bool {
  return path_text.starts_with(".") or "/." in path_text
}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: FdOptions = cli.applet(
    argv,
    {
      hidden: {
        form: "-H --hidden",
        default: false,
      },
      ignore: {
        form: "-I --no-ignore",
        default: true,
      },
      glob: {
        form: "-g --glob",
        default: false,
      },
      ignore_case: {
        form: "-i --ignore-case",
        default: false,
      },
      absolute: {
        form: "-a --absolute-path",
        default: false,
      },
      print0: {
        form: "-0 --print0",
        default: false,
      },
      max_depth: {
        form: "-d --max-depth N",
        kind: "Int",
        default: 0,
      },
      kind: {
        form: "-t --type TYPE",
        default: "",
      },
      ext: {
        form: "-e --extension EXT",
        default: "",
      },
      excludes: {
        form: "-E --exclude PATTERN",
        repeated: true,
      },
      color: {
        form: "--color WHEN",
        default: "",
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let operands = opts.operands
  let pattern = operands.get(0, "")
  let kind = opts.kind
  let ext = opts.ext
  let excludes = opts.excludes
  var roots: List[Path] = []

  if operands.len() > 1 {
    for operand in operands |> drop(1) {
      roots = roots.push(fp"${operand}")
    }
  }

  if roots.len() == 0 {
    roots = [p"."]
  }

  let match_pattern = if opts.ignore_case { pattern.lower() } else { pattern }
  let re = if opts.glob or match_pattern == "" { regex.compile(".*")? } else { regex.compile(match_pattern)? }

  for root in roots {
    for entry in fs.walk(root, gitignore: opts.ignore, hidden: opts.hidden)? |> sort-by .path {
      let name = entry.path.name()
      let comparable = if opts.ignore_case { name.lower() } else { name }
      let rel = entry.path.relative_to(root)
      let rel_text = rel.display()
      let depth = rel_text.split("/").len()
      continue when rel_text == "."
      continue when ! opts.hidden and hidden_path(rel_text)
      continue when opts.max_depth > 0 and depth > opts.max_depth
      continue when (kind == "f" or kind == "file") and entry.kind != "file"
      continue when (kind == "d" or kind == "dir" or kind == "directory") and entry.kind != "dir"
      continue when (kind == "l" or kind == "symlink") and entry.kind != "symlink"
      continue when (kind == "x" or kind == "executable") and ! entry.path.executable()?
      continue when ext != "" and entry.ext != ext
      continue when excludes |> any glob_match(., name)
      continue when opts.glob and ! glob_match(match_pattern, name)
      continue when ! opts.glob and ! re.matches(comparable)
      let shown = if opts.absolute { entry.path.resolve()?.display() } else { rel_text }

      if opts.print0 {
        io.write_stdout(f"${shown}\0")?
      } else {
        print $shown
      }
    }
  }
}
