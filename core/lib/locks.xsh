use disk_usage as usage
use gnu
use smart_json as smart
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

type DescriptorLock = {lock: KernelLock, descriptor: LinuxOpenFile, path: Str, size: Int}

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
  path: Str?,
  blocker: Int?,
  holders: List[Str],
  blocked: Bool,
}

type FilterToken = {kind: Str, value: Str}
type FilterValue = {kind: Str, value: Str, boolean: Bool}
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
  usage.amount(value, {block: 1, human: 1024, label: "Size"})
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
      let path = descriptor.path.display()
      let source = fp"/proc/{descriptor.pid}/fdinfo/{descriptor.fd}".read_text()
      if let Ok(text) = source {
        for line in text.lines() {
          continue when ! line.starts_with("lock:\t")
          if let lock = parse_kernel_lock(line.byte_slice(6)) {
            let stat = fs.stat(fp"/proc/{descriptor.pid}/fd/{descriptor.fd}", follow_symlinks: true)
            let size = if let Ok(metadata) = stat { metadata.size } else { 0 }
            yield {lock: lock, descriptor: descriptor, path: path, size: size}
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

proc mount_fallback(lock: KernelLock) [fs] -> Str? {
  let text = fp"/proc/self/mountinfo".read_text() ?? ""
  let device = f"{lock.major}:{lock.minor}"
  var target: Str? = null
  for line in text.lines() {
    let fields = src.parse_words(line)
    if fields.len() > 4 and fields[2] == device {
      target = mounts.decode_mount_field(fields[4]) + "..."
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
      let path = if resolved != null and ! lock.blocked { resolved.path } else if path_file != null { path_file.path.display() } else { null }
      let size = if resolved != null and ! lock.blocked {
        resolved.size
      } else if path_file != null {
        let metadata = fs.stat(fp"/proc/{pid}/fd/{path_file.fd}", follow_symlinks: true)
        if let Ok(value) = metadata { value.size } else { 0 }
      } else { 0 }
      let visible_path = if path != null { path } else if no_inaccessible { null } else { mount_fallback(lock) }
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

pure columns(output: Str?, output_all: Bool, environment: Str?) -> List[Str]? {
  var selected = if output_all { COLUMNS } else { DEFAULT_COLUMNS }
  let spec = output ?? environment
  if spec == null { return selected }
  let value = spec ?? ""
  let extend = value.starts_with("+")
  let names = (if extend { value.byte_slice(1) } else { value }).split(",")
  if names.is_empty() or (names.len() == 1 and names[0] == "") { return null }
  if ! extend { selected = [] }
  for name in names {
    let column = canonical_column(name)
    return null when column == null
    return null when (column ?? "") in selected
    selected += [column ?? ""]
  }
  selected
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
    "PATH" => {kind: "string", value: lock.path ?? "", boolean: false}
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
    output += [{kind: "operator", value: upper}] when upper in ["AND", "OR", "NOT"]
    output += [{kind: "operator", value: "NOT"}] when word == "!"
    output += [{kind: "word", value: word}] when upper not in ["AND", "OR", "NOT"]
  }
  output
}

pure operator_precedence(operator: Str) -> Int {
  return 1 when operator in ["OR", "||"]
  return 2 when operator in ["AND", "&&"]
  return 3 when operator in ["NOT", "!"]
  4
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
      while ! operators.is_empty() and operators[-1].kind == "operator" and
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
      if canonical_column(token.value) == null and upper not in ["TRUE", "FALSE"] and decimal(token.value) == null and usage.parse_size(token.value) == null {
        return null
      }
    }
    if token.kind in ["word", "literal"] { depth += 1 }
    else if token.value in ["NOT", "!"] { return null when depth < 1 }
    else { return null when depth < 2; depth -= 1 }
  }
  return null when depth != 1
  output
}

pure filter_literal(token: FilterToken) -> FilterValue {
  return {kind: "string", value: token.value, boolean: false} when token.kind == "literal"
  let upper = token.value.upper()
  return {kind: "boolean", value: "", boolean: upper == "TRUE"} when upper in ["TRUE", "FALSE"]
  if let column = canonical_column(token.value) { return {kind: "column", value: column, boolean: false} }
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
  if l.kind == "number" {
    let left_number = decimal(l.value) ?? 0
    let right_number = if r.kind == "number" { usage.parse_size(r.value) ?? (decimal(r.value) ?? 0) } else { usage.parse_size(r.value) ?? (decimal(r.value) ?? 0) }
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

pure filter_matches(program: List[FilterToken], lock: Lock) -> Bool {
  var stack: List[FilterValue] = []
  for token in program {
    if token.kind in ["word", "literal"] {
      stack += [filter_literal(token)]
    } else if token.value in ["NOT", "!"] {
      let value = stack[-1]
      stack = stack[..stack.len() - 1]
      let boolean = if value.kind == "column" { column_value(lock, value.value).boolean } else if value.kind == "boolean" { value.boolean } else { value.value != "" }
      stack += [{kind: "boolean", value: "", boolean: ! boolean}]
    } else if token.value in ["AND", "&&", "OR", "||"] {
      let right = stack[-1]
      let left = stack[-2]
      stack = stack[..stack.len() - 2]
      let left_bool = if left.kind == "column" { column_value(lock, left.value).boolean } else if left.kind == "boolean" { left.boolean } else { left.value != "" }
      let right_bool = if right.kind == "column" { column_value(lock, right.value).boolean } else if right.kind == "boolean" { right.boolean } else { right.value != "" }
      stack += [{kind: "boolean", value: "", boolean: if token.value in ["AND", "&&"] { left_bool and right_bool } else { left_bool or right_bool }}]
    } else {
      let right = stack[-1]
      let left = stack[-2]
      stack = stack[..stack.len() - 2]
      stack += [{kind: "boolean", value: "", boolean: compare_filter(left, right, token.value, lock)}]
    }
  }
  stack[0].boolean
}

pure lock_cell(lock: Lock, column: Str, bytes: Bool) -> Any {
  match column {
    "COMMAND" => lock.command
    "PID" => lock.pid
    "TYPE" => lock.kind
    "SIZE" => if lock.size == 0 { null } else if bytes { lock.size } else { human_size(lock.size) }
    "INODE" => lock.inode
    "MAJ:MIN" => f"{lock.major}:{lock.minor}"
    "MODE" => lock.mode + (if lock.blocked { "*" } else { "" })
    "M" => lock.mandatory
    "START" => lock.start
    "END" => lock.end
    "PATH" => lock.path
    "BLOCKER" => lock.blocker
    "HOLDERS" => lock.holders
    else => null
  }
}

pure table_value(lock: Lock, column: Str, bytes: Bool, raw: Bool) -> Str {
  match column {
    "COMMAND" => lock.command
    "PID" => f"{lock.pid}"
    "TYPE" => lock.kind
    "SIZE" => if lock.size == 0 { "" } else if bytes { f"{lock.size}" } else { human_size(lock.size) }
    "INODE" => f"{lock.inode}"
    "MAJ:MIN" => if raw { f"{lock.major}:{lock.minor}" } else { f"{lock.major:>3}:{lock.minor:<3}" }
    "MODE" => lock.mode + (if lock.blocked { "*" } else { "" })
    "M" => if lock.mandatory { "1" } else { "0" }
    "START" => f"{lock.start}"
    "END" => f"{lock.end}"
    "PATH" => lock.path ?? ""
    "BLOCKER" => f"{lock.blocker ?? ""}"
    "HOLDERS" => lock.holders.join("\n")
    else => ""
  }
}

pure table_width(columns: List[Str], rows: List[List[Str]], headings: Bool) -> List[Int] {
  collect {
    for index in range(columns.len()) {
      var width = if headings { columns[index].count_chars() } else { 0 }
      for row in rows { if row[index].count_chars() > width { width = row[index].count_chars() } }
      let minimum = match columns[index] {
        "COMMAND" => 15
        "PID" | "TYPE" | "INODE" => 5
        "SIZE" => 4
        "MAJ:MIN" | "MODE" => 5
        "M" => 1
        "START" | "END" => 10
        else => 0
      }
      yield if headings and minimum > width { minimum } else { width }
    }
  }
}

pure pad_left(value: Str, width: Int) -> Str {
  tui.left_pad(value, width)
}

pure pad_right(value: Str, width: Int) -> Str {
  tui.right_pad(value, width)
}

pure render_table(columns: List[Str], locks: List[Lock], headings: Bool, raw: Bool, bytes: Bool) -> Str {
  let data: List[List[Str]] = [[table_value(lock, column, bytes, raw) for column in columns] for lock in locks]
  let widths = table_width(columns, data, headings)
  let rows: List[List[Str]] = if headings { [columns, @data] } else { data }
  let lines = collect {
    for row in rows {
      let cells = collect {
        for index in range(row.len()) {
          let value = row[index]
          if raw {
            yield value
          } else if index == row.len() - 1 {
            yield if columns[index] in ["PID", "TYPE", "SIZE", "INODE", "START", "END", "BLOCKER"] { pad_left(value, widths[index]) } else { value }
          } else {
            yield if columns[index] in ["PID", "TYPE", "SIZE", "INODE", "START", "END", "BLOCKER"] { pad_left(value, widths[index]) } else { pad_right(value, widths[index]) }
          }
        }
      }
      yield cells.join(" ")
    }
  }
  lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })
}

pure json_lock(lock: Lock, columns: List[Str], bytes: Bool) -> smart.Object {
  smart.object([
    for column in columns {
      smart.m(column.lower(), lock_cell(lock, column, bytes))
    },
  ])
}

pure json_output(locks: List[Lock], columns: List[Str], bytes: Bool, mode: Str) -> Str {
  let rows = [json_lock(lock, columns, bytes) for lock in locks]
  if mode == "lines" {
    let lines = [smart.render(smart.object([smart.m("locks", [row])]), compact: true) for row in rows]
    return lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })
  }
  let document = smart.object([smart.m("locks", rows)])
  smart.render(document, compact: mode == "compact") + "\n"
}

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
    gnu.help("Usage:\n lslocks [options]\n\nList local system locks.\n\nOptions:\n -b, --bytes            print SIZE in bytes rather than in human readable format\n -J, --json             use JSON output format\n -i, --noinaccessible   ignore locks without read permissions\n -n, --noheadings       don't print headings\n -o, --output <list>    output columns (see --list-columns)\n     --output-all       output all columns\n -p, --pid <pid>        display only locks held by this process\n -Q, --filter <expr>    apply display filter\n -r, --raw              use the raw output format\n -u, --notruncate       don't truncate text in columns\n -H, --list-columns     list the available columns\n -h, --help             display this help\n -V, --version          display version\n")
    return
  }
  if opts.version { gnu.version("lslocks"); return }
  if opts.json and opts.raw { gnu.usage_error("options --json and --raw are mutually exclusive") }
  if opts.pid != null {
    let value = decimal(opts.pid ?? "")
    if value == null or (value ?? 0) <= 0 { gnu.usage_error(f"invalid PID argument: {gnu.quote(opts.pid ?? "")}") }
  }
  let output_environment = if let Ok(value) = env.get("LSLOCKS_COLUMNS") { value } else { null }
  if opts.list_columns {
    let names = COLUMNS
    let json_types = ["string", "number", "string", if opts.bytes { "number" } else { "<string|number>" }, "number", "string", "string", "boolean", "number", "number", "string", "number", "array(string)"]
    let rows = collect {
      for index in range(names.len()) { yield [names[index], json_types[index], COLUMN_DESCRIPTIONS[index]] }
    }
    let listing_columns = ["NAME", "JSON", "DESCRIPTION"]
    if opts.json {
      let encoded = [smart.object([smart.m("name", row[0]), smart.m("json", row[1]), smart.m("description", row[2])]) for row in rows]
      gnu.write_text(smart.render(smart.object([smart.m("columns", encoded)]), compact: false) + "\n")
    } else {
      let rendered_rows = collect {
        for row in if opts.noheadings { rows } else { [listing_columns, @rows] } { yield row.join(" ") }
      }
      gnu.write_text(rendered_rows.join("\n") + "\n")
    }
    return
  }
  let selected = columns(opts.output, opts.output_all, output_environment)
  if selected == null { gnu.usage_error("unknown column or duplicate column") }
  let chosen = selected ?? []
  let program: List[FilterToken]? = if opts.filter == null {
    null
  } else if let tokens = filter_tokens(opts.filter ?? "") {
    filter_program(tokens)
  } else {
    null
  }
  if opts.filter != null and program == null { gnu.usage_error(f"failed to parse {gnu.quote(opts.filter ?? "")}: invalid expression") }
  let all = collect(opts.no_inaccessible)?
  var rows = all
  if opts.pid != null {
    let selected_pid = decimal(opts.pid ?? "") ?? 0
    rows = rows |> where .pid == selected_pid |> collect
  }
  if program != null {
    rows = rows |> where filter_matches(program ?? [], .) |> collect
  }
  if opts.json {
    let mode = if let Ok(value) = env.get("LIBSMARTCOLS_JSON") { value } else { "" }
    gnu.write_text(json_output(rows, chosen, opts.bytes, mode))
  } else {
    gnu.write_text(render_table(chosen, rows, ! opts.noheadings, opts.raw, opts.bytes))
  }
}
