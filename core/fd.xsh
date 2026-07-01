#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure glob_match(pattern: Str, text: Str) -> Bool {
  if pattern == "*" {
    return true
  }

  let parts = pattern.split("*")

  if parts.len() == 1 {
    return text == pattern
  }

  if pattern.starts_with("*") and pattern.ends_with("*") {
    return text.contains(parts[1])
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
  return path_text.starts_with(".") or path_text.contains("/.")
}

proc main(...argv: List[Str]) [fs, error, io] {
  var pattern = ""
  var roots: List[Path] = []
  var kind = ""
  var ext = ""
  var hidden = false
  var ignore = true
  var glob = false
  var ignore_case = false
  var absolute = false
  var print0 = false
  var max_depth = 0
  var excludes: List[Str] = []
  var operands: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    match arg {
      "-H" | "--hidden" => hidden = true
      "-I" | "--no-ignore" => ignore = false
      "-g" | "--glob" => glob = true
      "-i" | "--ignore-case" => ignore_case = true
      "-a" | "--absolute-path" => absolute = true
      "-0" | "--print0" => print0 = true
      "-d" | "--max-depth" => {
        index += 1
        max_depth = argv[index].parse_int()?
      }
      "-t" | "--type" => {
        index += 1
        kind = argv[index]
      }
      "-e" | "--extension" => {
        index += 1
        ext = argv[index]
      }
      "-E" | "--exclude" => {
        index += 1
        excludes = excludes.push(argv[index])
      }
      _ => {
        if arg.starts_with("--color=") {
          let _ = arg
        } else if arg.starts_with("-d") and arg.count_chars() > 2 {
          max_depth = arg.replace("-d", "").parse_int()?
        } else if arg.starts_with("-t") and arg.count_chars() > 2 {
          kind = arg.replace("-t", "")
        } else if arg.starts_with("-e") and arg.count_chars() > 2 {
          ext = arg.replace("-e", "")
        } else if arg.starts_with("-E") and arg.count_chars() > 2 {
          excludes = excludes.push(arg.replace("-E", ""))
        } else if arg.starts_with("-") {
          return Err(AppletError.Usage("fd: unsupported option"))
        } else {
          operands = operands.push(arg)
        }
      }
    }

    index += 1
  }

  if operands.len() > 0 {
    pattern = operands[0]
  }

  if operands.len() > 1 {
    for operand in operands |> drop(1) {
      roots = roots.push(fp"${operand}")
    }
  }

  if roots.len() == 0 {
    roots = [p"."]
  }

  let match_pattern = if ignore_case { pattern.lower() } else { pattern }
  let re = if glob or match_pattern == "" { regex.compile(".*")? } else { regex.compile(match_pattern)? }

  for root in roots {
    for entry in fs.walk(root, gitignore: ignore, hidden: hidden)? |> sort-by .path {
      let name = entry.path.name()
      let comparable = if ignore_case { name.lower() } else { name }
      let rel = entry.path.relative_to(root)
      let rel_text = rel.display()
      let depth = rel_text.split("/").len()
      continue when rel_text == "."
      continue when ! hidden and hidden_path(rel_text)
      continue when max_depth > 0 and depth > max_depth
      continue when (kind == "f" or kind == "file") and entry.kind != "file"
      continue when (kind == "d" or kind == "dir" or kind == "directory") and entry.kind != "dir"
      continue when (kind == "l" or kind == "symlink") and entry.kind != "symlink"
      continue when (kind == "x" or kind == "executable") and ! entry.path.executable()?
      continue when ext != "" and entry.ext != ext
      continue when excludes |> any glob_match(., name)
      continue when glob and ! glob_match(match_pattern, name)
      continue when ! glob and ! re.matches(comparable)
      let shown = if absolute { entry.path.resolve()?.display() } else { rel_text }

      if print0 {
        io.write_stdout(f"${shown}\0")?
      } else {
        print $shown
      }
    }
  }
}
