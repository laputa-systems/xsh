#!/bin/xsh
use lib.gnu

const USAGE = """Usage: env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]
Set each NAME to VALUE in the environment and run COMMAND.

Options:
  -0, --null                 end each output line with NUL, not newline
  -C, --chdir=DIR            change working directory to DIR
  -S, --split-string=STRING  process and split STRING into separate arguments
  -a, --argv0=ARG             pass ARG as the zeroth argument to COMMAND
  -v, --debug                print verbose information for each processing step
      --block-signal[=SIG]    block delivery of SIG signal(s) to COMMAND
      --default-signal[=SIG]  reset signal handling to default
      --ignore-signal[=SIG]  set signal handling to ignore
      --list-signal-handling list nondefault signal handling
      --help                 display this help and exit
      --version              output version information and exit
"""

type EnvOptions = {
  ignore_environment: Bool,
  unset: List[Str],
  null: Bool,
  chdir: Str?,
  split: Str?,
  argv0: Str?,
  debug: Bool,
  block_signal: List[Str],
  ignore_signal: List[Str],
  default_signal: List[Str],
  list_signals: Bool,
  help: Bool,
  version: Bool,
  items: List[Str],
}

type SignalAction = {number: Int, name: Str, action: Str}

pure is_assignment(item: Str) -> Bool {
  item.find("=") != null
}

pure assignment_name(item: Str) -> Str {
  let equal_at = item.find("=") ?? item.byte_len()
  item.byte_slice(0, length: equal_at)
}

pure assignment_value(item: Str) -> Str {
  let equal_at = item.find("=") ?? item.byte_len()
  item.byte_slice(equal_at + 1)
}

pure utf8_width(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0
  return 1 when lead < 128
  return 2 when lead >= 194 and lead <= 223
  return 3 when lead >= 224 and lead <= 239
  return 4 when lead >= 240 and lead <= 244
  1
}

pure hex_pair(value: Int) -> Str {
  let digits = "0123456789ABCDEF"
  f"{digits.byte_slice(value / 16, length: 1)}{digits.byte_slice(value % 16, length: 1)}"
}

proc debug_quote(value: Str) [env] -> Str {
  let data = bytes.from_text(value)
  var needs_ansi_c = false
  var at = 0
  while at < data.len() {
    let char_width = utf8_width(data, at)
    let byte = data.byte_at(at) ?? 0
    if byte < 32 or byte == 127 { needs_ansi_c = true }
    at += char_width
  }
  if ! needs_ansi_c { return gnu.quote_bytes(data) }
  var out = ""
  at = 0
  while at < data.len() {
    let char_width = utf8_width(data, at)
    let byte = data.byte_at(at) ?? 0
    let escaped = if char_width > 1 {
      data.slice(at, length: char_width).utf8() ?? "\u{fffd}"
    } else {
      match byte {
        7 => "\\a"
        8 => "\\b"
        9 => "\\t"
        10 => "\\n"
        13 => "\\r"
        39 => "\\'"
        92 => "\\\\"
        _ if byte < 32 or byte == 127 => f"\\x{hex_pair(byte)}"
        _ => (bytes.from_ints([byte]) ?? b"").utf8() ?? ""
      }
    }
    out = f"{out}{escaped}"
    at += char_width
  }
  f"$'{out}'"
}

proc split_failure(text: Str, at: Int, message: Str) [process, env] -> Unit {
  let rest = text.byte_slice(at)
  if message.starts_with(r"only ${") {
    gnu.error(f"{message}, error at: {rest}")
  } else {
    gnu.error(message)
  }
  exit 125
}

