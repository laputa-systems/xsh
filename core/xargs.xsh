#!/bin/xsh
use lib.gnu
use lib.search

# One input item. `line` is the logical input line it came from, which -L
# counts; a line ending in a blank continues onto the next one.
type Item = {data: Bytes, line: Int}

# Lexer state carried across input chunks: quotes and escapes may span them.
type Lexer = {word: Bytes, quote: Int, escaped: Bool, started: Bool, line: Int, word_line: Int, row_words: Bool, trailing_space: Bool}

# Items found in one chunk. `failure` is the diagnostic for malformed input,
# which GNU reports only after running what was already read.
type Lexed = {state: Lexer, items: List[Item], nul: Bool, failure: Str}

type Settings = {delimiter: Int, input_file: Str?, eof: Str, replace: Str?, max_lines: Int, max_args: Int, max_chars: Int, max_procs: Int, interactive: Bool, no_run_if_empty: Bool, trace: Bool, exit_on_overflow: Bool, slot_var: Str?, open_tty: Bool, show_limits: Bool, command: List[Str]}

type LongOption = {long: Str, takes: Int, key: Str}

# A reason a command cannot be started, with the exit status that reports it.
type Problem = {status: Int, reason: Str}

const DEFAULT_LIMIT = 131072
const LONG_OPTIONS = [
  {long: "null", takes: 0, key: "0"}, {long: "arg-file", takes: 1, key: "a"}, {long: "delimiter", takes: 1, key: "d"},
  {long: "eof", takes: 2, key: "e"}, {long: "replace", takes: 2, key: "i"}, {long: "max-lines", takes: 2, key: "l"},
  {long: "max-args", takes: 1, key: "n"}, {long: "interactive", takes: 0, key: "p"}, {long: "no-run-if-empty", takes: 0, key: "r"},
  {long: "max-chars", takes: 1, key: "s"}, {long: "verbose", takes: 0, key: "t"}, {long: "exit", takes: 0, key: "x"},
  {long: "max-procs", takes: 1, key: "P"}, {long: "process-slot-var", takes: 1, key: "process-slot-var"},
  {long: "show-limits", takes: 0, key: "show-limits"}, {long: "open-tty", takes: 0, key: "o"},
  {long: "version", takes: 0, key: "version"}, {long: "help", takes: 0, key: "help"}
]

pure delimiter_error(spec: Str) -> Str {
  f"Invalid input delimiter specification {spec}: the delimiter must be either a single character or an escape sequence starting with \\."
}

pure octal_value(text: Str) -> Int? {
  if text == "" { return null }
  var value = 0
  for digit in text {
    let number = digit.parse_int() ?? -1
    if number < 0 or number > 7 { return null }
    value = value * 8 + number
  }
  value
}

pure hex_value(text: Str) -> Int? {
  if text == "" { return null }
  var value = 0
  for digit in text.lower() {
    let number = "0123456789abcdef".find(digit)
    if number == null { return null }
    value = value * 16 + (number ?? 0)
  }
  value
}

# The byte named by a -d argument: a single character, or a backslash escape.
pure parse_delimiter(spec: Str) -> Int? {
  if spec.byte_len() == 1 { return bytes.from_text(spec).byte_at(0) }
  if ! spec.starts_with("\\") or spec.byte_len() < 2 { return null }
  let rest = spec.byte_slice(1)
  if rest.byte_len() == 1 {
    return match rest { "a" => 7; "b" => 8; "f" => 12; "n" => 10; "r" => 13; "t" => 9; "v" => 11; "\\" => 92; "0" => 0; _ => null }
  }
  if rest.starts_with("x") {
    let value = hex_value(rest.byte_slice(1))
    return if value != null and rest.byte_len() <= 3 { value } else { null }
  }
  let value = octal_value(rest)
  if value != null and rest.byte_len() <= 3 and (value ?? 256) < 256 { value } else { null }
}

