#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type RgOptions = {
  ignore_case: Bool,
  fixed: Bool,
  word: Bool,
  line_match: Bool,
  invert: Bool,
  line_numbers: Bool,
  with_filename: Bool,
  no_filename: Bool,
  list_files: Bool,
  count: Bool,
  quiet: Bool,
  hidden: Bool,
  ignore: Bool,
  globs: List[Str],
  color: Str,
  pattern_option: Str,
  operands: List[Str],
}

pure glob_match(pattern: Str, text: Str) -> Bool {
  return true when pattern == "*"

  let parts = pattern.split("*")

  return text == pattern when parts.len() == 1

  return parts[1] in text when pattern.starts_with("*") and pattern.ends_with("*")

  return text.ends_with(parts[1]) when pattern.starts_with("*")

  return text.starts_with(parts[0]) when pattern.ends_with("*")

  text.starts_with(parts[0]) and text.ends_with(parts[1])
}

pure selected_by_glob_at(globs: List[Str], text: Str, index: Int, selected: Bool) -> Bool {
  return selected when index >= globs.len()

  let glob = globs[index]

  return false when glob.starts_with("!") and glob_match(glob.replace("!", ""), text)

  let next_selected = selected or (! glob.starts_with("!") and glob_match(glob, text))
  selected_by_glob_at(globs, text, index + 1, next_selected)
}

pure selected_by_glob(globs: List[Str], file_path: Path) -> Bool {
  return true when globs.len() == 0

  selected_by_glob_at(globs, file_path.display(), 0, false)
}

pure regex_pattern(pattern: Str, ignore_case: Bool, word: Bool, line: Bool) -> Str {
  let word_pattern = if word { f"\\b(?:${pattern})\\b" } else { pattern }
  let line_pattern = if line { f"^(?:${word_pattern})$" } else { word_pattern }
  if ignore_case {
    f"(?i:${line_pattern})"
  } else {
    line_pattern
  }
}

proc search_file(
  file_path: Path,
  pattern: Str,
  show_file: Bool,
  line_numbers: Bool,
  count: Bool,
  list_files: Bool,
  quiet: Bool,
  ignore_case: Bool,
  fixed: Bool,
  word: Bool,
  line_match: Bool,
  invert: Bool,
  color: Bool,
) [fs, error, io] -> Result[Bool] {
  let text = file_path.read_text()?
  let needle = if ignore_case { pattern.lower() } else { pattern }

  let re = if fixed {
    rx".*"
  } else {
    regex.compile(regex_pattern(pattern, ignore_case, word, line_match))?
  }

  let fixed_word = if fixed and word {
    regex.compile(regex_pattern(needle, false, true, false))?
  } else {
    rx".*"
  }

  var matches = 0

  for item in text.lines() |> enumerate() {
    let line = item.value
    let comparable = if fixed and ignore_case { line.lower() } else { line }

    let hit = if fixed and line_match {
      comparable == needle
    } else if fixed and word {
      fixed_word.matches(comparable)
    } else if fixed {
      needle in comparable
    } else {
      re.matches(comparable)
    }

    let selected = if invert { ! hit } else { hit }
    continue when ! selected
    matches += 1

    return true when quiet

    if list_files {
      print $file_path
      return true
    }

    if ! count {
      var out = line

      if color {
        out = line.replace(pattern, f"[1;31m${pattern}[0m")
      }

      if show_file and line_numbers {
        print f"${file_path.display()}:${item.index + 1}:${out}"
      } else if show_file {
        print f"${file_path.display()}:${out}"
      } else if line_numbers {
        print f"${item.index + 1}:${out}"
      } else {
        print $out
      }
    }
  }

  if count {
    if matches > 0 or show_file {
      if show_file {
        print f"${file_path.display()}:${matches}"
      } else {
        print $matches
      }
    }
  }

  matches > 0
}

proc main(...argv: List[Str]) [fs, error, io] {
  let opts: RgOptions = cli.applet(
    argv,
    {
      ignore_case: {
        form: "-i",
        default: false,
      },
      fixed: {
        form: "-F",
        default: false,
      },
      word: {
        form: "-w",
        default: false,
      },
      line_match: {
        form: "-x",
        default: false,
      },
      invert: {
        form: "-v",
        default: false,
      },
      line_numbers: {
        form: "-n",
        default: false,
      },
      with_filename: {
        form: "-H",
        default: false,
      },
      no_filename: {
        form: "-h",
        default: false,
      },
      list_files: {
        form: "-l",
        default: false,
      },
      count: {
        form: "-c",
        default: false,
      },
      quiet: {
        form: "-q",
        default: false,
      },
      hidden: {
        form: "--hidden",
        default: false,
      },
      ignore: {
        form: "-I --no-ignore",
        default: true,
      },
      globs: {
        form: "-g --glob GLOB",
        repeated: true,
      },
      color: {
        form: "--color WHEN",
        default: "auto",
      },
      pattern_option: {
        form: "-e PATTERN",
        default: "",
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let pattern = if opts.pattern_option != "" {
    opts.pattern_option
  } else {
    opts.operands.get(0) ?? ""
  }
  let path_args = if opts.pattern_option != "" { opts.operands } else { opts.operands |> drop(1) }
  var paths: List[Path] = [fp"${arg}" for arg in path_args]
  let {ignore_case, fixed, word, line_match, invert, line_numbers, with_filename, no_filename, list_files, count, quiet, hidden, ignore, globs, ..} = opts
  let color = opts.color == "always"

  return Err(AppletError.Usage("rg: missing pattern")) when pattern == ""

  if paths.len() == 0 {
    paths = [p"."]
  }

  var files = []

  for target in paths {
    let meta = target.metadata()?

    if meta.kind == "dir" {
      for entry in fs.walk(target, gitignore: ignore)? |> sort-by .path {
        continue when entry.kind != "file"
        continue when ! hidden and entry.path.name().starts_with(".")
        continue when ! selected_by_glob(globs, entry.path)
        files = files.push(entry.path)
      }
    } else if meta.kind == "file" {
      files += [target]
    }
  }

  let show_file = if no_filename { false } else if with_filename { true } else { files.len() > 1 }
  var any_match = false

  for file in files {
    if search_file(
      file,
      pattern,
      show_file,
      line_numbers,
      count,
      list_files,
      quiet,
      ignore_case,
      fixed,
      word,
      line_match,
      invert,
      color,
    )? {
      any_match = true

      return when quiet
    }
  }

  if ! any_match {
    abort(1)
  }
}
