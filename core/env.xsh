#!/bin/xsh
use lib.gnu

const USAGE = """Usage: env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]
Set each NAME to VALUE in the environment and run COMMAND.

Options:
  -a, --argv0=ARG        pass ARG as the zeroth argument of COMMAND
  -i, --ignore-environment  start with an empty environment
  -0, --null             end each output line with NUL, not newline
  -u, --unset=NAME       remove variable from the environment
  -C, --chdir=DIR        change working directory to DIR
  -S, --split-string=S   process and split S into separate arguments
  -v, --debug            print verbose information for each processing step
      --help             display this help and exit
      --version          output version information and exit

A mere - implies -i.  If no COMMAND, print the resulting environment.
"""

pure variable_reference_error(text: Str, at: Int) -> Str {
  let prefix = r"only ${VARNAME} expansion is supported"
  let length = text.count_chars()
  var end = at + 1
  if end < length and text[end..end + 1] == "{" {
    end += 1
    while end < length and text[end..end + 1] != "}" { end += 1 }
    if end < length { end += 1 }
  } else {
    while end < length and text[end..end + 1] in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_" { end += 1 }
  }
  f"{prefix}, error at: {text[at..end]}"
}

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
    if state in [0, 1] and ch == "'" { state = 2; started = true; at += 1; continue }
    if state in [0, 1] and ch == "\"" { state = 3; started = true; at += 1; continue }
    if (state == 2 and ch == "'") or (state == 3 and ch == "\"") {
      state = 1
      at += 1
      continue
    }
    if ch == "$" and state != 2 {
      if at + 1 >= length or text[at + 1..at + 2] != "{" {
        split_error(variable_reference_error(text, at))
      }
      var end = at + 2
      while end < length and text[end..end + 1] != "}" and text[end..end + 1] in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_" { end += 1 }
      if text[end..end + 1] != "}" or end == at + 2 {
        split_error(variable_reference_error(text, at))
      }
      let name = text[at + 2..end]
      if "0123456789".find(name[0..1]) != null {
        split_error(variable_reference_error(text, at))
      }
      word += env.get_or(name, "") ?? ""
      started = true
      state = if state == 0 { 1 } else { state }
      at = end + 1
      continue
    }
    if ch == "\\" {
      if at + 1 >= length { split_error("invalid backslash at end of string in -S") }
      let next = text[at + 1..at + 2]
      if state == 2 and next not in ["\\", "'"] {
        word += "\\"
        started = true
        at += 1
        continue
      }
      if state == 3 and next == "c" { split_error("'\\c' must not appear in double-quoted -S string") }
      if next == "\n" { at += 2; continue }
      if state == 1 and next == "_" {
        words += [word]
        word = ""
        started = false
        state = 0
        at += 2
        continue
      }
      if state != 3 and state != 2 and next == "c" {
        if started { words += [word] }
        return words
      }
      if next in ["\\", "'", "\"", "$", "#"] {
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
        split_error(f"invalid sequence '\\{next}' in -S")
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

proc print_environment(environment: Map[Str, Str], null_delimited: Bool) [error, io] {
  for name in environment.keys() {
    let value = environment.get(name) ?? ""
    io.write_stdout(f"{name}={value}{if null_delimited { "\0" } else { "\n" }}")
  }
}

proc environment_map(ignore_inherited: Bool, unset_names: List[Str], assignments: Map[Str, Str]) [env, error] -> Result[Map[Str, Str]] {
  var environment: Map[Str, Str] = {}
  if ! ignore_inherited {
    for item in env.list()? { environment[item.name] = item.value }
  }
  for name in unset_names { environment = environment.remove(name) }
  for name in assignments.keys() { environment[name] = assignments.get(name) ?? "" }
  Ok(environment)
}

proc split_words(source: Bytes, verbose: Bool) [process, env, error, io] -> List[Bytes] {
  let text = match source.utf8() { Ok(text) => text, Err(_) => { split_error("invalid split string"); "" } }
  let words = split_string(text)
  if verbose and ! words.is_empty() {
    io.write_stderr(f"split -S:  {gnu.quote(text)}\n")
    io.write_stderr(f" into:     {gnu.quote(words[0])}\n")
    for word in words[1..] { io.write_stderr(f"     &     {gnu.quote(word)}\n") }
  }
  collect { for word in words { yield bytes.from_text(word) } }
}

proc run_command(command_argv: List[Bytes], cwd: Path?, environment: Map[Str, Str], argv0: Bytes?, verbose: Bool, ignore_inherited: Bool) [fs, process, env, error, io] {
  if command_argv[0] == b"" {
    gnu.error("'': No such file or directory")
    exit 127
  }

  if let directory = cwd {
    match fs.stat(directory) {
      Ok(metadata) => {
        if metadata.kind != "dir" {
          gnu.error(f"cannot change directory to {gnu.quote(directory.display())}: Not a directory")
          exit 125
        }
      }
      Err(failure) => {
        gnu.error(f"cannot change directory to {gnu.quote(directory.display())}: {gnu.strerror(failure)}")
        exit 125
      }
    }
  }

  let argv0_text: Str? = if let selected = argv0 {
    match selected.utf8() {
      Ok(text) => text
      Err(_) => { gnu.error("--argv0 argument is not valid UTF-8"); exit 125 }
    }
  } else { null }

  let words: List[Path] = collect {
    for argument in command_argv { yield Path.parse_bytes(argument)? }
  }
  let target = words[0]
  if verbose {
    if let selected = argv0 { io.write_stderr(f"argv0:     {gnu.quote_bytes(selected)}\n") }
    io.write_stderr(f"executing: {gnu.quote_bytes(command_argv[0], always: false)}\n")
    var debug_argv = command_argv
    if let selected = argv0 { debug_argv[0] = selected }
    for index in range(debug_argv.len()) {
      io.write_stderr(f"   arg[{index}]= {gnu.quote_bytes(debug_argv[index])}\n")
    }
  }

  var search_environment: Map[Str, Str] = {}
  if let Ok(path_value) = environment.get("PATH") {
    search_environment["PATH"] = path_value
  } else if ! ignore_inherited {
    if let Ok(path_value) = env.get("PATH") {
      search_environment["PATH"] = path_value
    } else {
      search_environment["PATH"] = "/bin:/usr/bin"
    }
  } else {
    search_environment["PATH"] = "/bin:/usr/bin"
  }

  env (search_environment) {
    let executable = match process.which(target) {
      Ok(executable_path) => executable_path
      Err(_) => target
    }
    let command = if let directory = cwd {
      process.command_argv(executable, words, cwd: directory)
    } else {
      process.command_argv(executable, words)
    }
    match unix.exec_env(command, environment, argv0: argv0_text) {
      Ok(_) => return
      Err(failure) => {
        let number = gnu.errno(failure)
        if failure.message == "executable not found" or number == 2 {
          gnu.error(f"{gnu.quote_bytes(command_argv[0])}: No such file or directory")
          if let Ok(command_text) = command_argv[0].utf8() {
            if command_text.find(" ") != null { gnu.error("use -[v]S to pass options in shebang lines") }
          }
          exit 127
        }
        if let directory = cwd {
          gnu.error(f"cannot change directory to {gnu.quote(directory.display())}: {gnu.strerror(failure)}")
          exit 125
        }
        if failure.message in ["executable was found but is not executable", "path is not an executable file"] or number == 13 {
          gnu.error(f"{gnu.quote_bytes(command_argv[0])}: Permission denied")
          if let Ok(command_text) = command_argv[0].utf8() {
            if command_text.find(" ") != null { gnu.error("use -[v]S to pass options in shebang lines") }
          }
          exit 126
        }
        if number != 0 {
          gnu.error(f"{gnu.quote_bytes(command_argv[0])}: {gnu.strerror(failure)}")
          exit 126
        }
        gnu.error(failure.message)
        exit 125
      }
    }
  }
}

proc main(...raw: List[Bytes]) [process, env, error, io] {
  var argv = raw
  var cwd: Path? = null
  var null_delimited = false
  var ignore_inherited = false
  var verbose = false
  var argv0: Bytes? = null
  var unset_names: List[Str] = []
  var assignments: Map[Str, Str] = {}
  var option_index = 0

  while option_index < argv.len() {
    let raw_word = argv[option_index]
    if raw_word == b"--" { option_index += 1; break }
    if raw_word == b"--help" { gnu.help(USAGE); return }
    if raw_word == b"--version" { gnu.version("env"); return }
    if raw_word == b"--null" { null_delimited = true; option_index += 1; continue }
    if raw_word == b"--ignore-environment" or raw_word == b"-" { ignore_inherited = true; option_index += 1; continue }
    if raw_word == b"--debug" { verbose = true; option_index += 1; continue }

    if raw_word == b"--argv0" or byte_prefix(raw_word, b"--argv0=") {
      if raw_word == b"--argv0" {
        if option_index + 1 >= argv.len() { gnu.error("option '--argv0' requires an argument"); exit 125 }
        argv0 = argv[option_index + 1]
        option_index += 2
      } else {
        argv0 = raw_word[8..]
        option_index += 1
      }
      continue
    }

    if raw_word == b"--unset" or byte_prefix(raw_word, b"--unset=") {
      var name_bytes = b""
      if raw_word == b"--unset" {
        if option_index + 1 >= argv.len() { gnu.error("option '--unset' requires an argument"); exit 125 }
        name_bytes = argv[option_index + 1]
        option_index += 2
      } else {
        name_bytes = raw_word[8..]
        option_index += 1
      }
      let name = name_bytes.utf8()?
      if name == "" or name.find("=") != null { gnu.error(f"cannot unset {gnu.quote(name)}: Invalid argument"); exit 125 }
      unset_names += [name]
      continue
    }

    if raw_word == b"--chdir" or byte_prefix(raw_word, b"--chdir=") {
      if raw_word == b"--chdir" {
        if option_index + 1 >= argv.len() { gnu.error("option '--chdir' requires an argument"); exit 125 }
        cwd = Path.parse_bytes(argv[option_index + 1])?
        option_index += 2
      } else {
        cwd = Path.parse_bytes(raw_word[8..])?
        option_index += 1
      }
      continue
    }

    if raw_word == b"--split-string" or byte_prefix(raw_word, b"--split-string=") {
      var source = b""
      var consumed = 1
      if raw_word == b"--split-string" {
        if option_index + 1 >= argv.len() { gnu.error("option '--split-string' requires an argument"); exit 125 }
        source = argv[option_index + 1]
        consumed = 2
      } else {
        source = raw_word[15..]
      }
      let expanded = split_words(source, verbose)
      argv = argv[0..option_index].extend(expanded).extend(argv[option_index + consumed..])
      continue
    }

    if raw_word in [b"--ignore-signal", b"--default-signal", b"--block-signal", b"--list-signal-handling"] or byte_prefix(raw_word, b"--ignore-signal=") or byte_prefix(raw_word, b"--default-signal=") or byte_prefix(raw_word, b"--block-signal=") {
      gnu.error(f"{gnu.quote_bytes(raw_word, always: false)}: signal options are unsupported by the process command API")
      exit 125
    }

    if raw_word.byte_at(0) == 45 and raw_word.len() > 1 {
      var cursor = 1
      var consume_word = false
      var split_seen = false
      while cursor < raw_word.len() {
        let option = raw_word[cursor..cursor + 1].utf8()?
        cursor += 1
        if option == "i" { ignore_inherited = true; continue }
        if option == "v" { verbose = true; continue }
        if option == "0" { null_delimited = true; continue }
        if option in ["u", "C", "S", "a"] {
          var argument = raw_word[cursor..]
          if argument.len() == 0 {
            if option_index + 1 >= argv.len() { gnu.error(f"option '-{option}' requires an argument"); exit 125 }
            argument = argv[option_index + 1]
            consume_word = true
          }
          if option == "u" {
            let name = argument.utf8()?
            if name == "" or name.find("=") != null { gnu.error(f"cannot unset {gnu.quote(name)}: Invalid argument"); exit 125 }
            unset_names += [name]
          } else if option == "C" {
            cwd = Path.parse_bytes(argument)?
          } else if option == "a" {
            argv0 = argument
          } else {
            let expanded = split_words(argument, verbose)
            let consumed = if consume_word { 2 } else { 1 }
            argv = argv[0..option_index].extend(expanded).extend(argv[option_index + consumed..])
            split_seen = true
          }
          break
        }
        gnu.error(f"invalid option -- '{option}'")
        exit 125
      }
      if split_seen { continue }
      option_index += if consume_word { 2 } else { 1 }
      continue
    }
    break
  }
  argv = argv[option_index..]

  var assignment_index = 0
  while assignment_index < argv.len() and assignment_separator(argv[assignment_index]) != null {
    let separator = assignment_separator(argv[assignment_index]) ?? 0
    let name = argv[assignment_index][0..separator].utf8()?
    let value = argv[assignment_index][separator + 1..].utf8()?
    if name == "" { gnu.error("cannot set '': Invalid argument"); exit 125 }
    assignments[name] = value
    assignment_index += 1
  }

  let environment = environment_map(ignore_inherited, unset_names, assignments)?
  argv = argv[assignment_index..]
  if argv.is_empty() {
    if cwd != null { gnu.error("must specify command with --chdir (-C)"); exit 125 }
    if argv0 != null { gnu.error("must specify command with --argv0 (-a)"); exit 125 }
    print_environment(environment, null_delimited)
    return
  }
  if null_delimited { gnu.error("cannot specify --null (-0) with command"); exit 125 }
  run_command(argv, cwd, environment, argv0, verbose, ignore_inherited)
}
