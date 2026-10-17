##! Reads Linux lock state and renders the lslocks command interface.
use disk_usage as usage
use gnu
use smart_json as smart
use text_a2 as text
use sys_mount as mounts
use sys_source as src
use tui

const COLUMNS = ["COMMAND", "PID", "TYPE", "SIZE", "INODE", "MAJ:MIN", "MODE", "M", "START", "END", "PATH", "BLOCKER", "HOLDERS"]
const DEFAULT_COLUMNS = ["COMMAND", "PID", "TYPE", "SIZE", "MODE", "M", "START", "END", "PATH"]
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
  blocked: Bool,
}

type FilterToken = {kind: Str, value: Str}
type FilterValue = {kind: Str, value: Str, boolean: Bool}
enum ColumnSelectionError { UnknownColumn, TooManyColumns }
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

pure human_size(value: Int) -> Str {
  if value < 1024 { f"{value}B" } else { usage.amount(value, {block: 1, human: 1024, label: "Size"}) }
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

pure descriptor_for_lock(fallback: List[DescriptorLock], lock: KernelLock) -> DescriptorLock? {
  for item in fallback {
    if ! item.lock.blocked and same_lock(lock, item.lock) { return item }
  }
  null
}

pure holders_for_lock(fallback: List[DescriptorLock], lock: KernelLock) -> List[Str] {
  collect {
    for item in fallback {
      if same_lock(lock, item.lock) {
        yield f"{item.descriptor.pid},{item.descriptor.command},{item.descriptor.fd}"
      }
    }
  }
}

proc mount_fallback(lock: KernelLock) [fs] -> Bytes? {
  let text = fp"/proc/self/mountinfo".read_text() ?? ""
  let device = f"{lock.major}:{lock.minor}"
  var target: Str? = null
  for line in text.lines() {
    let fields = src.parse_words(line)
    if fields.len() > 4 and fields[2] == device {
      target = mounts.decode_mount_field(fields[4]) + "..."
    }
  }
  if let value = target { bytes.from_text(value) } else { null }
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
      let pid = if lock.pid < 0 and ! lock.blocked and resolved != null { resolved.descriptor.pid } else { lock.pid }
      let command = if lock.pid < 0 {
        if pid > 0 and resolved != null { resolved.descriptor.command } else { "(undefined)" }
      } else {
        var name = ""
        for file in files {
          if file.pid == pid { name = file.command; break }
        }
        if name == "" { "(unknown)" } else { name }
      }
      let path_file = file_path(files, pid, lock.inode)
      let lock_path: Bytes? = if resolved != null and ! lock.blocked { resolved.path } else if path_file != null { path_file.path.bytes() } else { null }
      let size = if resolved != null and ! lock.blocked {
        resolved.size
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
            let owner = if held.pid < 0 {
              let fallback_owner = descriptor_for_lock(fallback, held)
              if fallback_owner != null { fallback_owner.descriptor.pid } else { held.pid }
            } else { held.pid }
            blocker = if owner == 0 { null } else { owner }
            break
          }
        }
      }
      yield {
        command: command,
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
        holders: holders_for_lock(fallback, lock),
        blocked: lock.blocked,
      }
    }
  }
  collect {
    for index in range(rows.len()) { yield rows[rows.len() - index - 1] }
  }
}

pure canonical_column(name: Str) -> Str? {
  let upper = name.upper()
  for column in COLUMNS { return column when column == upper }
  null
}

pure columns(output: Str?, output_all: Bool, environment: Str?) -> Result[List[Str], ColumnSelectionError] {
  var selected = if output_all { COLUMNS } else { DEFAULT_COLUMNS }
  if output == null and environment == null { return Ok(selected) }
  let value = if output != null { output ?? "" } else { environment ?? "" }
  let extend = value.starts_with("+")
  let names = (if extend { value.byte_slice(1) } else { value }).split(",")
  if names.is_empty() or (names.len() == 1 and names[0] == "") { return Err(UnknownColumn) }
  if ! extend { selected = [] }
  for name in names {
    let column = canonical_column(name)
    return Err(UnknownColumn) when column == null
    return Err(TooManyColumns) when selected.len() == 25
    selected += [column ?? ""]
  }
  Ok(selected)
}