# Split a chunk of input into items: blanks and newlines separate them, quotes
# and backslashes group and escape, and in replace mode only newlines separate.
# A delimiter other than whitespace bypasses that grammar entirely.
pure lex(data: Bytes, state: Lexer, delimiter: Int, replace: Bool, eof: Bool) -> Lexed {
  var current = state
  var items: List[Item] = []
  var nul = false
  var skip_until = 0
  var at = -1
  while at + 1 < data.len() {
    at += 1
    if at < skip_until { at = skip_until - 1; continue }
    let byte = data.byte_at(at) ?? 0
    if delimiter >= 0 {
      if byte == delimiter {
        items += [{data: current.word, line: current.line}]
        current.word = b""; current.line += 1; current.started = false
      } else {
        var end = at + 1
        while end < data.len() and data.byte_at(end) != delimiter { end += 1 }
        current.word = bytes.concat([current.word, data[at..end]]); current.started = true; skip_until = end
      }
      continue
    }
    if byte == 0 { nul = true }
    if current.escaped {
      current.escaped = false
      current.row_words = true; current.trailing_space = byte == 32 or byte == 9
      current.word = bytes.concat([current.word, data[at..at + 1]]); current.started = true
      continue
    }
    if current.quote != 0 {
      if byte == current.quote { current.quote = 0 } else if byte == 10 {
        let kind = if current.quote == 39 { "single" } else { "double" }
        return {state: current, items: items, nul: nul, failure: f"unmatched {kind} quote; by default quotes are special to xargs unless you use the -0 option"}
      } else {
        var end = at + 1
        while end < data.len() {
          let next = data.byte_at(end) ?? 0
          if next == current.quote or next == 10 or next == 0 { break }
          end += 1
        }
        current.word = bytes.concat([current.word, data[at..end]]); skip_until = end
      }
      continue
    }
    if byte == 92 {
      if ! current.started { current.word_line = current.line }
      current.row_words = true; current.trailing_space = false; current.escaped = true; current.started = true
      continue
    }
    if byte == 34 or byte == 39 {
      if ! current.started { current.word_line = current.line }
      current.row_words = true; current.trailing_space = false; current.quote = byte; current.started = true
      continue
    }
    # Between words the vertical whitespace characters separate as well; inside
    # a word only blank and tab do, and \f \v \r stay part of it.
    let blank = byte == 32 or byte == 9 or (! current.started and (byte == 11 or byte == 12 or byte == 13))
    if (blank and ! replace) or byte == 10 {
      if current.started { items += [{data: current.word, line: current.word_line}] }
      current.word = b""; current.started = false
      if byte == 10 {
        # Only a line with arguments that does not end in a blank closes a
        # logical line; blank lines leave a continued line open.
        if current.row_words and (replace or ! current.trailing_space) { current.line += 1 }
        if current.row_words { current.trailing_space = false }
        current.row_words = false
      } else if current.row_words { current.trailing_space = true }
    } else if ! (replace and blank and ! current.started) {
      if ! current.started { current.word_line = current.line }
      current.row_words = true; current.trailing_space = false
      var end = at + 1
      while end < data.len() {
        let next = data.byte_at(end) ?? 0
        if next in [0, 10, 34, 39, 92] or (! replace and (next == 32 or next == 9)) { break }
        end += 1
      }
      current.word = bytes.concat([current.word, data[at..end]]); current.started = true; skip_until = end
    }
  }
  if eof {
    if delimiter < 0 and current.quote != 0 {
      let kind = if current.quote == 39 { "single" } else { "double" }
      return {state: current, items: items, nul: nul, failure: f"unmatched {kind} quote; by default quotes are special to xargs unless you use the -0 option"}
    }
    # An empty word left by quotes at the very end of input is dropped, while
    # one before a newline is kept.
    if current.started and (delimiter >= 0 or ! current.word.is_empty()) { items += [{data: current.word, line: current.word_line}] }
    current.word = b""; current.started = false
  }
  {state: current, items: items, nul: nul, failure: ""}
}

pure substitute(text: Bytes, marker: Bytes, value: Bytes) -> Bytes {
  if marker.is_empty() { return text }
  var out: List[Bytes] = []
  var at = 0
  while at < text.len() {
    let found = search.find_fixed(text, marker, offset: at)
    if found == null { out += [text[at..]]; break }
    let offset = found ?? text.len()
    out += [text[at..offset], value]
    at = offset + marker.len()
  }
  bytes.concat(out)
}

# An argument is a C string: it ends at the first NUL, so a NUL in the input
# hides the rest of that item from the command.
pure c_string(data: Bytes) -> Bytes {
  for at in range(data.len()) { if data.byte_at(at) == 0 { return data[0..at] } }
  data
}

pure status_code(status: Status) -> Int {
  return 125 when ! status.exited()
  let code = status.exit_code() ?? 1
  if code == 0 { 0 } else if code == 255 { 124 } else { 123 }
}