proc split_words(text: Str) [process, env, error] -> List[Str] {
  let data = bytes.from_text(text)
  var words: List[Str] = []
  var word = ""
  var quote = ""
  var active = false
  var at = 0

  while at < text.byte_len() {
    let char_width = utf8_width(data, at)
    let char = data.slice(at, length: char_width).utf8() ?? "\u{fffd}"
    if quote == "'" {
      if char == "'" { quote = ""; at += 1; continue }
      if char == "\\" and at + 1 < text.byte_len() {
        let next_width = utf8_width(data, at + 1)
        let next = data.slice(at + 1, length: next_width).utf8() ?? "\u{fffd}"
        if next == "'" or next == "\\" { word = f"{word}{next}"; at += 2; continue }
        if next == "\n" { at += 2; continue }
      }
      word = f"{word}{char}"
      active = true
    } else if char == "\\" {
      if at + 1 >= text.byte_len() { split_failure(text, at, "invalid backslash at end of -S string") }
      let next_width = utf8_width(data, at + 1)
      let next_raw = data.slice(at + 1, length: next_width)
      let next_decoded = next_raw.utf8() ?? "\u{fffd}"
      let next = if next_width > 1 { "\u{fffd}" } else { next_decoded }
      if next == "\n" { at += 2; continue }
      if next == "c" {
        if quote == "\"" { split_failure(text, at, "'\\c' must not appear in double-quoted -S string at position 2") }
        if active { words += [word] }
        return words
      }
      if quote == "" and next == "_" {
        if active { words += [word]; word = ""; active = false }
        at += 2
        continue
      }
      if next in ["$", "\\", "'", "\"", "#"] {
        word = f"{word}{next}"
        active = true
        at += 2
        continue
      }
      let replacement = match next {
        "r" => "\r"
        "n" => "\n"
        "t" => "\t"
        "f" => "\u{c}"
        "v" => "\u{b}"
        "_" => " "
        else => ""
      }
      if replacement == "" { split_failure(text, at, f"invalid sequence '\\{next}' in -S at position {at + 1}") }
      word = f"{word}{replacement}"
      active = true
      at += 2
      continue
    } else if quote != "" {
      if char == quote { quote = "" } else if char == "$" {
        if at + 1 >= text.byte_len() or (data.byte_at(at + 1) ?? -1) != 123 { split_failure(text, at, r"only ${VARNAME} expansion is supported") }
        var end = at + 2
        while end < text.byte_len() and (data.byte_at(end) ?? -1) != 125 { end += 1 }
        if end >= text.byte_len() { split_failure(text, at, r"only ${VARNAME} expansion is supported") }
        let name = text.byte_slice(at + 2, length: end - at - 2)
        if ! rx"^[A-Za-z_][A-Za-z0-9_]*$".matches(name) { split_failure(text, at, r"only ${VARNAME} expansion is supported") }
        word = f"{word}{env.get_or(name, "") ?? ""}"
        active = true
        at = end + 1
        continue
      } else { word = f"{word}{char}" }
      active = true
    } else if char == "'" or char == "\"" {
      quote = char
      active = true
    } else if char == "$" {
      if at + 1 >= text.byte_len() or (data.byte_at(at + 1) ?? -1) != 123 { split_failure(text, at, r"only ${VARNAME} expansion is supported") }
      var end = at + 2
      while end < text.byte_len() and (data.byte_at(end) ?? -1) != 125 { end += 1 }
      if end >= text.byte_len() { split_failure(text, at, r"only ${VARNAME} expansion is supported") }
      let name = text.byte_slice(at + 2, length: end - at - 2)
      if ! rx"^[A-Za-z_][A-Za-z0-9_]*$".matches(name) { split_failure(text, at, r"only ${VARNAME} expansion is supported") }
      word = f"{word}{env.get_or(name, "") ?? ""}"
      active = true
      at = end + 1
      continue
    } else if char == "#" and ! active {
      while at < text.byte_len() and text.byte_slice(at, length: 1) != "\n" { at += 1 }
      continue
    } else if char == " " or char == "\t" or char == "\n" or char == "\r" or char == "\u{b}" or char == "\u{c}" {
      if active { words += [word]; word = ""; active = false }
    } else {
      word = f"{word}{char}"
      active = true
    }
    at += char_width
  }

  if quote != "" { split_failure(text, text.byte_len(), f"no terminating quote in -S string at position {text.byte_len()} for quote '{quote}'") }
  if active { words += [word] }
  words
}

