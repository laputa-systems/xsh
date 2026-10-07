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

pure is_assignment(arg: Str) -> Bool {
  let parts = arg.split("=", maxsplit: 1)
  parts.len() > 1 and parts[0] != ""
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

proc run_command(command_argv: List[Str], cwd: Path?, path_update: Str?, module_update: Str?) [process, error] -> Status {
  if let directory = cwd {
    if let path_value = path_update {
      if let module_path = module_update {
        return process.run(process.command_argv(command_argv[0], command_argv, cwd: directory, env: {PATH: path_value, XSH_MODULE_PATH: module_path}))?
      }
      return process.run(process.command_argv(command_argv[0], command_argv, cwd: directory, env: {PATH: path_value}))?
    }
    if let module_path = module_update {
      return process.run(process.command_argv(command_argv[0], command_argv, cwd: directory, env: {XSH_MODULE_PATH: module_path}))?
    }
    return process.run(process.command_argv(command_argv[0], command_argv, cwd: directory))?
  }
  if let path_value = path_update {
    if let module_path = module_update {
      return process.run(process.command_argv(command_argv[0], command_argv, env: {PATH: path_value, XSH_MODULE_PATH: module_path}))?
    }
    return process.run(process.command_argv(command_argv[0], command_argv, env: {PATH: path_value}))?
  }
  if let module_path = module_update {
    return process.run(process.command_argv(command_argv[0], command_argv, env: {XSH_MODULE_PATH: module_path}))?
  }
  process.run(process.command_argv(command_argv[0], command_argv))?
}

proc split_argv(raw: List[Str]) [process, env, error] -> List[Str] {
  var argv = raw
  if argv.len() >= 1 and argv[0].starts_with("-S ") {
    argv = split_string(argv[0].byte_slice(3)).extend(raw |> drop(1))
  } else if argv.len() >= 2 and argv[0] == "-S" {
    argv = split_string(argv[1]).extend(raw |> drop(2))
  } else if argv.len() >= 1 and argv[0].starts_with("-S") and argv[0] != "-S" {
    argv = split_string(argv[0].byte_slice(2)).extend(raw |> drop(1))
  } else if argv.len() >= 1 and argv[0].starts_with("--split-string=") {
    argv = split_string(argv[0].byte_slice(15)).extend(raw |> drop(1))
  } else if argv.len() >= 2 and argv[0] == "--split-string" {
    argv = split_string(argv[1]).extend(raw |> drop(2))
  }
  argv
}

proc main(...raw: List[Str]) [process, env, error, io] {
  var argv = split_argv(raw)
  var cwd: Path? = null
  var null_delimited = false
  var option_index = 0

  while option_index < argv.len() {
    let word = argv[option_index]
    if word == "--" { option_index += 1; break }
    if word == "--help" {
      gnu.help("Usage: env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]\n")
      return
    }
    if word == "--version" { gnu.version("env"); return }
    if word in ["--null", "-0"] { null_delimited = true; option_index += 1; continue }
    if word in ["-i", "--ignore-environment", "-"] {
      gnu.error("ignoring the environment is unsupported by the process command API")
      exit 125
    }
    if word in ["-u", "--unset"] or word.starts_with("--unset=") {
      gnu.error("unsetting environment variables is unsupported by the process command API")
      exit 125
    }
    if word in ["--ignore-signal", "--default-signal", "--block-signal", "--list-signal-handling"] or word.starts_with("--ignore-signal=") or word.starts_with("--default-signal=") or word.starts_with("--block-signal=") {
      gnu.error(f"{word}: signal options are unsupported by the process command API")
      exit 125
    }
    if word == "-C" or word == "--chdir" {
      if option_index + 1 >= argv.len() { gnu.error(f"{word}: missing operand"); exit 125 }
      cwd = argv[option_index + 1] as Path
      option_index += 2
      continue
    }
    if word.starts_with("--chdir=") {
      cwd = word.byte_slice(8) as Path
      option_index += 1
      continue
    }
    if word.starts_with("-C") and word.byte_len() > 2 {
      cwd = word.byte_slice(2) as Path
      option_index += 1
      continue
    }
    if word.starts_with("-") and word != "-" and word != "--" {
      gnu.error(f"invalid option -- '{word.byte_slice(1)}'")
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

  while index < argv.len() and is_assignment(argv[index]) {
    let parts = argv[index].split("=", maxsplit: 1)
    let value = parts.get(1) ?? ""

    match parts[0] {
      "PATH" => path_update = value
      "XSH_MODULE_PATH" => xsh_module_path_update = value
      else => {
        gnu.error(f"unsupported assignment {parts[0]}")
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