# Parse an option's number the way GNU does, with its two diagnostics.
proc option_number(text: Str, option: Str, minimum: Int) [process, env] -> Int {
  let value = text.parse_int() ?? -9223372036854775807
  if value == -9223372036854775807 { gnu.usage_error(f"invalid number \"{text}\" for -{option} option") }
  return value
}

proc exclusive(previous: Str, current: Str) [process, env] -> Unit {
  eprint f"{gnu.prog()}: warning: options {previous} and {current} are mutually exclusive, ignoring previous {previous} value"
}

proc parse_options(args: List[Str]) [process, env, io] -> Settings {
  var cfg: Settings = {delimiter: -1, input_file: null, eof: "", replace: null, max_lines: 0, max_args: 0, max_chars: DEFAULT_LIMIT, max_procs: 1, interactive: false, no_run_if_empty: false, trace: false, exit_on_overflow: false, slot_var: null, open_tty: false, show_limits: false, command: []}
  var at = 0
  while at < args.len() {
    let arg = args[at]
    if arg == "--" { at += 1; break }
    var keys: List[Str] = []
    var values: List[Str?] = []
    var shown: List[Str] = []
    if arg.starts_with("--") {
      at += 1
      let body = arg.byte_slice(2)
      let eq = body.find("=")
      let name = if eq == null { body } else { body.byte_slice(0, eq ?? 0) }
      var value: Str? = if eq == null { null } else { body.byte_slice((eq ?? 0) + 1) }
      var matches: List[LongOption] = []
      for candidate in LONG_OPTIONS {
        if candidate.long == name { matches = [candidate]; break }
        if candidate.long.starts_with(name) { matches += [candidate] }
      }
      if matches.is_empty() { gnu.usage_error(f"unrecognized option '{arg}'") }
      if matches.len() > 1 {
        var names: List[Str] = []
        for candidate in matches { names += [f"'--{candidate.long}'"] }
        gnu.usage_error(f"option '--{name}' is ambiguous; possibilities: {names.join(" ")}")
      }
      let option = matches[0]
      if option.takes == 0 and value != null { gnu.usage_error(f"option '--{option.long}' doesn't allow an argument") }
      if option.takes == 1 and value == null {
        if at >= args.len() { gnu.usage_error(f"option '--{option.long}' requires an argument") }
        value = args[at]
        at += 1
      }
      keys = [option.key]; values = [value]; shown = [f"--{option.long}"]
    } else if arg.starts_with("-") and arg != "-" {
      at += 1
      var offset = 1
      while offset < arg.byte_len() {
        let flag = arg.byte_slice(offset, 1)
        offset += 1
        if flag in ["0","p","r","t","x","o"] { keys += [flag]; values += [null]; shown += [f"-{flag}"]; continue }
        if flag in ["e","i","l"] {
          keys += [flag]; values += [arg.byte_slice(offset)]; shown += [f"-{flag}"]
          offset = arg.byte_len()
          continue
        }
        if flag in ["a","E","I","L","n","s","P","d"] {
          var value = arg.byte_slice(offset)
          offset = arg.byte_len()
          if value == "" {
            if at >= args.len() { gnu.usage_error(f"option requires an argument -- '{flag}'") }
            value = args[at]
            at += 1
          }
          keys += [flag]; values += [value]; shown += [f"-{flag}"]
          continue
        }
        gnu.usage_error(f"invalid option -- '{flag}'")
      }
    } else { break }
    for index in range(keys.len()) {
      let key = keys[index]
      let value = values[index] ?? ""
      let given = values[index] != null
      match key {
        "0" => cfg.delimiter = 0
        "d" => {
          let parsed = parse_delimiter(value)
          if parsed == null { gnu.error(delimiter_error(value)); exit 1 }
          cfg.delimiter = parsed ?? 0
        }
        "a" => cfg.input_file = value
        "e" | "E" => cfg.eof = value
        "I" | "i" => {
          if cfg.max_args > 0 { exclusive("--max-args", "--replace/-I/-i"); cfg.max_args = 0 }
          if cfg.max_lines > 0 { exclusive("--max-lines", "--replace/-I/-i"); cfg.max_lines = 0 }
          cfg.replace = if key == "i" and ! (given and value != "") { "{}" } else { value }
        }
        "n" => {
          let number = option_number(value, "n", 1)
          if number < 1 { gnu.usage_error(f"value {number} for -n option should be >= 1") }
          if cfg.replace != null { exclusive("--replace", "--max-args/-n"); cfg.replace = null }
          if cfg.max_lines > 0 { exclusive("--max-lines", "--max-args/-n"); cfg.max_lines = 0 }
          cfg.max_args = number
        }
        "L" | "l" => {
          let number = if key == "l" and value == "" { 1 } else { option_number(value, "L", 1) }
          if number < 1 { gnu.usage_error(f"value {number} for -L option should be >= 1") }
          let current = if key == "L" { "-L" } else { "--max-lines/-l" }
          if cfg.replace != null { exclusive("--replace", current); cfg.replace = null }
          if cfg.max_args > 0 { exclusive("--max-args", current); cfg.max_args = 0 }
          cfg.max_lines = number
        }
        "s" => {
          let number = option_number(value, "s", 1)
          if number < 1 { gnu.error(f"value {number} for -s option should be >= 1") }
          if number > DEFAULT_LIMIT { gnu.error(f"value {number} for -s option should be <= {DEFAULT_LIMIT}") }
          cfg.max_chars = if number > DEFAULT_LIMIT { DEFAULT_LIMIT } else { number }
        }
        "P" => {
          let number = option_number(value, "P", 0)
          if number < 0 { gnu.usage_error(f"value {number} for -P option should be >= 0") }
          cfg.max_procs = number
        }
        "p" => { cfg.interactive = true; cfg.trace = true }
        "r" => cfg.no_run_if_empty = true
        "t" => cfg.trace = true
        "x" => cfg.exit_on_overflow = true
        "o" => cfg.open_tty = true
        "process-slot-var" => cfg.slot_var = value
        "show-limits" => cfg.show_limits = true
        "help" => { gnu.help(HELP); exit 0 }
        "version" => { gnu.version("xargs"); exit 0 }
        _ => {}
      }
    }
  }
  cfg.command = args[at..]
  cfg
}