pure apply_split(words: List[Str], rest: List[Str]) -> List[Str] {
  words.extend(rest)
}

pure splice(items: List[Str], start: Int, remove_count: Int, insert: List[Str]) -> List[Str] {
  var out: List[Str] = []
  for index in range(start) { out += [items[index]] }
  out += insert
  var index = start + remove_count
  while index < items.len() { out += [items[index]]; index += 1 }
  out
}

proc expand_split_args(argv: List[Str]) [process, env, error] -> List[Str] {
  var pending = argv
  var index = 0
  var scanning_options = true
  var split_count = 0
  while index < pending.len() {
    let arg = pending[index]
    if ! scanning_options {
      index += 1
      continue
    }
    if arg == "--" {
      scanning_options = false
      index += 1
      continue
    }
    if arg == "--split-string" or arg == "-S" {
      if index + 1 >= pending.len() { index += 1; continue }
      let words = split_words(pending[index + 1])
      pending = splice(pending, index, 2, words)
      split_count += 1
      if split_count > 256 { gnu.error("too many nested -S options"); exit 125 }
      continue
    }
    if arg.starts_with("--split-string=") {
      let words = split_words(arg.byte_slice(15))
      pending = splice(pending, index, 1, words)
      split_count += 1
      if split_count > 256 { gnu.error("too many nested -S options"); exit 125 }
      continue
    }
    if arg.starts_with("--split-string ") {
      let words = split_words(arg.byte_slice(15))
      pending = splice(pending, index, 1, words)
      split_count += 1
      if split_count > 256 { gnu.error("too many nested -S options"); exit 125 }
      continue
    }
    if arg.starts_with("-") and ! arg.starts_with("--") and arg != "-" {
      var s_at = 0
      var scan = 1
      while scan < arg.byte_len() and s_at == 0 {
        if arg.byte_slice(scan, length: 1) == "S" { s_at = scan }
        scan += 1
      }
      if s_at > 0 {
        let prefix = arg.byte_slice(0, s_at)
        let suffix = arg.byte_slice(s_at + 1)
        var insert: List[Str] = []
        if prefix != "-" { insert += [prefix] }
        var consumed = 1
        if suffix != "" {
        insert += split_words(suffix)
        } else if index + 1 < pending.len() {
          insert += split_words(pending[index + 1])
          consumed = 2
        } else {
          index += 1
          continue
        }
        pending = splice(pending, index, consumed, insert)
        split_count += 1
        if split_count > 256 { gnu.error("too many nested -S options"); exit 125 }
        continue
      }
      index += 1
      continue
    }
    scanning_options = false
    index += 1
  }
  pending
}

pure normalize_args(argv: List[Str]) -> List[Str] {
  var out: List[Str] = []
  for arg in argv {
    if arg.starts_with("--split-string ") {
      out += ["--split-string", arg.byte_slice(15)]
    } else {
      out += [arg]
    }
  }
  out
}

proc print_environment(overrides: Map[Str, Str], ignored: Bool, unset: List[Str], zero: Bool) [process, env, error, io] {
  var values: Map[Str, Str] = map.empty()
  if ! ignored {
    match env.list() {
      Ok(items) => {
        for item in items { values[item.name] = item.value }
      }
      Err(failure) => {
        gnu.error(gnu.strerror(failure))
        exit 125
      }
    }
  }
  for name in overrides.keys() {
    values[name] = overrides[name]
  }

  let ending = if zero { b"\0" } else { b"\n" }
  for name in values.keys() {
    if name not in unset {
      let line = bytes.concat([bytes.from_text(f"{name}={values[name]}"), ending])
      gnu.write_bytes(line)
    }
  }
}

