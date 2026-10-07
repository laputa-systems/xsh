#!/bin/xsh
use lib.gnu
error EnvError = Usage : Usage | Failed

proc split_error(message: Str) [process, env, error] {
  gnu.error(message)
  exit 125
}

proc split_string(text: Str) [process, env, error] -> List[Str] {
  var words: List[Str] = []
  var word = ""
  var state = 0
  var started = false
  var at = 0
  let length = text.count_chars()

  while at < length {
    let ch = text[at..at + 1]
    if state == 0 and ch in [" ", "\t", "\r", "\n", "\u{b}", "\u{c}"] {
      at += 1
      continue
    }
    if state == 0 and ch == "#" and ! started {
      while at < length and text[at..at + 1] != "\n" { at += 1 }
      continue
    }
    if state == 1 and ch in [" ", "\t", "\r", "\n", "\u{b}", "\u{c}"] {
      words += [word]
      word = ""
      started = false
      state = 0
      at += 1
      continue
    }
    if state == 0 and ch == "'" { state = 2; started = true; at += 1; continue }
    if state == 0 and ch == "\"" { state = 3; started = true; at += 1; continue }
    if (state == 2 and ch == "'") or (state == 3 and ch == "\"") {
      state = 1
      at += 1
      continue
    }
    if ch == "$" and state != 2 {
      if at + 1 >= length or text[at + 1..at + 2] != "{" {
        split_error(f"invalid variable reference in -S at position {at + 1}")
      }
      var end = at + 2
      while end < length and text[end..end + 1] != "}" and text[end..end + 1] in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_" { end += 1 }
      if end >= length or text[end..end + 1] != "}" or end == at + 2 {
        split_error(f"invalid braced variable in -S at position {at + 1}")
      }
      let name = text[at + 2..end]
      if "0123456789".find(name[0..1]) != null { split_error(f"variable name starts with a digit in -S at position {at + 3}") }
      word += env.get_or(name, "") ?? ""
      started = true
      state = if state == 0 { 1 } else { state }
      at = end + 1
      continue
    }
    if ch == "\\" and state != 2 {
      let position = at + 2
      if at + 1 >= length { split_error(f"backslash at end of -S string at position {at + 1}") }
      let next = text[at + 1..at + 2]
      if state == 3 and next == "c" { split_error(f"'\\c' must not appear in double-quoted -S string at position {position}") }
      if next == "\n" { at += 2; continue }
      if state == 1 and next == "_" {
        words += [word]
        word = ""
        started = false
        state = 0
        at += 2
        continue
      }
      if state == 0 and next == "c" {
        if started { words += [word] }
        return words
      }
      if state == 0 and next in ["\\", "'", "\"", "$", "#"] {
        word += next
      } else if next in ["r", "n", "t", "f", "v", "_", "#", "$", "\""] {
        word += match next {
          "r" => "\r"
          "n" => "\n"
          "t" => "\t"
          "f" => "\u{c}"
          "v" => "\u{b}"
          "_" => " "
          else => next
        }
      } else {
        split_error(f"invalid sequence '\\{next}' in -S at position {position}")
      }
      started = true
      state = if state == 0 { 1 } else { state }
      at += 2
      continue
    }
    word += ch
    started = true
    if state == 0 { state = 1 }
    at += 1
  }

  if state == 2 { split_error("missing closing single quote in -S string") }
  if state == 3 { split_error("missing closing double quote in -S string") }
  if started { words += [word] }
  words
}

pure assignment_separator(arg: Bytes) -> Int? {
  for at in range(arg.len()) {
    if arg.byte_at(at) == 61 { return at }
  }
  null
}

pure text_if_utf8(arg: Bytes) -> Str {
  match arg.utf8() {
    Ok(text) => text
    Err(_) => ""
  }
}

pure byte_prefix(arg: Bytes, prefix: Bytes) -> Bool {
  arg.len() >= prefix.len() and arg[0..prefix.len()] == prefix
}

proc print_environment(null_delimited: Bool) [env, io, error] {
  for item in env.list() |> sort-by .name {
    io.write_stdout(f"{item.name}={item.value}{if null_delimited { "\0" } else { "\n" }}")
  }
}

proc handle_status(status: Status) [error] {
  return when status.ok

  if status.exited() {
    exit status.exit_code()?
  }

  return Err(EnvError.Failed("command was signaled"))
}

proc run_command(command_argv: List[Bytes], cwd: Path?, path_update: Str?, module_update: Str?) [process, error] -> Status {
  let words: List[Path] = collect {
    for argument in command_argv { yield Path.parse_bytes(argument)? }
  }
  let target = words[0]
  if let directory = cwd {
    if let path_value = path_update {
      if let module_path = module_update {
        return process.run(process.command_argv(target, words, cwd: directory, env: {PATH: path_value, XSH_MODULE_PATH: module_path}))?
      }
      return process.run(process.command_argv(target, words, cwd: directory, env: {PATH: path_value}))?
    }
    if let module_path = module_update {
      return process.run(process.command_argv(target, words, cwd: directory, env: {XSH_MODULE_PATH: module_path}))?
    }
    return process.run(process.command_argv(target, words, cwd: directory))?
  }
  if let path_value = path_update {
    if let module_path = module_update {
      return process.run(process.command_argv(target, words, env: {PATH: path_value, XSH_MODULE_PATH: module_path}))?
    }
    return process.run(process.command_argv(target, words, env: {PATH: path_value}))?
  }
  if let module_path = module_update {
    return process.run(process.command_argv(target, words, env: {XSH_MODULE_PATH: module_path}))?
  }
  process.run(process.command_argv(target, words))?
}