# Why the command cannot run, as an exit status and the system's wording:
# 127 for a missing file, 126 for one that cannot be executed.
proc unrunnable(command: Str) [fs, process] -> Problem? {
  if "/" in command {
    return match fs.stat(Path.parse_bytes(bytes.from_text(command)) ?? p".", follow_symlinks: true) {
      Ok(found) => if found.kind == "dir" or found.mode.bit_and(0o111) == 0 { {status: 126, reason: "Permission denied"} } else { null }
      Err(failure) => {status: 127, reason: gnu.strerror(failure)}
    }
  }
  if process.which(command) is Err(_) { {status: 127, reason: "No such file or directory"} } else { null }
}

# Build the argument vector of one command line from the items of a batch:
# the items follow the initial arguments, or replace the marker inside them.
proc build_argv(items: List[Item], command: List[Str], raw: List[gnu.RawArgument], replace: Bool, marker: Bytes, max_chars: Int) [process, env, error] -> Result[List[Path], Error] {
  var argv: List[Path] = []
  for word in command {
    let text = gnu.argument_bytes(word, raw)
    let joined = if replace and ! items.is_empty() { substitute(text, marker, c_string(items[0].data)) } else { text }
    argv += [Path.parse_bytes(joined)?]
  }
  if ! replace { for item in items { argv += [Path.parse_bytes(c_string(item.data))?] } }
  var total = 0
  for word in argv {
    total += word.bytes().len() + 1
    if replace and word.bytes().len() + 1 >= max_chars { search.reject("command too long")? }
  }
  if replace and total > max_chars { search.reject("argument list too long")? }
  Ok(argv)
}

# Start a command. Its standard input is /dev/null, so it cannot consume the
# items, unless they came from a file (-a), which leaves standard input free.
proc start(argv: List[Path], inherit_stdin: Bool, use_tty: Bool, slot_var: Str?, slot: Int) [process, env, error] -> Result[ProcessHandle, Error] {
  var words = argv
  if slot_var != null { words = [fp"env", fp"{slot_var ?? ""}={slot}"] + argv }
  let target = words[0]
  let tail = words[1..]
  let handle = if use_tty { spawn run $target @tail < /dev/tty ? } else if inherit_stdin { spawn run $target @tail ? } else { spawn run $target @tail < /dev/null ? }
  Ok(handle)
}

