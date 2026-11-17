##! Reads Linux lock state and renders the lslocks command interface.
use core.lib.scols_escape as scols
use disk_usage as usage
use gnu
use lib.util_linux_io as ul_io
use smart_json as smart
use text_a2 as text
use sys_source as src
use tui

const COLUMNS = ["COMMAND", "PID", "TYPE", "SIZE", "INODE", "MAJ:MIN", "MODE", "M", "START", "END", "PATH", "BLOCKER", "HOLDERS"]
const DEFAULT_COLUMNS = ["COMMAND", "PID", "TYPE", "SIZE", "MODE", "M", "START", "END", "PATH"]
const LONG_OPTIONS = ["bytes", "noinaccessible", "json", "pid", "help", "output", "output-all", "noheadings", "raw", "notruncate", "version", "list-columns", "filter"]
const SHORT_OPTIONS = ["b", "i", "J", "p", "h", "o", "n", "r", "u", "V", "H", "Q"]
const COLUMN_DESCRIPTIONS = [
  "command of the process holding the lock",
  "PID of the process holding the lock",
  "kind of lock",
  "size of the lock, use <number> if --bytes is given",
  "inode number",
  "major:minor device number",
  "lock access mode",
  "mandatory state of the lock: 0 (none), 1 (set)",
  "relative byte offset of the lock",
  "ending offset of the lock",
  "path of the locked file",
  "PID of the process blocking the lock",
  "holders of the lock",
]

type KernelLock = {
  id: Int,
  blocked: Bool,
  kind: Str,
  mandatory: Bool,
  mode: Str,
  pid: Int,
  major: Int,
  minor: Int,
  inode: Int,
  start: Int,
  end: Int,
}

type DescriptorLock = {lock: KernelLock, descriptor: LinuxOpenFile, path: Bytes, size: Int}

type Lock = {
  command: Str,
  command_bytes: Bytes,
  pid: Int,
  kind: Str,
  size: Int,
  inode: Int,
  major: Int,
  minor: Int,
  mode: Str,
  mandatory: Bool,
  start: Int,
  end: Int,
  path: Bytes?,
  blocker: Int?,
  holders: List[Str],
  holders_bytes: List[Bytes],
  blocked: Bool,
}

type FilterToken = {kind: Str, value: Str}
type FilterValue = {kind: Str, value: Str, boolean: Bool}
enum FilterParseError { UnexpectedEnd, UnexpectedToken(Str), UnexpectedHolder, InvalidRegex(Str), InvalidExpression, UnknownFilterColumns(List[Str]) }
type FilterLexed = {tokens: List[FilterToken], echoed: Str}
type FilterProgram = {tokens: List[FilterToken], echoed: Str}
type FilterFailure = {reason: FilterParseError, echoed: Str}
enum ColumnSelectionError { EmptyColumnList, EmptyColumnName, UnknownColumn(Str), TooManyColumns }
type JsonPathReplacement = {marker: Bytes, value: Bytes}
type Options = {
  bytes: Bool,
  no_inaccessible: Bool,
  json: Bool,
  pid: Str?,
  help: Bool,
  output: Str?,
  output_all: Bool,
  noheadings: Bool,
  raw: Bool,
  notruncate: Bool,
  version: Bool,
  list_columns: Bool,
  filter: Str?,
  operands: List[Str],
}

pure long_option_name(argument: Str) -> Str? {
  let name = argument.byte_slice(2).split("=")[0]
  for option in LONG_OPTIONS { return option when option == name }
  let matches = collect {
    for option in LONG_OPTIONS {
      if option.starts_with(name) { yield option }
    }
  }
  if matches.len() == 1 { matches[0] } else { null }
}

pure first_option_diagnostic(argv: List[Str], permute: Bool) -> Str? {
  var index = 0
  while index < argv.len() {
    let argument = argv[index]
    if argument == "--" { return null }
    if argument.starts_with("--") {
      let name = long_option_name(argument)
      if name == null {
        let body = argument.byte_slice(2)
        let typed = body.split("=")[0]
        let matches = collect { for option in LONG_OPTIONS { yield option when option.starts_with(typed) } }
        return f"option is ambiguous: {body}" when matches.len() > 1
        return f"unrecognized option: {body}" when matches.is_empty()
        return null
      }
      let has_value = argument.find("=") != null
      return f"option does not take an argument: {name}" when has_value and name not in ["output", "pid", "filter"]
      return null when ! has_value and name in ["help", "version"]
      if name in ["output", "pid", "filter"] and ! has_value {
        return f"option requires an argument: {name}" when index + 1 == argv.len()
        index += 2
      } else {
        index += 1
      }
      continue
    }
    if argument.starts_with("-") and argument != "-" {
      let shorts = argument.byte_slice(1)
      var offset = 0
      while offset < shorts.byte_len() {
        let code = shorts.byte_slice(offset, length: 1)
        return f"unrecognized option: {code}" when code not in SHORT_OPTIONS
        return null when code in ["h", "V"]
        if code in ["o", "p", "Q"] {
          if offset + 1 < shorts.byte_len() { break }
          return f"option requires an argument: {code}" when index + 1 == argv.len()
          index += 1
          break
        }
        offset += 1
      }
      index += 1
      continue
    }
    return null when ! permute
    index += 1
  }
  null
}

pure human_size(value: Int) -> Str {
  var unit = 1
  var suffix_index = 0
  while suffix_index < 6 and value / unit >= 1024 {
    unit *= 1024
    suffix_index += 1
  }
  var whole = value / unit
  var remainder = value % unit
  var thousandths = 0
  for _ in range(3) {
    var digit = 0
    var next = 0
    for _ in range(10) {
      if next >= unit - remainder { next -= unit - remainder; digit += 1 } else { next += remainder }
    }
    thousandths = thousandths * 10 + digit
    remainder = next
  }
  var tenth = (thousandths + 50) / 100
  if tenth == 10 { whole += 1; tenth = 0 }
  let suffix = ["B", "K", "M", "G", "T", "P", "E"][suffix_index]
  if tenth == 0 { f"{whole}{suffix}" } else { f"{whole}.{tenth}{suffix}" }
}

pure decimal(value: Str) -> Int? {
  let parsed = value.parse_int()
  return null when parsed is Err(_)
  parsed ?? 0
}

pure hexadecimal(value: Str) -> Int? {
  return null when value == ""
  var number = 0
  for digit in value.lower() {
    let part = "0123456789abcdef".find(digit) ?? -1
    return null when part < 0 or number > (9223372036854775807 - part) / 16
    number = number * 16 + part
  }
  number
}

pure lock_offset(value: Str) -> Int? {
  return 0 when value == "EOF"
  decimal(value)
}

pure parse_kernel_lock(line: Str) -> KernelLock? {
  let fields = src.parse_words(line)
  return null when fields.len() < 8
  let id_text = fields[0].replace(":", with: "")
  let id = decimal(id_text)
  return null when id == null
  var at = 1
  var blocked = false
  if fields[at] == "->" {
    blocked = true
    at += 1
  }
  return null when fields.len() < at + 7
  let device = fields[at + 4].split(":")
  return null when device.len() != 3
  let major = hexadecimal(device[0])
  let minor = hexadecimal(device[1])
  let inode = decimal(device[2])
  let start = lock_offset(fields[at + 5])
  let end = lock_offset(fields[at + 6])
  let pid = decimal(fields[at + 3])
  return null when major == null or minor == null or inode == null or start == null or end == null or pid == null
  {
    id: id ?? 0,
    blocked: blocked,
    kind: fields[at],
    mandatory: fields[at + 1].starts_with("M"),
    mode: fields[at + 2],
    pid: pid ?? 0,
    major: major ?? 0,
    minor: minor ?? 0,
    inode: inode ?? 0,
    start: start ?? 0,
    end: end ?? 0,
  }
}

