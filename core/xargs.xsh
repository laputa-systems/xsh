#!/bin/xsh
use lib.gnu
use lib.search

error InputError = Invalid(message: Str)
type Item = {data: Bytes, line: Int}
type Lexer = {word: Bytes, quote: Int, escaped: Bool, started: Bool, line: Int, word_line: Int, row_words: Bool, trailing_space: Bool}
type Lexed = {state: Lexer, items: List[Item]}

# Quotes and escapes cross input chunks; a NUL/delimited input bypasses this
# grammar and treats every byte other than its delimiter literally.
pure lex(data: Bytes, state: Lexer, delimiter: Int, replace: Bool, eof: Bool) -> Result[Lexed] {
  var current = state
  var items: List[Item] = []
  var skip_until = 0
  for at in range(data.len()) {
    if at < skip_until { continue }
    let byte = data.byte_at(at) ?? 0
    if delimiter >= 0 {
      if byte == delimiter {
        items += [{data: current.word, line: current.line}]
        current.word = b""; current.line += 1; current.started = false
      } else {
        var end = at + 1
        while end < data.len() and data.byte_at(end) != delimiter { end += 1 }
        current.word = bytes.concat([current.word,data[at..end]]); current.started = true; skip_until = end
      }
      continue
    }
    if byte == 0 { return Err(InputError.Invalid(message: "NUL byte in input; use -0 to delimit arguments")) }
    if current.escaped {
      current.escaped = false
      if byte != 10 { current.row_words = true; current.trailing_space = false; current.word = bytes.concat([current.word, data[at..at + 1]]); current.started = true }
      continue
    }
    if byte == 92 and current.quote != 39 { if ! current.started { current.word_line = current.line }; current.row_words = true; current.trailing_space = false; current.escaped = true; current.started = true; continue }
    if current.quote != 0 {
      if byte == current.quote { current.quote = 0 } else if byte == 10 { return Err(InputError.Invalid(message: "unmatched quote; by default quotes are special to xargs unless you use the -0 option")) } else {
        var end = at + 1
        while end < data.len() {
          let next = data.byte_at(end) ?? 0
          if next == current.quote or next == 10 or next == 0 or (next == 92 and current.quote != 39) { break }
          end += 1
        }
        current.word = bytes.concat([current.word,data[at..end]]); skip_until = end
      }
      continue
    }
    if byte == 34 or byte == 39 { if ! current.started { current.word_line = current.line }; current.row_words = true; current.trailing_space = false; current.quote = byte; current.started = true; continue }
    let space = byte in [9,10,11,12,13,32]
    if space and (! replace or byte == 10) {
      if current.started { items += [{data: current.word, line: current.word_line}] }
      current.word = b""; current.started = false
      if byte == 10 {
        if ! (current.row_words and current.trailing_space) { current.line += 1 }
        current.row_words = false; current.trailing_space = false
      } else if current.row_words { current.trailing_space = true }
    } else if ! (replace and space and ! current.started) {
      if ! current.started { current.word_line = current.line }
      current.row_words = true; current.trailing_space = false
      var end = at + 1
      while end < data.len() {
        let next = data.byte_at(end) ?? 0
        if next in [0,10,34,39,92] or (! replace and next in [9,11,12,13,32]) { break }
        end += 1
      }
      current.word = bytes.concat([current.word,data[at..end]]); current.started = true; skip_until = end
    }
  }
  if eof {
    if current.quote != 0 { return Err(InputError.Invalid(message: "unmatched quote")) }
    if current.escaped { return Err(InputError.Invalid(message: "trailing backslash")) }
    if current.started { items += [{data: current.word, line: current.word_line}] }
    current.word = b""; current.started = false
  }
  Ok({state: current, items: items})
}