# A word as the shell would need it written. Words with an apostrophe use
# double quotes unless they also hold a character that is special inside
# them or in single quotes, in which case the apostrophes are spelled '\\''.
proc shell_quote(raw: Bytes) [env] -> Str {
  var apostrophe = false
  var printable = true
  var special = false
  for at in range(raw.len()) {
    let byte = raw.byte_at(at) ?? 0
    if byte == 39 { apostrophe = true }
    if byte < 32 or byte > 126 { printable = false }
    if byte in [33, 34, 35, 36, 38, 40, 41, 42, 59, 60, 61, 62, 91, 92, 94, 96, 123, 124, 125, 126] { special = true }
  }
  if ! apostrophe or ! printable { return gnu.quote_bytes(raw, always: false) }
  let text = raw.utf8() ?? ""
  if ! special { return f"\"{text}\"" }
  f"'{text.replace("'", with: "'\\''")}'"
}

# Show the command line on standard error (-t) and, for -p, ask whether to
# run it. Returns whether the command should run.
proc announce(argv: List[Path], interactive: Bool) [io, process, env, error] -> Result[Bool, Error] {
  var texts: List[Str] = []
  for word in argv { texts += [shell_quote(word.bytes())] }
  if interactive { return ask(texts.join(" ")) }
  eprint texts.join(" ")
  Ok(true)
}

# Read the answer to a -p prompt from the terminal.
proc ask(command_line: Str) [io, process, env, error] -> Result[Bool, Error] {
  io.write_stderr(command_line)?
  io.flush_stderr()?
  let descriptor = match unix.open_fd(p"/dev/tty") {
    Ok(opened) => opened
    Err(failure) => { gnu.error(f"failed to open /dev/tty for reading: {gnu.strerror(failure)}"); exit 1 }
  }
  io.write_stderr("?...")?
  io.flush_stderr()?
  var answer = ""
  while true {
    let chunk = unix.read_fd(descriptor, 1)?
    if chunk.is_empty() { break }
    let letter = chunk.utf8() ?? ""
    if letter == "\n" { break }
    answer = f"{answer}{letter}"
  }
  unix.close_fd(descriptor)?
  Ok(answer.starts_with("y") or answer.starts_with("Y"))
}

# Ends the run, as GNU xargs does, when the command cannot be started.
proc require_command(command: Str) [fs, process, env] -> Unit {
  let problem = unrunnable(command)
  if problem != null {
    let found = problem ?? {status: 127, reason: ""}
    gnu.error(f"failed to run command {gnu.quote(command)}: {found.reason}")
    exit found.status
  }
}