pure same_lock(left: KernelLock, right: KernelLock) -> Bool {
  left.start == right.start and left.end == right.end and left.inode == right.inode and
    left.major == right.major and left.minor == right.minor and left.mandatory == right.mandatory and
    left.blocked == right.blocked and left.kind == right.kind and left.mode == right.mode
}

proc descriptor_locks(files: List[LinuxOpenFile]) [fs] -> List[DescriptorLock] {
  collect {
    for descriptor in files {
      continue when descriptor.fd < 0
      let descriptor_path = descriptor.path.bytes()
      let source = fp"/proc/{descriptor.pid}/fdinfo/{descriptor.fd}".read_text()
      if let Ok(text) = source {
        for line in text.lines() {
          continue when ! line.starts_with("lock:\t")
          if let lock = parse_kernel_lock(line.byte_slice(6)) {
            let stat = fs.stat(fp"/proc/{descriptor.pid}/fd/{descriptor.fd}", follow_symlinks: true)
            let size = if let Ok(metadata) = stat { metadata.size } else { 0 }
            yield {lock: lock, descriptor: descriptor, path: descriptor_path, size: size}
          }
        }
      }
    }
  }
}

pure file_path(files: List[LinuxOpenFile], pid: Int, inode: Int) -> LinuxOpenFile? {
  for file in files {
    if file.pid == pid and file.fd >= 0 and file.inode == inode { return file }
  }
  null
}

proc live_lock_file(pid: Int, inode: Int) [fs, process] -> LinuxOpenFile? {
  let observed = try { linux.open_files(pid)?.collect() }
  guard let files = observed else { return null }
  file_path(files, pid, inode)
}

pure descriptor_for_lock(fallback: List[DescriptorLock], lock: KernelLock) -> DescriptorLock? {
  for item in fallback {
    if ! item.lock.blocked and same_lock(lock, item.lock) { return item }
  }
  null
}

proc holders_for_lock(fallback: List[DescriptorLock], lock: KernelLock) [fs] -> List[Bytes] {
  collect {
    for item in fallback {
      if same_lock(lock, item.lock) {
        let name = fp"/proc/{item.descriptor.pid}/comm".read_bytes() ?? bytes.from_text(item.descriptor.command)
        let command = if name.ends_with(b"\n") { name[..name.len() - 1] } else { name }
        yield bytes.concat([bytes.from_text(f"{item.descriptor.pid},"), command, bytes.from_text(f",{item.descriptor.fd}")])
      }
    }
  }
}

proc mount_fallback(lock: KernelLock) [fs] -> Bytes? {
  let content = fp"/proc/self/mountinfo".read_bytes() ?? b""
  let device = bytes.from_text(f"{lock.major}:{lock.minor}")
  var target: Bytes? = null
  for line in content.lines() {
    var fields: List[Bytes] = []
    var start = 0
    for index in range(line.len()) {
      if line.byte_at(index) == 32 {
        fields += [line[start..index]]
        start = index + 1
        if fields.len() == 5 { break }
      }
    }
    if fields.len() >= 5 and fields[2] == device {
      let field = fields[4]
      var parts: List[Bytes] = []
      var index = 0
      while index < field.len() {
        let escape = if index + 4 <= field.len() { field[index..index + 4] } else { b"" }
        let decoded = match escape { b"\\040" => b" "; b"\\011" => b"\t"; b"\\012" => b"\n"; b"\\134" => b"\\"; else => b"" }
        if ! decoded.is_empty() { parts += [decoded]; index += 4 } else { parts += [field[index..index + 1]]; index += 1 }
      }
      target = bytes.concat(parts.extend([b"..."]))
    }
  }
  target
}

proc collect(no_inaccessible: Bool) [fs, process, error] -> List[Lock] {
  let files: List[LinuxOpenFile] = linux.open_files()?.collect()
  let fallback = descriptor_locks(files)
  let text = fp"/proc/locks".read_text()?
  var locks: List[KernelLock] = []
  for line in text.lines() {
    if let lock = parse_kernel_lock(line) { locks += [lock] }
  }
  let rows = collect {
    for lock in locks {
      let resolved = descriptor_for_lock(fallback, lock)
      let original_command = if lock.pid > 0 { fp"/proc/{lock.pid}/comm".read_bytes() } else { Err(error.failure("no process")) }
      let needs_owner = ! lock.blocked and original_command is Err(_) and resolved != null
      let pid = if needs_owner { resolved.descriptor.pid } else { lock.pid }
      let command_bytes = if pid < 0 {
        b"(undefined)"
      } else if let Ok(name) = fp"/proc/{pid}/comm".read_bytes() {
        if name.ends_with(b"\n") { name[..name.len() - 1] } else { name }
      } else { b"(unknown)" }
      let command = command_bytes.utf8() ?? filter_byte_text(command_bytes)
      let descriptor_owner = if needs_owner { resolved } else { null }
      let path_file = if lock.pid > 0 { live_lock_file(lock.pid, lock.inode) } else { file_path(files, pid, lock.inode) }
      let lock_path: Bytes? = if descriptor_owner != null { descriptor_owner.path } else if path_file != null { path_file.path.bytes() } else { null }
      let size = if descriptor_owner != null {
        descriptor_owner.size
      } else if path_file != null {
        let metadata = fs.stat(fp"/proc/{pid}/fd/{path_file.fd}", follow_symlinks: true)
        if let Ok(value) = metadata { value.size } else { 0 }
      } else { 0 }
      let visible_path: Bytes? = if lock_path != null { lock_path } else if no_inaccessible { null } else { mount_fallback(lock) }
      continue when no_inaccessible and visible_path == null
      var blocker: Int? = null
      if lock.blocked {
        for held in locks {
          if held.id == lock.id and ! held.blocked {
            let held_command = if held.pid > 0 { fp"/proc/{held.pid}/comm".read_text() } else { Err(error.failure("no process")) }
            let fallback_owner = descriptor_for_lock(fallback, held)
            let owner = if held_command is Err(_) and fallback_owner != null { fallback_owner.descriptor.pid } else { held.pid }
            blocker = if owner == 0 { null } else { owner }
            break
          }
        }
      }
      let holder_bytes = holders_for_lock(fallback, lock)
      yield {
        command: command,
        command_bytes: command_bytes,
        pid: pid,
        kind: lock.kind,
        size: size,
        inode: lock.inode,
        major: lock.major,
        minor: lock.minor,
        mode: lock.mode,
        mandatory: lock.mandatory,
        start: lock.start,
        end: lock.end,
        path: visible_path,
        blocker: blocker,
        holders: [holder.utf8() ?? filter_byte_text(holder) for holder in holder_bytes],
        holders_bytes: holder_bytes,
        blocked: lock.blocked,
      }
    }
  }
  collect {
    for index in range(rows.len()) { yield rows[rows.len() - index - 1] }
  }
}

pure canonical_column(name: Str) -> Str? {
  if ! rx"^[A-Za-z0-9:]+$".matches(name) { return null }
  let upper = name.upper()
  for column in COLUMNS { return column when column == upper }
  null
}