pure substitute(text: Bytes, marker: Bytes, value: Bytes) -> Bytes {
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

pure status_code(status: Status) -> Int {
  return 125 when ! status.exited()
  let code = status.exit_code() ?? 1
  if code == 0 { 0 } else if code == 255 { 124 } else { 123 }
}

proc execute(items: List[Item], command: List[Str], replacement: Str, trace: Bool, max_size: Int) -> Result[List[Path]] {
  var argv: List[Path] = []
  if command.is_empty() {
    var chunks: List[Bytes] = []
    for index in range(items.len()) {
      if index > 0 { chunks += [b" "] }
      chunks += [items[index].data]
    }
    gnu.write_bytes(bytes.concat(chunks + [b"\n"]))
    return Ok(argv)
  }
  for word in command {
    let raw = if replacement == "" { bytes.from_text(word) } else { substitute(bytes.from_text(word), bytes.from_text(replacement), items[0].data) }
    argv += [Path.parse_bytes(raw)?]
  }
  if replacement == "" { for item in items { argv += [Path.parse_bytes(item.data)?] } }
  var size = 0
  for word in argv { size += word.bytes().len() + 1 }
  if size > max_size { search.reject("argument list too long")? }
  if trace {
    var texts: List[Str] = []
    for word in argv { texts += [gnu.quote_bytes(word.bytes(), always: false)] }
    eprint texts.join(" ")
  }
  Ok(argv)
}

proc main(...args: List[Str]) {
  var delimiter = -1
  var maximum = 0
  var max_lines = 0
  var max_size = 131072
  var parallel = 1
  var replacement = ""
  var no_empty = false
  var trace = false
  var exact_size = false
  var end_marker: Str? = null
  var input_file: Str? = null
  var at = 0
  var command: List[Str] = []
  while at < args.len() {
    let arg = args[at]; at += 1
    if arg == "--" { command = args[at..]; break }
    if ! arg.starts_with("-") or arg == "-" { command = args[at - 1..]; break }
    var flags: List[Str] = []
    var value: Str? = null
    if arg.starts_with("--") {
      let parts = arg.byte_slice(2).split("=")
      flags = [parts[0]]
      if parts.len() > 1 { value = parts[1..].join("=") }
    } else {
      var offset = 1
      while offset < arg.byte_len() {
        let flag = arg.byte_slice(offset, 1); offset += 1; flags += [flag]
        if flag in ["n","L","s","P","I","E","d","a"] {
          if offset < arg.byte_len() { value = arg.byte_slice(offset) }
          break
        }
      }
    }
    for flag in flags {
      if arg.starts_with("--") and value != null and flag not in ["max-args","max-lines","max-chars","max-procs","replace","eof","delimiter","arg-file"] { gnu.usage_error(f"option {gnu.quote(arg)} does not take an argument") }
      if flag in ["n","max-args","L","max-lines","s","max-chars","P","max-procs","I","replace","E","eof","d","delimiter","a","arg-file"] and value == null {
        if at >= args.len() { gnu.usage_error(f"option {gnu.quote(arg)} requires an argument") }
        value = args[at]; at += 1
      }
      let val = value ?? ""
      match flag {
        "0" | "null" => delimiter = 0
        "r" | "no-run-if-empty" => no_empty = true
        "t" | "verbose" => trace = true
        "x" | "exit" => exact_size = true
        "n" | "max-args" => { maximum = val.parse_int() ?? 0; max_lines = 0; replacement = ""; if maximum < 1 { gnu.usage_error("invalid number of arguments") } }
        "L" | "max-lines" => { max_lines = val.parse_int() ?? 0; maximum = 0; replacement = ""; if max_lines < 1 { gnu.usage_error("invalid number of lines") } }
        "s" | "max-chars" => { max_size = val.parse_int() ?? 0; if max_size < 1 or max_size > 131072 { gnu.usage_error("argument size must be between 1 and 131072") } }
        "P" | "max-procs" => { parallel = val.parse_int() ?? -1; if parallel < 1 { gnu.usage_error("-P requires a positive bounded process count") } }
        "I" | "replace" => { replacement = val; maximum = 1; max_lines = 1; if val == "" { gnu.usage_error("replacement string must not be empty") } }
        "E" | "eof" => end_marker = val
        "a" | "arg-file" => input_file = val
        "d" | "delimiter" => {
          let parsed = if val == "\\n" { 10 } else if val == "\\t" { 9 } else if val == "\\0" { 0 } else if val.byte_len() == 1 { bytes.from_text(val).byte_at(0) ?? -1 } else { -1 }
          if parsed < 0 { gnu.usage_error("delimiter must be one byte") }
          delimiter = parsed
        }
        "p" | "interactive" | "show-limits" | "process-slot-var" => gnu.usage_error(f"option {gnu.quote(arg)} is not supported")
        "help" => { gnu.help("Usage: xargs [OPTION]... [COMMAND [INITIAL-ARG]...]\nBuild and execute command lines from standard input."); return }
        "version" => { gnu.version("xargs"); return }
        _ => gnu.usage_error(f"unrecognized option {gnu.quote(arg)}")
      }
    }
  }
  var initial_size = 0
  for word in command { initial_size += word.byte_len() + 1 }
  if initial_size >= max_size { gnu.usage_error("initial arguments exceed the size limit") }
  if ! command.is_empty() and (replacement == "" or command[0].find(replacement) == null) {
    if process.which(command[0]) is Err(_) {
      if "/" in command[0] and fs.stat(fp"{command[0]}") is Ok(_) { gnu.error(f"{command[0]}: Permission denied"); exit 126 }
      gnu.error(f"{command[0]}: No such file or directory"); exit 127
    }
  }
  var state: Lexer = {word: b"", quote: 0, escaped: false, started: false, line: 0, word_line: 0, row_words: false, trailing_space: false}
  var batch: List[Item] = []
  var size = initial_size
  var batch_lines = 0
  var previous_line = -1
  var active: List[ProcessHandle] = []
  var result = 0
  var any_input = false
  var stopped = false
  var marker_seen = false
  var file_offset = 0
  let reader: Path? = if input_file == null { null } else { fp"{input_file ?? ""}" }
  let outcome = try {
    var source_size = -1
    if reader != null {
      let meta = fs.stat(reader ?? p"", follow_symlinks: true)?
      source_size = if meta.kind == "file" and meta.size > 0 { meta.size } else { -2 }
    }
    while ! stopped {
      let data = if reader == null { io.stdin_read(65536)? } else if source_size == -2 {
        if file_offset == 0 { (reader ?? p"").read_bytes()? } else { b"" }
      } else if file_offset >= source_size { b"" } else {
        let left = source_size - file_offset
        bytes.read_at(reader ?? p"",file_offset,if left < 65536 { left } else { 65536 })?
      }
      file_offset += data.len()
      let eof = data.is_empty()
      let decoded = lex(data, state, delimiter, replacement != "", eof)?
      state = decoded.state
      if state.word.len() + initial_size > max_size { search.reject("argument line too long")? }
      for item in decoded.items {
        if end_marker != null and item.data == bytes.from_text(end_marker ?? "") { stopped = true; marker_seen = true; break }
        any_input = true
        if item.data.len() + initial_size + 1 > max_size { search.reject("argument line too long")? }
        let too_many = maximum > 0 and batch.len() >= maximum
        let too_many_lines = max_lines > 0 and ! batch.is_empty() and item.line != previous_line and batch_lines >= max_lines
        let too_large = size + item.data.len() + 1 > max_size
        if ! batch.is_empty() and (too_many or too_many_lines or too_large) {
          if too_large and exact_size { search.reject("argument list too long")? }
          let argv = execute(batch, command, replacement, trace, max_size)?
          if ! argv.is_empty() {
            let target = argv[0]; let tail = argv[1..]
            let handle = if reader != null { spawn run $target @tail ? } else { spawn run $target @tail < /dev/null ? }
            active += [handle]
          }
          batch = []; size = initial_size; batch_lines = 0; previous_line = -1
          if active.len() >= parallel {
            let ended = process.wait_any(active)?
            let code = status_code(ended.status)
            if code > result { result = code }
            active = active[..ended.index] + active[ended.index + 1..]
            if result >= 124 { stopped = true; break }
          }
        }
        if item.line != previous_line { batch_lines += 1; previous_line = item.line }
        batch += [item]; size += item.data.len() + 1
      }
      if eof { break }
    }
    if (! stopped or marker_seen) and (! batch.is_empty() or (! any_input and ! no_empty and replacement == "")) {
      let argv = execute(batch, command, replacement, trace, max_size)?
      if ! argv.is_empty() {
        let target = argv[0]; let tail = argv[1..]
        let handle = if reader != null { spawn run $target @tail ? } else { spawn run $target @tail < /dev/null ? }
        active += [handle]
      }
    }
    while ! active.is_empty() {
      let ended = process.wait_any(active)?
      let code = status_code(ended.status)
      if code > result { result = code }
      active = active[..ended.index] + active[ended.index + 1..]
    }
  }
  if let Err(failure) = outcome {
    for handle in active { let _ = handle.cancel(signal: "TERM", kill_after: 100ms) }
    match failure { InputError.Invalid {message} => gnu.error(message); _ => gnu.error(failure.message) }
    exit if gnu.errno(failure) in [8,13] { 126 } else if gnu.errno(failure) == 2 { 127 } else { 1 }
  }
  if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  exit result
}