proc main(...argv: List[Bytes]) {
  let prepared = gnu.prepare_arguments(argv)
  let raw = prepared.raw
  let cfg = parse_options(prepared.text)
  let replace = cfg.replace != null
  let marker = gnu.argument_bytes(cfg.replace ?? "", raw)
  let command = cfg.command
  if cfg.eof != "" and cfg.delimiter >= 0 { eprint f"{gnu.prog()}: warning: the -E option has no effect if -0 or -d is used.\n" }
  var initial_size = 0
  for word in command { initial_size += gnu.argument_bytes(word, raw).len() + 1 }
  if command.is_empty() { initial_size = 5 }
  if cfg.show_limits {
    var environment_bytes = 0
    for entry in env.entries() { environment_bytes += entry.name.len() + entry.value.len() + 2 }
    let upper = 2097152 - environment_bytes - 2048
    eprint f"Your environment variables take up {environment_bytes} bytes\nPOSIX upper limit on argument length (this system): {upper}\nPOSIX smallest allowable upper limit on argument length (all systems): 4096\nMaximum length of command we could actually use: {upper - initial_size}\nSize of command buffer we are actually using: {cfg.max_chars}\nMaximum parallelism (--max-procs must be no greater): 2147483647"
  }
  if initial_size > cfg.max_chars { gnu.error("cannot fit single argument within argument list size limit"); exit 1 }
  if cfg.open_tty and fs.access(p"/dev/tty", read: true) is Err(_) { gnu.error(f"{gnu.quote("/dev/tty")}: No such device or address"); exit 1 }
  var state: Lexer = {word: b"", quote: 0, escaped: false, started: false, line: 0, word_line: 0, row_words: false, trailing_space: false}
  var batch: List[Item] = []
  var size = initial_size
  var batch_lines = 0
  var previous_line = -1
  var active: List[ProcessHandle] = []
  var slots: List[Int] = []
  var result = 0
  var any_input = false
  var stopped = false
  var marker_seen = false
  var warned_nul = false
  var failure = ""
  var checked = false
  var file_offset = 0
  let reader: Path? = if cfg.input_file == null { null } else { Path.parse_bytes(gnu.argument_bytes(cfg.input_file ?? "", raw)) ?? p"." }
  let limit = if cfg.max_procs == 0 { 1000000 } else { cfg.max_procs }
  let outcome = try {
    var source_size = -1
    if reader != null {
      match fs.stat(reader ?? p"", follow_symlinks: true) {
        Ok(meta) => { source_size = if meta.kind == "file" and meta.size > 0 { meta.size } else { -2 } }
        Err(problem) => { gnu.error(f"Cannot open input file {gnu.quote(cfg.input_file ?? "")}: {gnu.strerror(problem)}"); exit 1 }
      }
    }
    while ! stopped {
      var data = b""
      if reader == null { data = io.stdin_read(65536)? } else if source_size == -2 {
        if file_offset == 0 { data = match (reader ?? p"").read_bytes() { Ok(bytes_read) => bytes_read; Err(_) => b"" } }
      } else if file_offset < source_size {
        let left = source_size - file_offset
        data = bytes.read_at(reader ?? p"", file_offset, if left < 65536 { left } else { 65536 })?
      }
      file_offset += data.len()
      let eof = data.is_empty()
      let decoded = lex(data, state, cfg.delimiter, replace, eof)
      state = decoded.state
      if decoded.nul and cfg.delimiter < 0 and ! warned_nul {
        warned_nul = true
        gnu.error("WARNING: a NUL character occurred in the input.  It cannot be passed through in the argument list.  Did you mean to use the --null option?")
      }
      for item in decoded.items {
        if cfg.eof != "" and cfg.delimiter < 0 and item.data == gnu.argument_bytes(cfg.eof, raw) { stopped = true; marker_seen = true; break }
        any_input = true
        let length = c_string(item.data).len()
        if length == 0 and initial_size + 1 > cfg.max_chars { gnu.error("cannot fit single argument within argument list size limit"); exit 1 }
        let too_long = if replace { length + 1 > cfg.max_chars } else { length + initial_size + 1 > cfg.max_chars }
        if too_long and cfg.exit_on_overflow { gnu.error("argument line too long"); exit 1 }
        if too_long { failure = "argument line too long"; break }
        let counted = cfg.max_args > 0 or cfg.max_lines > 0 or replace
        let too_many = cfg.max_args > 0 and batch.len() >= cfg.max_args
        let too_many_lines = (cfg.max_lines > 0 or replace) and ! batch.is_empty() and item.line != previous_line and (replace or batch_lines >= cfg.max_lines)
        let too_large = size + length + 1 > cfg.max_chars
        if ! batch.is_empty() and (too_many or too_many_lines or too_large) {
          if too_large and cfg.exit_on_overflow and counted and ! too_many and ! too_many_lines { search.reject("argument list too long")? }
          let argv = build_argv(batch, command, raw, replace, marker, cfg.max_chars)?
          if command.is_empty() {
            var chunks: List[Bytes] = []
            for index in range(batch.len()) { if index > 0 { chunks += [b" "] }; chunks += [c_string(batch[index].data)] }
            gnu.write_bytes(bytes.concat(chunks + [b"\n"]))
          } else {
            if ! checked { checked = true; require_command(command[0]) }
            var proceed = true
            if cfg.trace { proceed = announce(argv, cfg.interactive)? }
            if proceed {
              var slot = 0
              while slot in slots { slot += 1 }
              active += [start(argv, reader != null, cfg.open_tty, cfg.slot_var, slot)?]
              slots += [slot]
            }
          }
          batch = []; size = initial_size; batch_lines = 0; previous_line = -1
          if active.len() >= limit {
            let ended = process.wait_any(active)?
            let code = status_code(ended.status)
            if code == 124 { gnu.error(f"{command[0]}: exited with status 255; aborting") }
            if code == 125 { gnu.error(f"{command[0]}: terminated by signal {ended.status.signal_number() ?? 0}") }
            if code > result { result = code }
            active = active[..ended.index] + active[ended.index + 1..]
            slots = slots[..ended.index] + slots[ended.index + 1..]
            if result >= 124 { stopped = true; break }
          }
        }
        if item.line != previous_line { batch_lines += 1; previous_line = item.line }
        batch += [item]; size += length + 1
      }
      if failure == "" and decoded.failure != "" { failure = decoded.failure }
      if failure != "" or eof { break }
    }
    if (! stopped or marker_seen) and (! batch.is_empty() or (! any_input and ! cfg.no_run_if_empty and ! replace and failure == "")) {
      let argv = build_argv(batch, command, raw, replace, marker, cfg.max_chars)?
      if command.is_empty() {
        var chunks: List[Bytes] = []
        for index in range(batch.len()) { if index > 0 { chunks += [b" "] }; chunks += [c_string(batch[index].data)] }
        gnu.write_bytes(bytes.concat(chunks + [b"\n"]))
      } else {
        if ! checked { checked = true; require_command(command[0]) }
        var proceed = true
        if cfg.trace { proceed = announce(argv, cfg.interactive)? }
        if proceed {
          var slot = 0
          while slot in slots { slot += 1 }
          active += [start(argv, reader != null, cfg.open_tty, cfg.slot_var, slot)?]
          slots += [slot]
        }
      }
    }
    while ! active.is_empty() {
      let ended = process.wait_any(active)?
      let code = status_code(ended.status)
      if code == 124 { gnu.error(f"{command[0]}: exited with status 255; aborting") }
      if code == 125 { gnu.error(f"{command[0]}: terminated by signal {ended.status.signal_number() ?? 0}") }
      if code > result { result = code }
      active = active[..ended.index] + active[ended.index + 1..]
      slots = slots[..ended.index] + slots[ended.index + 1..]
    }
  }
  if let Err(problem) = outcome {
    for handle in active { let _ = handle.cancel(signal: "TERM", kill_after: 100ms) }
    gnu.error(problem.message)
    exit 1
  }
  if failure != "" { gnu.error(failure); exit 1 }
  if let Err(problem) = io.flush_stdout() { gnu.write_failed(problem) }
  exit result
}