pure select_columns(output: Str?, output_all: Bool, environment: Str?) -> Result[List[Str], ColumnSelectionError] {
  var selected = if output_all { COLUMNS } else { DEFAULT_COLUMNS }
  if output == null and environment == null { return Ok(selected) }
  let value = if output != null { output ?? "" } else { environment ?? "" }
  let extend = value.starts_with("+")
  let list = if extend { value.byte_slice(1) } else { value }
  let names = list.split(",")
  if names.is_empty() or (names.len() == 1 and names[0] == "") { return Err(EmptyColumnList) }
  if ! extend { selected = [] }
  var offset = 0
  for index in range(names.len()) {
    let name = names[index]
    if name == "" {
      return Err(UnknownColumn(",")) when index == names.len() - 2 and names[-1] == ""
      return Err(UnknownColumn(names[index - 1] + ",")) when index == names.len() - 1 and index > 0
      return Err(UnknownColumn(",")) when index == names.len() - 1
      return Err(EmptyColumnName)
    }
    let column = canonical_column(name)
    return Err(UnknownColumn(list.byte_slice(offset))) when column == null
    return Err(TooManyColumns) when selected.len() == 26
    selected += [column ?? ""]
    offset += name.byte_len() + 1
  }
  Ok(selected)
}

pure column_value(lock: Lock, column: Str) -> FilterValue {
  match column {
    "PID" => {kind: "number", value: f"{lock.pid}", boolean: false}
    "TYPE" => {kind: "string", value: lock.kind, boolean: false}
    "SIZE" => {kind: if lock.size == 0 { "empty" } else { "number" }, value: f"{lock.size}", boolean: false}
    "INODE" => {kind: "number", value: f"{lock.inode}", boolean: false}
    "MAJ:MIN" => {kind: "string", value: f"{lock.major}:{lock.minor}", boolean: false}
    "MODE" => {kind: "string", value: lock.mode + (if lock.blocked { "*" } else { "" }), boolean: false}
    "M" => {kind: "boolean", value: "", boolean: lock.mandatory}
    "START" => {kind: "number", value: f"{lock.start}", boolean: false}
    "END" => {kind: "number", value: f"{lock.end}", boolean: false}
    "PATH" => {kind: "string", value: if let path_bytes = lock.path { path_bytes.utf8() ?? filter_byte_text(path_bytes) } else { "" }, boolean: false}
    "BLOCKER" => {kind: if lock.blocker == null { "empty" } else { "number" }, value: f"{lock.blocker ?? 0}", boolean: false}
    "HOLDERS" => {kind: "string", value: lock.holders.join("\n"), boolean: false}
    else => {kind: "string", value: lock.command, boolean: false}
  }
}

pure filter_tokens(text: Str) -> Result[FilterLexed, FilterFailure] {
  var output: List[FilterToken] = []
  var echoed = ""
  var at = 0
  let characters = [character for character in text]
  let length = characters.len()
  while at < length {
    let current = characters[at]
    if current in [" ", "\t", "\n", "\r"] {
      at += 1
      continue
    }
    if current in ["(", ")"] {
      output += [{kind: current, value: current}]
      at += 1
      continue
    }
    if current in ["'", "\""] {
      let quote = current
      let quote_at = at
      at += 1
      var value = ""
      var closed = false
      while at < length {
        let part = characters[at]
        if part == quote {
          closed = true
          at += 1
          break
        }
        value += part
        at += 1
      }
      if ! closed {
        echoed += quote
        at = quote_at + 1
        continue
      }
      if ! output.is_empty() and output[-1].kind in ["word", "literal", ")"] { return Err({reason: UnexpectedHolder, echoed: echoed}) }
      output += [{kind: "literal", value: value}]
      continue
    }
    let pair = if at + 1 < length { current + characters[at + 1] } else { "" }
    if pair in ["==", "!=", ">=", "<=", "=~", "!~", "&&", "||"] {
      output += [{kind: "operator", value: pair}]
      at += 2
      continue
    }
    if current in [">", "<", "!"] {
      output += [{kind: "operator", value: current}]
      at += 1
      continue
    }
    if ! rx"^[A-Za-z0-9]$".matches(current) {
      echoed += current
      at += 1
      continue
    }
    let start = at
    let numeric_start = rx"^[0-9]$".matches(current)
    while at < length {
      let part = characters[at]
      break when part in [" ", "\t", "\n", "\r", "(", ")", "<", ">", "!", "=", "&", "|", "'", "\"", "[", "]"]
      let code = bytes.from_text(part).byte_at(0) ?? 0
      break when part.byte_len() != 1 or ! ((code >= 65 and code <= 90) or (code >= 97 and code <= 122) or (code >= 48 and code <= 57) or part in ["_", ":", ".", "-", "%", "/"])
      break when numeric_start and part == "-"
      at += 1
    }
    let word = characters[start..at].join("")
    let upper = word.upper()
    if upper in ["AND", "OR", "NOT"] {
      output += [{kind: "operator", value: upper}]
    } else if upper in ["EQ", "NE", "LT", "LE", "GT", "GE"] {
      output += [{kind: "operator", value: match upper { "EQ" => "=="; "NE" => "!="; "LT" => "<"; "LE" => "<="; "GT" => ">"; else => ">=" }}]
    } else {
      if ! output.is_empty() and output[-1].kind in ["word", "literal", ")"] { return Err({reason: UnexpectedHolder, echoed: echoed}) }
      output += [{kind: "word", value: word}]
    }
  }
  Ok({tokens: output, echoed: echoed})
}

pure operator_precedence(operator: Str) -> Int {
  return 1 when operator in ["OR", "||", "AND", "&&"]
  2
}

pure filter_integer(value: Str) -> Int? {
  if rx"^[0-9]+$".matches(value) { return decimal(value) }
  return null when ! rx"^[0-9]+[KMGTPEZY](iB)?$".matches(value)
  usage.parse_size(value)
}

pure filter_number(value: Str) -> Bool {
  filter_integer(value) != null or rx"^[0-9]+\.[0-9]+$".matches(value)
}

pure filter_program(tokens: List[FilterToken]) -> Result[List[FilterToken], FilterParseError] {
  var output: List[FilterToken] = []
  var operators: List[FilterToken] = []
  var expects_value = true
  for token in tokens {
    if token.kind in ["word", "literal"] {
      return Err(UnexpectedHolder) when ! expects_value
      output += [token]
      expects_value = false
    } else if token.kind == "(" {
      return Err(UnexpectedToken("(")) when ! expects_value
      operators += [token]
      expects_value = true
    } else if token.kind == ")" {
      return Err(InvalidExpression) when expects_value
      var found = false
      while ! operators.is_empty() {
        let top = operators[-1]
        operators = operators[..operators.len() - 1]
        if top.kind == "(" { found = true; break }
        output += [top]
      }
      return Err(UnexpectedToken(")")) when ! found
      expects_value = false
    } else {
      let operator = if token.value == "!" and expects_value { "NOT" } else { token.value }
      return Err(InvalidExpression) when operator in ["AND", "&&", "OR", "||", "==", "!=", ">", "<", ">=", "<=", "=~", "!~"] and expects_value
      return Err(InvalidExpression) when operator in ["NOT", "!"] and ! expects_value
      let unary = operator in ["NOT", "!"] and expects_value
      while ! unary and ! operators.is_empty() and operators[-1].kind == "operator" and
          operator_precedence(operators[-1].value) >= operator_precedence(operator) {
        output += [operators[-1]]
        operators = operators[..operators.len() - 1]
      }
      operators += [{kind: "operator", value: operator}]
      expects_value = true
    }
  }
  return Err(UnexpectedEnd) when expects_value
  return Err(InvalidExpression) when output.is_empty()
  while ! operators.is_empty() {
    let top = operators[-1]
    operators = operators[..operators.len() - 1]
    return Err(UnexpectedEnd) when top.kind == "("
    output += [top]
  }
  var depth = 0
  var unknown: List[Str] = []
  for token in output {
    if token.kind == "word" {
      let upper = token.value.upper()
      if canonical_column(token.value) == null and upper not in ["TRUE", "FALSE"] and ! filter_number(token.value) {
        unknown += [token.value]
      }
    }
    if token.kind in ["word", "literal"] {
      depth += 1
    } else if token.value in ["NOT", "!"] {
      return Err(InvalidExpression) when depth < 1
    } else {
      return Err(InvalidExpression) when depth < 2
      depth -= 1
    }
  }
  return Err(InvalidExpression) when depth != 1
  if ! unknown.is_empty() { return Err(UnknownFilterColumns(unknown)) }
  Ok(output)
}