proc split_argv(raw: List[Bytes]) [process, env, error] -> List[Bytes] {
  var argv = raw
  let first = if argv.is_empty() { "" } else { text_if_utf8(argv[0]) }
  var split_words: List[Str] = []
  var remainder: List[Bytes] = []
  if first.starts_with("-S ") {
    split_words = split_string(first.byte_slice(3))
    remainder = raw[1..]
  } else if first == "-S" and argv.len() >= 2 {
    let source = match argv[1].utf8() { Ok(text) => text, Err(_) => { split_error("invalid split string"); "" } }
    split_words = split_string(source)
    remainder = raw[2..]
  } else if first.starts_with("-S") and first != "-S" {
    split_words = split_string(first.byte_slice(2))
    remainder = raw[1..]
  } else if first.starts_with("--split-string=") {
    split_words = split_string(first.byte_slice(15))
    remainder = raw[1..]
  } else if first == "--split-string" and argv.len() >= 2 {
    let source = match argv[1].utf8() { Ok(text) => text, Err(_) => { split_error("invalid split string"); "" } }
    split_words = split_string(source)
    remainder = raw[2..]
  } else {
    return argv
  }
  let expanded: List[Bytes] = collect {
    for word in split_words { yield bytes.from_text(word) }
  }
  expanded.extend(remainder)
}

proc main(...raw: List[Bytes]) [process, env, error, io] {
  var argv = split_argv(raw)
  var cwd: Path? = null
  var null_delimited = false
  var option_index = 0

  while option_index < argv.len() {
    let raw_word = argv[option_index]
    let word = text_if_utf8(raw_word)
    if raw_word == b"--" { option_index += 1; break }
    if raw_word == b"--help" {
      gnu.help("Usage: env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]\n")
      return
    }
    if raw_word == b"--version" { gnu.version("env"); return }
    if raw_word in [b"--null", b"-0"] { null_delimited = true; option_index += 1; continue }
    if raw_word in [b"-i", b"--ignore-environment", b"-"] {
      gnu.error("ignoring the environment is unsupported by the process command API")
      exit 125
    }
    if raw_word in [b"-u", b"--unset"] or byte_prefix(raw_word, b"--unset=") {
      gnu.error("unsetting environment variables is unsupported by the process command API")
      exit 125
    }
    if raw_word in [b"--ignore-signal", b"--default-signal", b"--block-signal", b"--list-signal-handling"] or byte_prefix(raw_word, b"--ignore-signal=") or byte_prefix(raw_word, b"--default-signal=") or byte_prefix(raw_word, b"--block-signal=") {
      gnu.error(f"{gnu.quote_bytes(raw_word, always: false)}: signal options are unsupported by the process command API")
      exit 125
    }
    if raw_word == b"-C" or raw_word == b"--chdir" {
      if option_index + 1 >= argv.len() { gnu.error(f"{word}: missing operand"); exit 125 }
      cwd = Path.parse_bytes(argv[option_index + 1])?
      option_index += 2
      continue
    }
    if byte_prefix(raw_word, b"--chdir=") {
      cwd = Path.parse_bytes(argv[option_index][8..])?
      option_index += 1
      continue
    }
    if byte_prefix(raw_word, b"-C") and raw_word.len() > 2 {
      cwd = Path.parse_bytes(argv[option_index][2..])?
      option_index += 1
      continue
    }
    if (raw_word.byte_at(0) ?? 0) == 45 and raw_word != b"-" and raw_word != b"--" {
      gnu.error(f"invalid option -- '{text_if_utf8(raw_word[1..])}'")
      exit 125
    }
    break
  }
  argv = argv[option_index..]

  if argv.is_empty() {
    print_environment(null_delimited)
    return
  }
  if null_delimited { gnu.error("cannot specify --null (-0) with command"); exit 125 }

  var path_update: Str? = null
  var xsh_module_path_update: Str? = null
  var index = 0

  while index < argv.len() and assignment_separator(argv[index]) != null {
    let separator = assignment_separator(argv[index]) ?? 0
    let name = argv[index][0..separator].utf8()?
    let value = argv[index][separator + 1..].utf8()?

    match name {
      "PATH" => path_update = value
      "XSH_MODULE_PATH" => xsh_module_path_update = value
      else => {
        gnu.error(f"unsupported assignment {name}")
        exit 125
      }
    }

    index += 1
  }

  if index >= argv.len() {
    print_environment(null_delimited)
    return
  }

  let command_argv = collect {
    while index < argv.len() {
      yield argv[index]
      index += 1
    }
  }

  handle_status(run_command(command_argv, cwd, path_update, xsh_module_path_update))
}