# The text of --help.
const HELP = r"""
  Usage: xargs [OPTION]... [COMMAND [INITIAL-ARGS]...]
  Run COMMAND with arguments INITIAL-ARGS and more arguments read from input.

  Mandatory and optional arguments to long options are also
  mandatory or optional for the corresponding short option.
    -0, --null                   items are separated by a null, not whitespace;
                                   disables quote and backslash processing and
                                   logical EOF processing
    -a, --arg-file=FILE          read arguments from FILE, not standard input
    -d, --delimiter=CHARACTER    items in input stream are separated by CHARACTER,
                                   not by whitespace; disables quote and backslash
                                   processing and logical EOF processing
    -E END                       set logical EOF string; if END occurs as a line
                                   of input, the rest of the input is ignored
                                   (ignored if -0 or -d was specified)
    -e, --eof[=END]              equivalent to -E END if END is specified;
                                   otherwise, there is no end-of-file string
    -I R                         same as --replace=R
    -i, --replace[=R]            replace R in INITIAL-ARGS with names read
                                   from standard input, split at newlines;
                                   if R is unspecified, assume {}
    -L, --max-lines=MAX-LINES    use at most MAX-LINES non-blank input lines per
                                   command line
    -l[MAX-LINES]                similar to -L but defaults to at most one non-
                                   blank input line if MAX-LINES is not specified
    -n, --max-args=MAX-ARGS      use at most MAX-ARGS arguments per command line
    -o, --open-tty               Reopen standard input as /dev/tty in the child
                                   process before executing the command; useful to
                                   run an interactive application.
    -P, --max-procs=MAX-PROCS    run at most MAX-PROCS processes at a time
    -p, --interactive            prompt before running commands
        --process-slot-var=VAR   set environment variable VAR in child processes
    -r, --no-run-if-empty        if there are no arguments, then do not run COMMAND;
                                   if this option is not given, COMMAND will be
                                   run at least once
    -s, --max-chars=MAX-CHARS    limit length of command line to MAX-CHARS
        --show-limits            show limits on command-line length
    -t, --verbose                print commands before executing them
    -x, --exit                   exit if the size (see -s) is exceeded
        --help                   display this help and exit
        --version                output version information and exit

  If COMMAND is omitted, the default is 'echo'.

  Please see also the documentation at https://www.gnu.org/software/findutils/.
  You can report (and track progress on fixing) bugs in the "xargs"
  program via the GNU findutils bug-reporting page at
  https://savannah.gnu.org/bugs/?group=findutils or, if
  you have no web access, by sending email to <bug-findutils@gnu.org>.
  """