pure parse_filter(text: Str) -> Result[FilterProgram, FilterFailure] {
  if text == "" { return Ok({tokens: [], echoed: ""}) }
  let lexed = filter_tokens(text)
  if let Err(failure) = lexed { return Err(failure) }
  let tokens = lexed ?? {tokens: [], echoed: ""}
  let parsed = filter_program(tokens.tokens)
  if let Ok(program) = parsed {
    for index in range(program.len()) {
      if program[index].value in ["=~", "!~"] and index > 0 and program[index - 1].kind == "literal" {
        let pattern = program[index - 1].value
        if let Err(failure) = regex.compile(pattern) {
          let detail = failure.message
          let reason = if "open group" in detail { "Missing ')'" } else if "unclosed character class" in detail { "Missing ']'" } else if "after dash" in detail { "Invalid character range" } else if "repetition operator" in detail { "Repetition not preceded by valid expression" } else { detail }
          return Err({reason: InvalidRegex(reason), echoed: tokens.echoed})
        }
      }
    }
    return Ok({tokens: program, echoed: tokens.echoed})
  }
  if let Err(reason) = parsed { return Err({reason: reason, echoed: tokens.echoed}) }
  Err({reason: InvalidExpression, echoed: tokens.echoed})
}

pure filter_echo(text: Str) -> Str {
  match filter_tokens(text) {
    Ok(parsed) => parsed.echoed
    Err(failure) => failure.echoed
  }
}

pure filter_literal(token: FilterToken) -> FilterValue {
  return {kind: "string", value: token.value, boolean: false} when token.kind == "literal"
  let upper = token.value.upper()
  return {kind: "boolean", value: "", boolean: upper == "TRUE"} when upper in ["TRUE", "FALSE"]
  if let column = canonical_column(token.value) { return {kind: "column", value: column, boolean: false} }
  if rx"^[0-9]+\.[0-9]+$".matches(token.value) { return {kind: "float", value: token.value, boolean: false} }
  if let number = filter_integer(token.value) { return {kind: "number", value: f"{number}", boolean: false} }
  {kind: "string", value: token.value, boolean: false}
}

pure regex_byte_text(value: Str) -> Str {
  let raw = bytes.from_text(value)
  var chunks: List[Bytes] = []
  var index = 0
  while index < raw.len() {
    if raw.byte_at(index) == 0 and index + 3 < raw.len() and raw.byte_at(index + 3) == 0 {
      let hex = raw[index + 1..index + 3].utf8() ?? ""
      let byte = hexadecimal(hex) ?? 0
      chunks += [bytes.from_ints([238, 128 + byte / 64, 128 + byte % 64]) ?? b""]
      index += 4
    } else {
      chunks += [raw[index..index + 1]]
      index += 1
    }
  }
  bytes.concat(chunks).utf8() ?? ""
}

pure filter_bytes(value: Str) -> List[Int] {
  let raw = bytes.from_text(value)
  var result: List[Int] = []
  var index = 0
  while index < raw.len() {
    if raw.byte_at(index) == 0 and index + 3 < raw.len() and raw.byte_at(index + 3) == 0 {
      result += [hexadecimal(raw[index + 1..index + 3].utf8() ?? "") ?? 0]
      index += 4
    } else {
      result += [raw.byte_at(index) ?? 0]
      index += 1
    }
  }
  result
}

pure filter_byte_order(left: Str, right: Str) -> Int {
  let l = filter_bytes(left)
  let r = filter_bytes(right)
  let length = if l.len() < r.len() { l.len() } else { r.len() }
  for index in range(length) {
    if l[index] < r[index] { return -1 }
    if l[index] > r[index] { return 1 }
  }
  if l.len() < r.len() { -1 } else if l.len() > r.len() { 1 } else { 0 }
}

pure compare_filter(left: FilterValue, right: FilterValue, operator: Str, lock: Lock) -> Bool {
  let l = if left.kind == "column" { column_value(lock, left.value) } else { left }
  let r = if right.kind == "column" { column_value(lock, right.value) } else { right }
  if l.kind == "empty" or r.kind == "empty" { return false }
  if operator in ["=~", "!~"] {
    let pattern = regex.compile(regex_byte_text(r.value))
    let matched = if let Ok(compiled) = pattern { compiled.matches(regex_byte_text(l.value)) } else { false }
    return if operator == "=~" { matched } else { ! matched }
  }
  if l.kind in ["number", "float"] or r.kind in ["number", "float"] {
    if (l.kind == "string" and l.value.parse_float() is Err(_)) or (r.kind == "string" and r.value.parse_float() is Err(_)) { return operator == "!=" }
    if l.kind == "float" or r.kind == "float" {
      let left_number = if l.kind == "boolean" { if l.boolean { 1.0 } else { 0.0 } } else { l.value.parse_float() ?? 0.0 }
      let right_number = if r.kind == "boolean" { if r.boolean { 1.0 } else { 0.0 } } else { r.value.parse_float() ?? 0.0 }
      return left_number == right_number when operator == "=="
      return left_number != right_number when operator == "!="
      return left_number > right_number when operator == ">"
      return left_number < right_number when operator == "<"
      return left_number >= right_number when operator == ">="
      return left_number <= right_number when operator == "<="
    }
    let left_number = if l.kind == "boolean" { if l.boolean { 1 } else { 0 } } else { decimal(l.value) ?? 0 }
    let right_number = if r.kind == "boolean" { if r.boolean { 1 } else { 0 } } else { decimal(r.value) ?? 0 }
    return left_number == right_number when operator == "=="
    return left_number != right_number when operator == "!="
    return left_number > right_number when operator == ">"
    return left_number < right_number when operator == "<"
    return left_number >= right_number when operator == ">="
    return left_number <= right_number when operator == "<="
  }
  if l.kind == "boolean" {
    let right_bool = if r.kind == "boolean" { r.boolean } else { r.value.upper() == "TRUE" or r.value == "1" }
    return l.boolean == right_bool when operator == "=="
    return l.boolean != right_bool when operator == "!="
  }
  let order = filter_byte_order(l.value, r.value)
  return order == 0 when operator == "=="
  return order != 0 when operator == "!="
  return order > 0 when operator == ">"
  return order < 0 when operator == "<"
  return order >= 0 when operator == ">="
  return order <= 0 when operator == "<="
  false
}