proc signal_number(name: Str) [process] -> Int? {
  match process.signal(name) {
    Ok(signal) if signal.number > 0 => signal.number
    else => null
  }
}

proc validate_signal_options(options: List[Str]) [process, env, error] {
  for option in options {
    let all = option == ""
    let names = if all { [signal.name for signal in process.signals() if signal.number > 0] } else { option.split(",") }
    for name in names {
      if signal_number(name) == null {
        gnu.error(f"{gnu.quote(name)}: invalid signal")
        exit 125
      }
    }
  }
}

proc apply_signal_action(options: List[Str], action: Str, actions: List[SignalAction]) [process, env, error] -> List[SignalAction] {
  var next = actions
  for option in options {
    let all = option == ""
    let names = if all { [signal.name for signal in process.signals() if signal.number > 0] } else { option.split(",") }
    for name in names {
      let number = signal_number(name)
      if number == null {
        gnu.error(f"{gnu.quote(name)}: invalid signal")
        exit 125
      }
      let signal = process.signal(name)?
      if let Err(_) = process.set_signal_action(signal.name, action) {
        if ! all {
          gnu.error(f"failed to set signal action for signal {signal.number}: Invalid argument")
          exit 125
        }
      } else {
        next += [{number: signal.number, name: signal.name, action: action.upper()}]
      }
    }
  }
  next
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: EnvOptions = cli.applet(
    expand_split_args(normalize_args(argv)),
    {
      gnu: {
        status: 125,
        permute: false,
      },
      ignore_environment: {form: "-i --ignore-environment", default: false},
      unset: {form: "-u --unset NAME", repeated: true},
      null: {form: "-0 --null", default: false},
      chdir: {form: "-C --chdir DIR"},
      split: {form: "-S --split-string STRING"},
      argv0: {form: "-a --argv0 ARG"},
      debug: {form: "-v --debug", default: false},
      block_signal: {form: "--block-signal[=SIGNALS]", repeated: true, optional_default: ""},
      ignore_signal: {form: "--ignore-signal[=SIGNALS]", repeated: true, optional_default: ""},
      default_signal: {form: "--default-signal[=SIGNALS]", repeated: true, optional_default: ""},
      list_signals: {form: "--list-signal-handling", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      items: {form: "...ARG"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("env"); return }

  var items = opts.items
  var ignore_environment = opts.ignore_environment
  if items.len() > 0 and items[0] == "-" {
    var rest: List[Str] = []
    for index in range(1, items.len()) { rest += [items[index]] }
    items = rest
    ignore_environment = true
  }

  var overrides: Map[Str, Str] = map.empty()
  var index = 0
  while index < items.len() and is_assignment(items[index]) {
    let item = items[index]
    let name = assignment_name(item)
    if name == "" {
      gnu.error(f"warning: no name specified for value {gnu.quote_value(assignment_value(item))}")
    } else {
      overrides[name] = assignment_value(item)
    }
    index += 1
  }

  for name in opts.unset {
    if name == "" or name.find("=") != null {
      gnu.error(f"cannot unset {gnu.quote(name)}: Invalid argument")
      exit 125
    }
  }

  if opts.chdir != null and index >= items.len() {
    gnu.usage_error("must specify command with --chdir (-C)", 125)
  }
  if opts.argv0 != null and index >= items.len() {
    gnu.usage_error("must specify command with --argv0 (-a)", 125)
  }

  validate_signal_options(opts.default_signal)
  validate_signal_options(opts.ignore_signal)
  validate_signal_options(opts.block_signal)
  if opts.block_signal.len() > 0 {
    gnu.error("--block-signal is not supported: the runtime cannot block signals for an exec'd child")
    exit 125
  }

  if index >= items.len() {
    print_environment(overrides, ignore_environment, opts.unset, opts.null)
    return
  }

  if opts.null {
    gnu.error("cannot specify --null (-0) with command")
    exit 125
  }

  let command = items[index]
  if ignore_environment {
    gnu.error("option '-i' is not supported: the runtime cannot replace the inherited environment")
    exit 125
  }
  if opts.unset.len() > 0 {
    gnu.error("option '-u' is not supported: the runtime cannot remove variables from a child environment")
    exit 125
  }
  if opts.argv0 != null {
    gnu.error("--argv0 is not supported: the runtime cannot set a child process argv[0]")
    exit 125
  }
  var actions: List[SignalAction] = []
  actions = apply_signal_action(opts.default_signal, "default", actions)
  actions = apply_signal_action(opts.ignore_signal, "ignore", actions)

  if opts.list_signals {
    for signal in process.signals() {
      var selected = ""
      for action in actions {
        if action.number == signal.number { selected = action.action }
      }
      if selected != "" {
        let pad = if signal.name.byte_len() < 10 { [" " for _ in range(10 - signal.name.byte_len())].join("") } else { "" }
        eprint f"{signal.name}{pad} ({signal.number}): {selected}"
      }
    }
  }

  var command_argv = [command]
  index += 1
  while index < items.len() { command_argv += [items[index]]; index += 1 }

  if opts.debug {
    var debug_level = 1
    for arg in argv { if arg.starts_with("-vv") { debug_level = 2 } }
    if debug_level >= 2 {
      io.write_stderr("input args:\n")
      io.write_stderr(f"arg[0]: {debug_quote(gnu.prog())}\n")
      for arg_index in range(argv.len()) {
        io.write_stderr(f"arg[{arg_index + 1}]: {debug_quote(argv[arg_index])}\n")
      }
    }
    io.write_stderr(f"executing: {gnu.quote_maybe(command)}\n")
    for arg_index in range(command_argv.len()) {
      io.write_stderr(f"   arg[{arg_index}]= {debug_quote(command_argv[arg_index])}\n")
    }
  }

  let cwd = if opts.chdir == null { p"." } else { fp"{opts.chdir ?? ""}" }
  if opts.chdir != null {
    match fs.stat(cwd) {
      Ok(meta) if meta.kind == "dir" => {}
      _ => {
        gnu.error(f"cannot change directory to {gnu.quote(opts.chdir ?? "")}")
        exit 125
      }
    }
  }

  if command.find("/") != null {
    let command_path = if command.starts_with("/") { fp"{command}" } else if opts.chdir == null { fp"{command}" } else { fp"{cwd}/{command}" }
    if let Ok(meta) = fs.metadata(command_path) {
      if ! meta.executable {
        gnu.error(f"{gnu.quote(command)}: Permission denied")
        exit 126
      }
    }
  }

  if overrides.keys().len() == 0 {
    if let Err(_) = process.which(command) {
      gnu.error(f"{gnu.quote(command)}: No such file or directory")
      if command.starts_with("'") and command.find("-v") != null {
        gnu.error("use -[v]S to pass options in shebang lines")
      }
      exit 127
    }
    let plan = process.command_argv(command, command_argv, cwd)
    if let Err(failure) = unix.exec(plan) {
      gnu.error(f"{gnu.quote(command)}: {gnu.strerror(failure)}")
      exit 126
    }
    return
  }

  env (overrides) {
    if let Err(_) = process.which(command) {
      gnu.error(f"{gnu.quote(command)}: No such file or directory")
      if command.starts_with("'") and command.find("-v") != null {
        gnu.error("use -[v]S to pass options in shebang lines")
      }
      exit 127
    }
    let plan = process.command_argv(command, command_argv, cwd)
    if let Err(failure) = unix.exec(plan) {
      gnu.error(f"{gnu.quote(command)}: {gnu.strerror(failure)}")
      exit 126
    }
    return
  }
}