pure column_selection_error(error: ColumnSelectionError) -> Str {
  if error == TooManyColumns { "too many columns specified, the limit is 25 columns" } else { "unknown column" }
}

pure column_value(lock: Lock, column: Str) -> FilterValue {
  match column {
    "PID" => {kind: "number", value: f"{lock.pid}", boolean: false}
    "TYPE" => {kind: "string", value: lock.kind, boolean: false}
    "SIZE" => {kind: "number", value: f"{lock.size}", boolean: false}
    "INODE" => {kind: "number", value: f"{lock.inode}", boolean: false}
    "MAJ:MIN" => {kind: "string", value: f"{lock.major}:{lock.minor}", boolean: false}
    "MODE" => {kind: "string", value: lock.mode + (if lock.blocked { "*" } else { "" }), boolean: false}
    "M" => {kind: "boolean", value: "", boolean: lock.mandatory}
    "START" => {kind: "number", value: f"{lock.start}", boolean: false}
    "END" => {kind: "number", value: f"{lock.end}", boolean: false}
    "PATH" => {kind: "string", value: if let path_bytes = lock.path { path_bytes.utf8() ?? "" } else { "" }, boolean: false}
    "BLOCKER" => {kind: "number", value: f"{lock.blocker ?? 0}", boolean: false}
    "HOLDERS" => {kind: "string", value: lock.holders.join("\n"), boolean: false}
    else => {kind: "string", value: lock.command, boolean: false}
  }
}

pure filter_tokens(text: Str) -> List[FilterToken]? {
  var output: List[FilterToken] = []
  var at = 0
  let length = text.byte_len()
  while at < length {
    let current = text.byte_slice(at, length: 1)
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
      at += 1
      var value = ""
      var closed = false
      while at < length {
        let part = text.byte_slice(at, length: 1)
        if part == quote {
          closed = true
          at += 1
          break
        }
        if part == "\\" and at + 1 < length {
          at += 1
          value += text.byte_slice(at, length: 1)
          at += 1
        } else {
          value += part
          at += 1
        }
      }
      return null when ! closed
      output += [{kind: "literal", value: value}]
      continue
    }
    let pair = if at + 1 < length { text.byte_slice(at, length: 2) } else { "" }
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
    let start = at
    while at < length {
      let part = text.byte_slice(at, length: 1)
      break when part in [" ", "\t", "\n", "\r", "(", ")", "<", ">", "!", "=", "&", "|", "'", "\""]
      at += 1
    }
    return null when start == at
    let word = text.byte_slice(start, length: at - start)
    let upper = word.upper()
    if upper in ["AND", "OR", "NOT"] {
      output += [{kind: "operator", value: upper}]
    } else if upper in ["EQ", "NE", "LT", "LE", "GT", "GE"] {
      output += [{kind: "operator", value: match upper { "EQ" => "=="; "NE" => "!="; "LT" => "<"; "LE" => "<="; "GT" => ">"; else => ">=" }}]
    } else {
      output += [{kind: "word", value: word}]
    }
  }
  output
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