pure filter_truth(value: FilterValue, lock: Lock) -> Bool {
  let resolved = if value.kind == "column" { column_value(lock, value.value) } else { value }
  if resolved.kind == "empty" { return false }
  if resolved.kind == "boolean" { return resolved.boolean }
  if resolved.kind == "number" { return (decimal(resolved.value) ?? 0) != 0 }
  if resolved.kind == "float" { return (resolved.value.parse_float() ?? 0.0) != 0.0 }
  resolved.value != ""
}

pure filter_matches(program: List[FilterToken], lock: Lock) -> Bool {
  var stack: List[FilterValue] = []
  for token in program {
    if token.kind in ["word", "literal"] {
      stack += [filter_literal(token)]
    } else if token.value in ["NOT", "!"] {
      let value = stack[-1]
      stack = stack[..stack.len() - 1]
      stack += [{kind: "boolean", value: "", boolean: ! filter_truth(value, lock)}]
    } else if token.value in ["AND", "&&", "OR", "||"] {
      let right = stack[-1]
      let left = stack[-2]
      stack = stack[..stack.len() - 2]
      let left_bool = filter_truth(left, lock)
      let right_bool = filter_truth(right, lock)
      stack += [{kind: "boolean", value: "", boolean: if token.value in ["AND", "&&"] { left_bool and right_bool } else { left_bool or right_bool }}]
    } else {
      let right = stack[-1]
      let left = stack[-2]
      stack = stack[..stack.len() - 2]
      stack += [{kind: "boolean", value: "", boolean: compare_filter(left, right, token.value, lock)}]
    }
  }
  filter_truth(stack[0], lock)
}

pure lock_cell(lock: Lock, column: Str, as_bytes: Bool, byte_locale: Bool, path_override: Str?) -> Any {
  match column {
    "COMMAND" => lock.command
    "PID" => lock.pid
    "TYPE" => lock.kind
    "SIZE" => if lock.size == 0 { null } else if as_bytes { lock.size } else { human_size(lock.size) }
    "INODE" => lock.inode
    "MAJ:MIN" => f"{lock.major}:{lock.minor}"
    "MODE" => lock.mode + (if lock.blocked { "*" } else { "" })
    "M" => lock.mandatory
    "START" => lock.start
    "END" => lock.end
    "PATH" => if path_override != null { path_override ?? "" } else if let path_bytes = lock.path {
      if let Ok(text) = path_bytes.utf8() { text } else { safe_table_text(path_bytes, column, byte_locale) }
    } else { "" }
    "BLOCKER" => lock.blocker
    "HOLDERS" => lock.holders
    else => null
  }
}

pure table_value(lock: Lock, column: Str, as_bytes: Bool, raw: Bool) -> Bytes {
  if column == "PATH" { return lock.path ?? b"" }
  if column == "COMMAND" { return lock.command_bytes }
  if column == "HOLDERS" {
    return bytes.concat(collect {
      for index in range(lock.holders_bytes.len()) {
        if index > 0 { yield b"\n" }
        yield lock.holders_bytes[index]
      }
    })
  }
  let value = match column {
    "COMMAND" => lock.command
    "PID" => f"{lock.pid}"
    "TYPE" => lock.kind
    "SIZE" => if lock.size == 0 { "" } else if as_bytes { f"{lock.size}" } else { human_size(lock.size) }
    "INODE" => f"{lock.inode}"
    "MAJ:MIN" => if raw { f"{lock.major}:{lock.minor}" } else { f"{lock.major:>3}:{lock.minor:<3}" }
    "MODE" => lock.mode + (if lock.blocked { "*" } else { "" })
    "M" => if lock.mandatory { "1" } else { "0" }
    "START" => f"{lock.start}"
    "END" => f"{lock.end}"
    "BLOCKER" => if let blocker = lock.blocker { f"{blocker}" } else { "" }
    "HOLDERS" => lock.holders.join("\n")
    else => ""
  }
  bytes.from_text(value)
}

pure safe_table_text(raw: Bytes, column: Str, byte_locale: Bool) -> Str {
  scols.safe_table_text(raw, column == "HOLDERS", byte_locale)
}

pure cell_display_width(value: Str, column: Str) -> Int {
  var width = 0
  for line in if column == "HOLDERS" { value.split("\n") } else { [value] } {
    let measured = tui.cell_width(line)
    if measured > width { width = measured }
  }
  width
}

pure column_width_hint(column: Str) -> Int {
  match column {
    "COMMAND" => 15
    "PID" | "TYPE" | "INODE" | "MODE" => 5
    "SIZE" => 4
    "MAJ:MIN" => 6
    "M" => 1
    "START" | "END" => 10
    else => 0
  }
}

pure table_width(columns: List[Str], rows: List[List[Str]], headings: Bool) -> List[Int] {
  collect {
    for index in range(columns.len()) {
      let header_width = tui.cell_width(columns[index])
      var width = if headings { header_width } else { 0 }
      for row in rows {
        let measured = cell_display_width(row[index], columns[index])
        if measured > width { width = measured }
      }
      let minimum = if headings and header_width > 0 { header_width } else { 1 }
      let hint = column_width_hint(columns[index])
      let selected = if width <= minimum {
        minimum
      } else if hint > 0 and width < hint and minimum < hint {
        hint
      } else {
        width
      }
      yield if ! headings and width == 0 { 0 } else { selected }
    }
  }
}

pure pad_left(value: Str, width: Int) -> Str {
  tui.left_pad(value, width)
}

pure pad_right(value: Str, width: Int) -> Str {
  tui.right_pad(value, width)
}

pure render_table(columns: List[Str], locks: List[Lock], headings: Bool, raw: Bool, as_bytes: Bool, notruncate: Bool, terminal: Bool, terminal_width: Int, byte_locale: Bool) -> Bytes {
  let values: List[List[Bytes]] = [[table_value(lock, column, as_bytes, raw) for column in columns] for lock in locks]
  let data: List[List[Str]] = [[safe_table_text(values[row_index][index], columns[index], byte_locale) for index in range(columns.len())] for row_index in range(values.len())]
  var widths = table_width(columns, data, headings)
  var output_width = columns.len() - 1
  for width in widths { output_width += width }
  if terminal and ! raw and ! notruncate and "PATH" in columns and output_width > terminal_width {
    var path_column = -1
    for index in range(columns.len()) {
      if columns[index] == "PATH" { path_column = index; break }
    }
    if path_column >= 0 {
      let excess = output_width - terminal_width
      let minimum = if headings { tui.cell_width("PATH") } else { 1 }
      widths[path_column] = if excess >= widths[path_column] - minimum { minimum } else { widths[path_column] - excess }
    }
  }
  output_width = columns.len() - 1
  for width in widths { output_width += width }
  if terminal and ! raw {
    var changed = true
    while output_width < terminal_width and changed {
      changed = false
      for offset in range(columns.len()) {
        let index = columns.len() - offset - 1
        if widths[index] < column_width_hint(columns[index]) and output_width < terminal_width {
          widths[index] += 1
          output_width += 1
          changed = true
        }
      }
    }
  }
  let rows: List[List[Str]] = if headings { [columns, @data] } else { data }
  let byte_rows: List[List[Bytes]] = if headings {
    [[bytes.from_text(column) for column in columns], @values]
  } else { values }
  var output: Bytes = b""
  for row_index in range(rows.len()) {
    let row = rows[row_index]
    let heading = headings and row_index == 0
    var line_count = 1
    if ! raw and ! heading {
      for index in range(row.len()) {
        let count = row[index].split("\n").len()
        if count > line_count { line_count = count }
      }
    }
    for line_index in range(line_count) {
      var rendered_cells: List[Str] = []
      var raw_cells: List[Bytes] = []
      for index in range(row.len()) {
        let value = if heading {
          row[index]
        } else {
          let pieces = row[index].split("\n")
          if line_index < pieces.len() { pieces[line_index] } else { "" }
        }
        let shown = if terminal and ! raw and ! notruncate and columns[index] == "PATH" and ! heading {
          tui.cell_clip(value, widths[index])
        } else { value }
        let padded = if raw or (index == row.len() - 1 and (shown == "" or columns[index] not in ["PID", "TYPE", "SIZE", "INODE", "START", "END", "BLOCKER"])) {
          shown
        } else if columns[index] in ["PID", "TYPE", "SIZE", "INODE", "START", "END", "BLOCKER"] {
          tui.cell_left_pad(shown, widths[index])
        } else {
          tui.cell_right_pad(shown, widths[index])
        }
        rendered_cells += [padded]
        raw_cells += [if heading { bytes.from_text(row[index]) } else { scols.raw_table_text(byte_rows[row_index][index], byte_locale) }]
      }
      if raw {
        var raw_line: Bytes = b""
        for index in range(raw_cells.len()) {
          if index > 0 { raw_line = bytes.concat([raw_line, b" "]) }
          raw_line = bytes.concat([raw_line, raw_cells[index]])
        }
        output = bytes.concat([output, raw_line, b"\n"])
      } else {
        output = bytes.concat([output, bytes.from_text(rendered_cells.join(" ")), b"\n"])
      }
    }
  }
  output
}

pure json_lock(lock: Lock, columns: List[Str], as_bytes: Bool, byte_locale: Bool, path_override: Str?) -> smart.Object {
  smart.object([smart.m(column.lower(), lock_cell(lock, column, as_bytes, byte_locale, path_override)) for column in columns])
}

pure json_table(rows: List[smart.Object], name: Str, mode: Str) -> Str {
  if mode == "lines" {
    let lines = [smart.render(row, compact: true) for row in rows]
    return lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })
  }
  let document = smart.object([smart.m(name, rows)])
  if mode == "compact" {
    let compact_rows = [smart.render(row, compact: true) for row in rows]
    let compact_array = "[\n" + compact_rows.join(",\n") + (if compact_rows.is_empty() { "" } else { "\n" }) + "]"
    return "{" + (json.encode(name) ?? "\"\"") + ":" + compact_array + "}\n"
  }
  if rows.is_empty() {
    let indentation = smartcols_json_indent(1)
    return "{\n" + indentation + (json.encode(name) ?? "\"\"") + ": [\n\n" + indentation + "]\n}\n"
  }
  smartcols_json(document, 0) + "\n"
}

pure smartcols_json_indent(depth: Int) -> Str {
  var output = ""
  for _ in range(depth) { output += "   " }
  output
}

pure smartcols_json(value: Any, depth: Int) -> Str {
  match value {
    _ is Null => "null"
    flag is Bool => if flag { "true" } else { "false" }
    number is Int => f"{number}"
    text is Str => json.encode(text) ?? "\"\""
    items is List[Any] => {
      return "[]" when items.is_empty()
      let inner = smartcols_json_indent(depth + 1)
      let object_rows = items[0] is smart.Object
      let string_rows = items[0] is Str
      let rows = collect {
        for index in range(items.len()) {
          let item = items[index]
          let indent = if (object_rows or string_rows) and index > 0 { "" } else { inner + (if item is Str { " " } else { "" }) }
          yield indent + smartcols_json(item, depth + 1)
        }
      }
      let separator = if object_rows { "," } else if string_rows { ", " } else { ",\n" }
      "[\n" + rows.join(separator) + "\n" + smartcols_json_indent(depth) + "]"
    }
    node is smart.Object => {
      return "{}" when node.members.is_empty()
      let inner = smartcols_json_indent(depth + 1)
      let rows = collect {
        for entry in node.members {
          yield inner + (json.encode(entry.key) ?? "\"\"") + ": " + smartcols_json(entry.value, depth + 1)
        }
      }
      "{\n" + rows.join(",\n") + "\n" + smartcols_json_indent(depth) + "}"
    }
    else => "null"
  }
}

pure json_path_marker(index: Int) -> Str {
  f"\0xsh-lslocks-path-{index}\0"
}

pure json_string_bytes(value: Bytes) -> Bytes {
  let digits = "0123456789abcdef"
  var output: List[Bytes] = [b"\"",]
  for index in range(value.len()) {
    let byte = value.byte_at(index) ?? 0
    if byte == 34 {
      output += [b"\\\""]
    } else if byte == 92 {
      output += [b"\\\\"]
    } else if byte == 8 {
      output += [b"\\b"]
    } else if byte == 9 {
      output += [b"\\t"]
    } else if byte == 10 {
      output += [b"\\n"]
    } else if byte == 12 {
      output += [b"\\f"]
    } else if byte == 13 {
      output += [b"\\r"]
    } else if byte < 32 {
      output += [bytes.from_text("\\u00" + digits.byte_slice(byte / 16, length: 1) + digits.byte_slice(byte % 16, length: 1))]
    } else {
      output += [value[index..index + 1]]
    }
  }
  output += [b"\""]
  bytes.concat(output)
}

pure replace_json_paths(rendered: Bytes, replacements: List[JsonPathReplacement]) -> Bytes {
  var output: List[Bytes] = []
  var at = 0
  while at < rendered.len() {
    var replaced = false
    for replacement in replacements {
      let end = at + replacement.marker.len()
      if end <= rendered.len() and rendered[at..end] == replacement.marker {
        output += [replacement.value]
        at = end
        replaced = true
        break
      }
    }
    if ! replaced {
      output += [rendered[at..at + 1]]
      at += 1
    }
  }
  bytes.concat(output)
}

pure json_output(locks: List[Lock], columns: List[Str], as_bytes: Bool, mode: Str, byte_locale: Bool) -> Bytes {
  var rows: List[smart.Object] = []
  var replacements: List[JsonPathReplacement] = []
  for index in range(locks.len()) {
    let lock = locks[index]
    var rendered_lock = lock
    var path_override: Str? = null
    if let path_bytes = lock.path {
      if path_bytes.utf8() is Err(_) {
        let marker = json_path_marker(index)
        path_override = marker
        replacements += [{marker: bytes.from_text(json.encode(marker) ?? "\"\""), value: json_string_bytes(path_bytes)}]
      }
    }
    if lock.command_bytes.utf8() is Err(_) {
      let marker = f"\0xsh-lslocks-command-{index}\0"
      rendered_lock.command = marker
      replacements += [{marker: bytes.from_text(json.encode(marker) ?? "\"\""), value: json_string_bytes(lock.command_bytes)}]
    }
    for holder_index in range(lock.holders_bytes.len()) {
      let holder = lock.holders_bytes[holder_index]
      if holder.utf8() is Err(_) {
        let marker = f"\0xsh-lslocks-holder-{index}-{holder_index}\0"
        rendered_lock.holders[holder_index] = marker
        replacements += [{marker: bytes.from_text(json.encode(marker) ?? "\"\""), value: json_string_bytes(holder)}]
      }
    }
    rows += [json_lock(rendered_lock, columns, as_bytes, byte_locale, path_override)]
  }
  replace_json_paths(bytes.from_text(json_table(rows, "locks", mode)), replacements)
}