pure filter_program(tokens: List[FilterToken]) -> List[FilterToken]? {
  var output: List[FilterToken] = []
  var operators: List[FilterToken] = []
  var expects_value = true
  for token in tokens {
    if token.kind in ["word", "literal"] {
      return null when ! expects_value
      output += [token]
      expects_value = false
    } else if token.kind == "(" {
      return null when ! expects_value
      operators += [token]
      expects_value = true
    } else if token.kind == ")" {
      return null when expects_value
      var found = false
      while ! operators.is_empty() {
        let top = operators[-1]
        operators = operators[..operators.len() - 1]
        if top.kind == "(" { found = true; break }
        output += [top]
      }
      return null when ! found
      expects_value = false
    } else {
      let operator = if token.value == "!" and expects_value { "NOT" } else { token.value }
      return null when operator in ["AND", "&&", "OR", "||", "==", "!=", ">", "<", ">=", "<=", "=~", "!~"] and expects_value
      return null when operator in ["NOT", "!"] and ! expects_value
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
  return null when expects_value or output.is_empty()
  while ! operators.is_empty() {
    let top = operators[-1]
    operators = operators[..operators.len() - 1]
    return null when top.kind == "("
    output += [top]
  }
  var depth = 0
  for token in output {
    if token.kind == "word" {
      let upper = token.value.upper()
      if canonical_column(token.value) == null and upper not in ["TRUE", "FALSE"] and ! filter_number(token.value) {
        return null
      }
    }
    if token.kind in ["word", "literal"] {
      depth += 1
    } else if token.value in ["NOT", "!"] {
      return null when depth < 1
    } else {
      return null when depth < 2
      depth -= 1
    }
  }
  return null when depth != 1
  output
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

pure compare_filter(left: FilterValue, right: FilterValue, operator: Str, lock: Lock) -> Bool {
  let l = if left.kind == "column" { column_value(lock, left.value) } else { left }
  let r = if right.kind == "column" { column_value(lock, right.value) } else { right }
  if operator in ["=~", "!~"] {
    let pattern = regex.compile(r.value)
    let matched = if let Ok(compiled) = pattern { compiled.matches(l.value) } else { false }
    return if operator == "=~" { matched } else { ! matched }
  }
  if l.kind in ["number", "float"] or r.kind in ["number", "float"] {
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
  return l.value == r.value when operator == "=="
  return l.value != r.value when operator == "!="
  return l.value > r.value when operator == ">"
  return l.value < r.value when operator == "<"
  return l.value >= r.value when operator == ">="
  return l.value <= r.value when operator == "<="
  false
}

pure filter_truth(value: FilterValue, lock: Lock) -> Bool {
  let resolved = if value.kind == "column" { column_value(lock, value.value) } else { value }
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

pure hex_byte(value: Int) -> Str {
  let digits = "0123456789abcdef"
  "\\x" + digits.byte_slice(value / 16, length: 1) + digits.byte_slice(value % 16, length: 1)
}

pure codepoint_value(value: Str) -> Int {
  let raw = bytes.from_text(value)
  let first = raw.byte_at(0) ?? 0
  return first when first < 128
  let second = (raw.byte_at(1) ?? 128) - 128
  return (first - 192) * 64 + second when raw.len() == 2
  let third = (raw.byte_at(2) ?? 128) - 128
  return (first - 224) * 4096 + second * 64 + third when raw.len() == 3
  let third_byte = (raw.byte_at(2) ?? 128) - 128
  let fourth_byte = (raw.byte_at(3) ?? 128) - 128
  (first - 240) * 262144 + second * 4096 + third_byte * 64 + fourth_byte
}

pure utf8_size(first: Int) -> Int {
  return 1 when first < 128
  return 2 when first >= 194 and first < 224
  return 3 when first >= 224 and first < 240
  return 4 when first >= 240 and first < 245
  0
}

pure safe_table_text(raw: Bytes, column: Str, byte_locale: Bool) -> Str {
  if byte_locale {
    var output = ""
    var at = 0
    while at < raw.len() {
      let byte = raw.byte_at(at) ?? 0
      if column == "HOLDERS" and byte == 10 {
        output += "\n"
        at += 1
      } else if byte == 92 and at + 1 < raw.len() and raw.byte_at(at + 1) == 120 {
        output += hex_byte(byte)
        at += 1
      } else if byte >= 32 and byte < 127 {
        output += raw[at..at + 1].utf8() ?? ""
        at += 1
      } else {
        output += hex_byte(byte)
        at += 1
      }
    }
    return output
  }
  var output = ""
  var at = 0
  while at < raw.len() {
    let byte = raw.byte_at(at) ?? 0
    if byte == 92 and at + 1 < raw.len() and raw.byte_at(at + 1) == 120 {
      output += hex_byte(byte)
      at += 1
      continue
    }
    let size = utf8_size(byte)
    if size > 0 and at + size <= raw.len() {
      if let Ok(character) = raw[at..at + size].utf8() {
        let codepoint = codepoint_value(character)
        if column == "HOLDERS" and codepoint == 10 {
          output += "\n"
        } else if codepoint <= 31 or codepoint in [127, 8232, 8233] or (codepoint >= 128 and codepoint < 160) or
            (codepoint >= 8203 and codepoint < 8208) or (codepoint >= 8234 and codepoint < 8239) or
            (codepoint >= 8287 and codepoint < 8304) or codepoint == 65279 {
          for byte_index in range(size) { output += hex_byte(raw.byte_at(at + byte_index) ?? 0) }
        } else {
          output += character
        }
        at += size
        continue
      }
    }
    output += hex_byte(byte)
    at += 1
  }
  output
}

pure raw_table_text(raw: Bytes, byte_locale: Bool) -> Bytes {
  var output: List[Bytes] = []
  for index in range(raw.len()) {
    let byte = raw.byte_at(index) ?? 0
    if byte == 32 or byte == 92 or byte < 32 or (byte >= 127 and byte_locale) {
      output += [bytes.from_text(hex_byte(byte))]
    } else {
      output += [raw[index..index + 1]]
    }
  }
  bytes.concat(output)
}

pure cell_display_width(value: Str, column: Str) -> Int {
  var width = 0
  for line in if column == "HOLDERS" { value.split("\n") } else { [value] } {
    let measured = tui.cell_width(line)
    if measured > width { width = measured }
  }
  width
}

pure table_width(columns: List[Str], rows: List[List[Str]], headings: Bool) -> List[Int] {
  collect {
    for index in range(columns.len()) {
      var width = if headings { tui.cell_width(columns[index]) } else { 0 }
      for row in rows {
        let measured = cell_display_width(row[index], columns[index])
        if measured > width { width = measured }
      }
      let minimum = match columns[index] {
        "COMMAND" => 15
        "PID" => 3
        "TYPE" | "INODE" => 5
        "SIZE" => 4
        "MAJ:MIN" => 6
        "MODE" => 5
        "M" => 1
        "START" | "END" => 10
        else => 0
      }
      yield if minimum > width { minimum } else { width }
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
      widths[path_column] = if excess >= widths[path_column] { 0 } else { widths[path_column] - excess }
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
        let padded = if raw or (index == row.len() - 1 and columns[index] not in ["PID", "TYPE", "SIZE", "INODE", "START", "END", "BLOCKER"]) {
          shown
        } else if columns[index] in ["PID", "TYPE", "SIZE", "INODE", "START", "END", "BLOCKER"] {
          tui.cell_left_pad(shown, widths[index])
        } else {
          tui.cell_right_pad(shown, widths[index])
        }
        rendered_cells += [padded]
        raw_cells += [if heading { bytes.from_text(row[index]) } else { raw_table_text(byte_rows[row_index][index], byte_locale) }]
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
  if mode == "compact" { return smart.render(document, compact: true) + "\n" }
  smartcols_json(document, 0) + "\n"
}

pure smartcols_json_indent(depth: Int) -> Str {
  var output = ""
  repeat depth times { output += "   " }
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
      let rows = collect {
        for item in items { yield inner + smartcols_json(item, depth + 1) }
      }
      "[\n" + rows.join(",\n") + "\n" + smartcols_json_indent(depth) + "]"
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
    var path_override: Str? = null
    if let path_bytes = lock.path {
      if path_bytes.utf8() is Err(_) {
        let marker = json_path_marker(index)
        path_override = marker
        replacements += [{marker: bytes.from_text(json.encode(marker) ?? "\"\""), value: json_string_bytes(path_bytes)}]
      }
    }
    rows += [json_lock(lock, columns, as_bytes, byte_locale, path_override)]
  }
  replace_json_paths(bytes.from_text(json_table(rows, "locks", mode)), replacements)
}

## Runs the lslocks command over Linux procfs state.
export proc main(argv: List[Str]) [fs, process, env, error, io] {
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
  if opts.help {
    gnu.help("\nUsage:\n lslocks [options]\n\nList local system locks.\n\nOptions:\n -b, --bytes            print SIZE in bytes rather than in human readable format\n -J, --json             use JSON output format\n -i, --noinaccessible   ignore locks without read permissions\n -n, --noheadings       don't print headings\n -o, --output <list>    output columns (see --list-columns)\n     --output-all       output all columns\n -p, --pid <pid>        display only locks held by this process\n -Q, --filter <expr>    apply display filter\n -r, --raw              use the raw output format\n -u, --notruncate       don't truncate text in columns\n\n -H, --list-columns     list the available columns\n -h, --help             display this help\n -V, --version          display version\n\nFor more details see lslocks(8).\n")
    return
  }
  if opts.version { gnu.version("lslocks"); return }
  if opts.json and opts.raw { gnu.usage_error("options --json and --raw are mutually exclusive") }
  if opts.pid != null {
    let value = decimal(opts.pid ?? "")
    if value == null or (value ?? 0) <= 0 { gnu.usage_error(f"invalid PID argument: {gnu.quote(opts.pid ?? "")}") }
  }
  let output_environment: Str? = if let Ok(value) = env.get("LSLOCKS_COLUMNS") { value } else { null }
  let json_mode = if let Ok(value) = env.get("LIBSMARTCOLS_JSON") { value } else { "" }
  if opts.list_columns {
    let names = COLUMNS
    let json_types = ["<string>", "<integer>", "<string>", "<string|number>", "<integer>", "<string>", "<string>", "<boolean>", "<integer>", "<integer>", "<string>", "<integer>", "<string>"]
    let rows = collect {
      for index in range(names.len()) { yield [names[index], json_types[index], COLUMN_DESCRIPTIONS[index]] }
    }
    if opts.json {
      let encoded = [smart.object([smart.m("holder", row[0]), smart.m("type", row[1]), smart.m("description", row[2])]) for row in rows]
      gnu.write_text(json_table(encoded, "lslocks-columns", json_mode))
    } else {
      var holder_width = 0
      var type_width = 0
      for row in rows {
        if row[0].count_chars() > holder_width { holder_width = row[0].count_chars() }
        if row[1].count_chars() > type_width { type_width = row[1].count_chars() }
      }
      let rendered_rows = collect {
        for row in rows {
          yield (if opts.raw { row[0] } else { pad_left(row[0], holder_width) }) + " " +
            (if opts.raw { row[1] } else { pad_right(row[1], type_width) }) + " " + row[2]
        }
      }
      gnu.write_text(rendered_rows.join("\n") + "\n")
    }
    return
  }
  let selection = columns(opts.output, opts.output_all, output_environment)
  if let Err(error) = selection {
    gnu.usage_error(column_selection_error(error))
    return
  }
  let chosen = selection ?? []
  let program: List[FilterToken]? = if opts.filter == null {
    null
  } else if let tokens = filter_tokens(opts.filter ?? "") {
    filter_program(tokens)
  } else {
    null
  }
  if opts.filter != null and program == null { gnu.usage_error(f"failed to parse {gnu.quote(opts.filter ?? "")}: invalid expression") }
  let all = collect(opts.no_inaccessible)
  return when all.is_empty()
  var rows = all
  if opts.pid != null {
    let selected_pid = decimal(opts.pid ?? "") ?? 0
    rows = rows |> where .pid == selected_pid |> collect
  }
  if program != null {
    rows = rows |> where filter_matches(program ?? [], .) |> collect
  }
  let byte_locale = text.byte_columns()
  if opts.json {
    gnu.write_bytes(json_output(rows, chosen, opts.bytes, json_mode, byte_locale))
  } else {
    let terminal = unix.isatty(1)
    let terminal_width = if terminal {
      if let Ok(size) = unix.window_size(1) { if size.cols > 0 { size.cols } else { 80 } } else { 80 }
    } else { 80 }
    gnu.write_bytes(render_table(chosen, rows, ! opts.noheadings, opts.raw, opts.bytes, opts.notruncate, terminal, terminal_width, byte_locale))
  }
}