proc validate_early_option(name: Str, value: Str) [process, env, io, error] {
  if name == "pid" {
    let number = decimal(value.trim())
    let valid = rx"^[ \t\r\n\x0b\x0c]*[+-]?[0-9]+$".matches(value)
    if ! valid or number == null or (number ?? 0) <= 0 or (number ?? 0) > 2147483647 {
      let suffix = if valid { ": Result not representable" } else { "" }
      gnu.error(f"invalid PID argument: '{value}'{suffix}")
      exit 1
    }
  }
  if name == "filter" {
    if let Err(failure) = parse_filter(value) {
      if failure.reason is UnknownFilterColumns(_) { return }
      match failure.reason {
        UnexpectedEnd => gnu.error(f"failed to parse \"{value}\": syntax error, unexpected end of file")
        UnexpectedToken(token) => gnu.error(f"failed to parse \"{value}\": syntax error, unexpected '{token}', expecting end of file")
        UnexpectedHolder => gnu.error(f"failed to parse \"{value}\": syntax error, unexpected HOLDER, expecting end of file")
        InvalidRegex(message) => gnu.error(f"failed to parse \"{value}\": {message}")
        InvalidExpression => gnu.usage_error(f"failed to parse {gnu.quote(value)}: invalid expression")
        UnknownFilterColumns(_) => {}
      }
      exit 1
    }
  }
}

proc validate_early(argv: List[Str], permute: Bool) [process, env, io, error] {
  var index = 0
  var json_seen = false
  var raw = false
  var first_mode = ""
  while index < argv.len() {
    let argument = argv[index]
    return when argument == "--"
    var options: List[Str] = []
    var attached = ""
    var has_attached = false
    if argument.starts_with("--") {
      let name = long_option_name(argument) ?? ""
      options = [name]
      if let equal = argument.find("=") { attached = argument.byte_slice(equal + 1); has_attached = true }
    } else if argument.starts_with("-") and argument != "-" {
      let shorts = argument.byte_slice(1)
      var offset = 0
      while offset < shorts.byte_len() {
        let short = shorts.byte_slice(offset, length: 1)
        let name = match short { "J" => "json"; "r" => "raw"; "p" => "pid"; "Q" => "filter"; "o" => "output"; "h" => "help"; "V" => "version"; else => short }
        options += [name]
        offset += 1
        if name in ["pid", "filter", "output"] {
          if offset < shorts.byte_len() { attached = shorts.byte_slice(offset); has_attached = true }
          break
        }
      }
    } else {
      return when ! permute
      index += 1
      continue
    }
    for name in options {
      return when name in ["help", "version"]
      if name == "json" { json_seen = true; if first_mode == "" { first_mode = name } }
      if name == "raw" { raw = true; if first_mode == "" { first_mode = name } }
      if json_seen and raw { gnu.error(f"options --{first_mode} and --{name} cannot be combined"); exit 1 }
      if name in ["pid", "filter", "output"] {
        var value = attached
        if ! has_attached { index += 1; value = argv[index] }
        validate_early_option(name, value)
      }
    }
    index += 1
  }
}

pure filter_byte_text(value: Bytes) -> Str {
  var result = ""
  for index in range(value.len()) {
    let byte = value.byte_at(index) ?? 0
    if byte >= 128 {
      result += "\0" + scols.hex_byte(byte).byte_slice(2) + "\0"
    } else {
      result += value[index..index + 1].utf8() ?? ""
    }
  }
  result
}

## Runs the lslocks command over Linux procfs state.
export proc main(...arguments: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(arguments)
  var argv = prepared.text
  for index in range(argv.len()) {
    let original = gnu.argument_bytes(argv[index], prepared.raw)
    if original.utf8() is Err(_) {
      if index > 0 and argv[index - 1] in ["-Q", "--filter"] {
        argv[index] = filter_byte_text(original)
      } else if original.starts_with(b"--filter=") {
        argv[index] = "--filter=" + filter_byte_text(original[9..])
      } else if original.starts_with(b"-Q") {
        argv[index] = "-Q" + filter_byte_text(original[2..])
      }
    }
  }
  let permute = env.get("POSIXLY_CORRECT") is Err(_)
  var echoed = ""
  defer { if echoed != "" { ul_io.write_diagnostic_bytes(b""); ul_io.write_text(echoed) } }
  var raw_index = 0
  while raw_index < arguments.len() {
    let prefix = argv[..raw_index]
    if let detail = first_option_diagnostic(prefix, permute) { gnu.usage_error(detail) }
    validate_early(prefix, permute)
    let word = arguments[raw_index]
    let argument = argv[raw_index]
    raw_index += 1
    if word == b"--" { break }
    if argument.starts_with("--") and long_option_name(argument) == "filter" {
      let value = if let equal = argument.find("=") { argument.byte_slice(equal + 1) } else if raw_index < argv.len() { argv[raw_index] } else { "" }
      echoed += filter_echo(value)
    } else if argument.starts_with("-") and ! argument.starts_with("--") {
      for position in range(1, argument.byte_len()) {
        if (word.byte_at(position) ?? 0) >= 128 { break }
        let flag = argument.byte_slice(position, length: 1)
        if flag in ["o", "p", "Q"] {
          if flag == "Q" {
            let value = if position + 1 < argument.byte_len() { argument.byte_slice(position + 1) } else if raw_index < argv.len() { argv[raw_index] } else { "" }
            echoed += filter_echo(value)
          }
          break
        }
        if flag in ["h", "V"] or flag not in SHORT_OPTIONS { break }
      }
    }
    if word.starts_with(b"--") and word.utf8() is Err(_) {
      if ! word.starts_with(b"--output=") and ! word.starts_with(b"--pid=") and ! word.starts_with(b"--filter=") {
        ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: unrecognized option: "), word[2..], bytes.from_text(f"\nTry '{gnu.phrase()} --help' for more information.\n")]))
        exit 1
      }
    }
    var value_kind = ""
    var raw_value: Bytes? = null
    if word.starts_with(b"--output=") { value_kind = "output"; raw_value = word[9..] }
    if word.starts_with(b"--pid=") { value_kind = "pid"; raw_value = word[6..] }
    if word in [b"--output", b"--pid"] and raw_index < arguments.len() {
      value_kind = if word == b"--pid" { "pid" } else { "output" }
      raw_value = arguments[raw_index]
    }
    if word.starts_with(b"-") and ! word.starts_with(b"--") {
      var stop_options = false
      for position in range(1, word.len()) {
        let flag = word.byte_at(position) ?? 0
        if flag >= 128 {
          if let detail = first_option_diagnostic(argv[..raw_index - 1], permute) { gnu.usage_error(detail) }
          let length = if ul_io.byte_locale() { 1 } else if flag >= 194 and flag < 224 { 2 } else if flag >= 224 and flag < 240 { 3 } else if flag >= 240 and flag < 245 { 4 } else { 1 }
          let available = word.len() - position
          let character = if length <= available and word.slice(position, length: length).utf8() is Ok(_) { word.slice(position, length: length) } else { word.slice(position, length: 1) }
          ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: unrecognized option: "), character, bytes.from_text(f"\nTry '{gnu.phrase()} --help' for more information.\n")]))
          exit 1
        }
        if flag in [111, 112, 81] {
          if flag in [111, 112] {
            value_kind = if flag == 112 { "pid" } else { "output" }
            if position + 1 < word.len() { raw_value = word[position + 1..] } else if raw_index < arguments.len() { raw_value = arguments[raw_index] }
          }
          if position + 1 == word.len() { raw_index += 1 }
          break
        }
        if flag in [104, 86] { stop_options = true; break }
        if flag not in [98, 105, 74, 110, 114, 117, 72] { break }
      }
      if stop_options { break }
    } else if word in [b"--help", b"--version"] { break } else if word in [b"--output", b"--pid", b"--filter"] { raw_index += 1 }
    if let value = raw_value {
      if value.utf8() is Err(_) {
        if value_kind == "pid" {
          ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: invalid PID argument: '"), value, b"'\n"]))
          exit 1
        }
        if value_kind == "output" {
          var start = 0
          for index in range(value.len()) {
            if (value.byte_at(index) ?? 0) >= 128 { break }
            if value.byte_at(index) == 44 { start = index + 1 }
          }
          ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: unknown column: "), value[start..], b"\n"]))
          exit 1
        }
      }
    }
  }
  if let detail = first_option_diagnostic(argv, permute) {
    gnu.usage_error(detail)
    return
  }
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    bytes: {form: "-b --bytes", default: false},
    no_inaccessible: {form: "-i --noinaccessible", default: false},
    json: {form: "-J --json", default: false},
    pid: {form: "-p --pid PID"},
    help: {form: "-h --help", default: false, stop: true},
    output: {form: "-o --output LIST"},
    output_all: {form: "--output-all", default: false},
    noheadings: {form: "-n --noheadings", default: false},
    raw: {form: "-r --raw", default: false},
    notruncate: {form: "-u --notruncate", default: false},
    version: {form: "-V --version", default: false, stop: true},
    list_columns: {form: "-H --list-columns", default: false},
    filter: {form: "-Q --filter EXPR"},
    operands: {form: "...ARG"},
  })?
  validate_early(argv, permute)
  if opts.json and opts.raw {
    gnu.error("options --json and --raw cannot be combined")
    exit 1
  }
  if opts.pid != null {
    let value = decimal((opts.pid ?? "").trim())
    let valid = rx"^[ \t\r\n\x0b\x0c]*[+-]?[0-9]+$".matches(opts.pid ?? "")
    if ! valid or value == null or (value ?? 0) <= 0 or (value ?? 0) > 2147483647 {
      let suffix = if valid { ": Result not representable" } else { "" }
      gnu.error(f"invalid PID argument: '{opts.pid ?? ""}'{suffix}")
      exit 1
    }
  }
  var program: List[FilterToken]? = null
  if let expression = if opts.help or opts.version or opts.list_columns { null } else { opts.filter } {
    match parse_filter(expression) {
      Ok(parsed) => { program = parsed.tokens }
      Err(failure) => {
        match failure.reason {
          UnexpectedEnd => gnu.error(f"failed to parse \"{expression}\": syntax error, unexpected end of file")
          UnexpectedToken(token) => gnu.error(f"failed to parse \"{expression}\": syntax error, unexpected '{token}', expecting end of file")
          UnexpectedHolder => gnu.error(f"failed to parse \"{expression}\": syntax error, unexpected HOLDER, expecting end of file")
          InvalidRegex(message) => gnu.error(f"failed to parse \"{expression}\": {message}")
          InvalidExpression => gnu.usage_error(f"failed to parse {gnu.quote(expression)}: invalid expression")
          UnknownFilterColumns(names) => { for name in names { gnu.error(f"unknown column: {name}") }; gnu.error("failed to initialize filter") }
        }
        exit 1
      }
    }
  }
  if opts.help {
    if echoed != "" { ul_io.write_text(echoed); echoed = "" }
    ul_io.help("\nUsage:\n lslocks [options]\n\nList local system locks.\n\nOptions:\n -b, --bytes            print SIZE in bytes rather than in human readable format\n -J, --json             use JSON output format\n -i, --noinaccessible   ignore locks without read permissions\n -n, --noheadings       don't print headings\n -o, --output <list>    output columns (see --list-columns)\n     --output-all       output all columns\n -p, --pid <pid>        display only locks held by this process\n -Q, --filter <expr>    apply display filter\n -r, --raw              use the raw output format\n -u, --notruncate       don't truncate text in columns\n\n -H, --list-columns     list the available columns\n -h, --help             display this help\n -V, --version          display version\n\nFor more details see lslocks(8).\n")
    return
  }
  if opts.version { if echoed != "" { ul_io.write_text(echoed); echoed = "" }; ul_io.version("lslocks"); return }
  let output_environment: Str? = if let Ok(value) = env.get("LSLOCKS_COLUMNS") { value } else { null }
  let json_mode = if let Ok(value) = env.get("LIBSMARTCOLS_JSON") { value } else { "" }
  if opts.list_columns {
    if echoed != "" { ul_io.write_text(echoed); echoed = "" }
    let names = COLUMNS
    let json_types = ["<string>", "<integer>", "<string>", "<string|number>", "<integer>", "<string>", "<string>", "<boolean>", "<integer>", "<integer>", "<string>", "<integer>", "<string>"]
    let rows = collect {
      for index in range(names.len()) { yield [names[index], json_types[index], COLUMN_DESCRIPTIONS[index]] }
    }
    if opts.json {
      let encoded = [smart.object([smart.m("holder", row[0]), smart.m("type", row[1]), smart.m("description", row[2])]) for row in rows]
      ul_io.write_text(json_table(encoded, "lslocks-columns", json_mode))
    } else {
      var holder_width = 0
      var type_width = 0
      for row in rows {
        if row[0].count_chars() > holder_width { holder_width = row[0].count_chars() }
        if row[1].count_chars() > type_width { type_width = row[1].count_chars() }
      }
      let rendered_rows = collect {
        for row in rows {
          let description = if opts.raw { scols.raw_table_text(bytes.from_text(row[2]), false).utf8() ?? "" } else { row[2] }
          yield (if opts.raw { row[0] } else { pad_left(row[0], holder_width) }) + " " +
            (if opts.raw { row[1] } else { pad_right(row[1], type_width) }) + " " + description
        }
      }
      ul_io.write_text(rendered_rows.join("\n") + "\n")
    }
    return
  }
  let selection = select_columns(opts.output, opts.output_all, output_environment)
  if let Err(error) = selection {
    match error {
      EmptyColumnList => exit 1
      EmptyColumnName => exit 1
      UnknownColumn(name) => { gnu.error(f"unknown column: {name}"); exit 1 }
      TooManyColumns => exit 1
    }
    return
  }
  let chosen = selection ?? []
  let all = collect(opts.no_inaccessible)
  return when all.is_empty()
  var rows = all
  if opts.pid != null {
    let selected_pid = decimal((opts.pid ?? "").trim()) ?? 0
    rows = rows |> where .pid == selected_pid |> collect
  }
  if program != null {
    if ! (program ?? []).is_empty() { rows = rows |> where filter_matches(program ?? [], .) |> collect }
  }
  if rows.is_empty() and ! opts.json { return }
  let byte_locale = ul_io.byte_locale()
  if echoed != "" { ul_io.write_text(echoed); echoed = "" }
  if opts.json {
    ul_io.write_bytes(json_output(rows, chosen, opts.bytes, json_mode, byte_locale))
  } else {
    let terminal = unix.isatty(1)
    let terminal_width = if terminal {
      let fallback = if let Ok(value) = env.get("COLUMNS") { decimal(value) ?? 80 } else { 80 }
      if let Ok(size) = unix.window_size(1) { if size.cols > 0 { size.cols } else if fallback > 0 { fallback } else { 80 } } else if fallback > 0 { fallback } else { 80 }
    } else { 80 }
    ul_io.write_bytes(render_table(chosen, rows, ! opts.noheadings, opts.raw, opts.bytes, opts.notruncate, terminal, terminal_width, byte_locale))
  }
}
