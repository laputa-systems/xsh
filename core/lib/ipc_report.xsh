export error OptionError = Invalid(detail: Str) | EmptyColumn | TooManyColumns

export type Options = {
  shmems: Bool,
  posix_shmems: Bool,
  queues: Bool,
  posix_mqueues: Bool,
  semaphores: Bool,
  posix_semaphores: Bool,
  global: Bool,
  id: Str?,
  name: Str?,
  output: Str?,
  noheadings: Bool,
  notruncate: Bool,
  time_format: Str?,
  bytes: Bool,
  creator: Bool,
  export: Bool,
  json: Bool,
  newline: Bool,
  list: Bool,
  numeric_perms: Bool,
  raw: Bool,
  time: Bool,
  shell: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

export type Selection = {
  resources: List[Str],
  explicit_resources: Bool,
  global: Bool,
  id: Int?,
  name: Str?,
  output_mode: Str,
  output_mode_explicit: Bool,
  columns: List[Str],
  explicit_columns: Bool,
  noheadings: Bool,
  notruncate: Bool,
  shell: Bool,
  bytes: Bool,
  numeric_perms: Bool,
  creator: Bool,
  time: Bool,
  time_format: Str,
}

export type Cell = {text: Str, present: Bool}
export type Element = {number: Int, value: Int, waiting_for_increase: Int, waiting_for_zero: Int, pid: Int, command: Str}
export type Row = {cells: List[Cell], elements: List[Element]}

const RESOURCE_OPTIONS = [
  {option: "queues", name: "queues", columns: ["KEY", "ID", "PERMS", "OWNER", "USEDBYTES", "MSGS", "LSPID", "LRPID"]},
  {option: "posix_mqueues", name: "posix_mqueues", columns: ["NAME", "PERMS", "OWNER", "MTIME", "USEDBYTES", "MSGS"]},
  {option: "shmems", name: "shmems", columns: ["KEY", "ID", "PERMS", "OWNER", "SIZE", "NATTCH", "STATUS", "CTIME", "CPID", "LPID", "COMMAND"]},
  {option: "posix_shmems", name: "posix_shmems", columns: ["NAME", "PERMS", "OWNER", "SIZE", "MTIME"]},
  {option: "semaphores", name: "semaphores", columns: ["KEY", "ID", "PERMS", "OWNER", "NSEMS"]},
  {option: "posix_semaphores", name: "posix_semaphores", columns: ["NAME", "PERMS", "OWNER", "MTIME", "SVAL"]},
]

const GLOBAL_COLUMNS = ["RESOURCE", "DESCRIPTION", "LIMIT", "USED", "USE%"]
const ALL_COLUMNS = [
  "KEY", "ID", "OWNER", "PERMS", "CUID", "CUSER", "CGID", "CGROUP", "UID", "USER", "GID", "GROUP", "CTIME",
  "NAME", "MTIME", "USEDBYTES", "MSGS", "SEND", "RECV", "LSPID", "LRPID", "SIZE", "NATTCH", "STATUS", "ATTACH", "DETACH", "COMMAND", "CPID", "LPID", "NSEMS", "OTIME",
  "RESOURCE", "DESCRIPTION", "LIMIT", "USED", "USE%", "SVAL",
]
const GENERIC_COLUMNS = ["KEY", "ID", "OWNER", "PERMS", "CUID", "CUSER", "CGID", "CGROUP", "UID", "USER", "GID", "GROUP", "CTIME"]
const HEX = "0123456789abcdef"
const RIGHT_COLUMNS = ["ID", "OWNER", "PERMS", "CUID", "CGID", "UID", "GID", "CTIME", "MTIME", "USEDBYTES", "SEND", "RECV", "LSPID", "LRPID", "SIZE", "NATTCH", "ATTACH", "DETACH", "CPID", "LPID", "NSEMS", "OTIME", "LIMIT", "USED", "USE%", "SVAL"]
const PRETTY_COLUMNS = [
  {column: "KEY", label: "Key"}, {column: "ID", label: "ID"}, {column: "OWNER", label: "Owner"},
  {column: "PERMS", label: "Permissions"}, {column: "CUID", label: "Creator UID"}, {column: "CUSER", label: "Creator user"},
  {column: "CGID", label: "Creator GID"}, {column: "CGROUP", label: "Creator group"}, {column: "UID", label: "UID"},
  {column: "USER", label: "User name"}, {column: "GID", label: "GID"}, {column: "GROUP", label: "Group name"},
  {column: "CTIME", label: "Last change"}, {column: "NAME", label: "Name"}, {column: "MTIME", label: "Last action"},
  {column: "USEDBYTES", label: "Bytes used"}, {column: "MSGS", label: "Messages"}, {column: "SEND", label: "Msg sent"},
  {column: "RECV", label: "Msg received"}, {column: "LSPID", label: "Msg sender"}, {column: "LRPID", label: "Msg receiver"},
  {column: "SIZE", label: "Segment size"}, {column: "NATTCH", label: "Attached processes"}, {column: "STATUS", label: "Status"},
  {column: "ATTACH", label: "Attach time"}, {column: "DETACH", label: "Detach time"}, {column: "COMMAND", label: "Creator command"},
  {column: "CPID", label: "Creator PID"}, {column: "LPID", label: "Last user PID"}, {column: "NSEMS", label: "Semaphores"},
  {column: "OTIME", label: "Last operation"}, {column: "RESOURCE", label: "Resource"}, {column: "DESCRIPTION", label: "Description"},
  {column: "LIMIT", label: "Limit"}, {column: "USED", label: "Used"}, {column: "USE%", label: "Use"}, {column: "SVAL", label: "Value"},
]
const LONG_OPTIONS = [
  "bytes", "creator", "export", "global", "help", "id", "json", "list", "name", "newline", "noheadings", "notruncate", "numeric-perms", "output", "posix-mqueues", "posix-semaphores", "posix-shmems", "queues", "raw", "semaphores", "shmems", "time", "time-format", "version", "shell",
]

pure hex_byte(byte: Int) -> Str {
  f"\\x{HEX.byte_slice(byte / 16, length: 1)}{HEX.byte_slice(byte % 16, length: 1)}"
}

pure escape_cell(text: Str) -> Str {
  let data = bytes.from_text(text)
  var out = ""
  var start = 0
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte < 32 or byte == 127 {
      out += text.byte_slice(start, length: index - start)
      out += hex_byte(byte)
      start = index + 1
    }
  }
  out + text.byte_slice(start)
}

pure escape_json(text: Str) -> Str {
  let data = bytes.from_text(text)
  var out = ""
  var start = 0
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte < 32 or byte == 34 or byte == 92 {
      out += text.byte_slice(start, length: index - start)
      out += if byte == 34 { "\\\"" } else if byte == 92 { "\\\\" } else if byte == 8 { "\\b" } else if byte == 12 { "\\f" } else if byte == 10 { "\\n" } else if byte == 13 { "\\r" } else if byte == 9 { "\\t" } else { f"\\u00{(hex_byte(byte)).byte_slice(2)}" }
      start = index + 1
    }
  }
  out + text.byte_slice(start)
}

pure escape_export(text: Str) -> Str {
  let data = bytes.from_text(text)
  var out = ""
  var start = 0
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte < 32 or byte == 34 or byte == 92 or byte == 127 {
      out += text.byte_slice(start, length: index - start)
      out += hex_byte(byte)
      start = index + 1
    }
  }
  out + text.byte_slice(start)
}

pure shell_column(column: Str) -> Str {
  let data = bytes.from_text(column)
  var out = ""
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    let accepted = (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 48 and byte <= 57) or byte == 95
    out += if accepted { column.byte_slice(index, length: 1) } else { "_" }
  }
  let first = out.byte_at(0) ?? 0
  if out.is_empty() or (first >= 48 and first <= 57) { out = "_" + out }
  out
}

pure heading(selection: Selection, column: Str) -> Str {
  if selection.shell { return shell_column(column) }
  column
}

pure pretty_label(column: Str) -> Str {
  for entry in PRETTY_COLUMNS {
    if entry.column == column { return entry.label }
  }
  column
}

pure truncate_cells(text: Str, width: Int) -> Str {
  var out = ""
  var used = 0
  for character in text {
    let cells = tui.cell_width(character)
    if used + cells > width { break }
    out += character
    used += cells
  }
  out
}

pure render_table(selection: Selection, rows: List[Row], output_width: Int) -> Str {
  var widths: List[Int] = [tui.cell_width(heading(selection, column)) for column in selection.columns]
  for row in rows {
    for index in range(selection.columns.len()) {
      let shown = tui.cell_width(escape_cell(row.cells[index].text))
      if shown > widths[index] { widths[index] = shown }
    }
  }
  if !selection.notruncate and output_width > 0 {
    var total_width = 0
    for index in range(widths.len()) {
      total_width += widths[index]
      if index + 1 < widths.len() { total_width += 1 }
    }
    if total_width > output_width {
      let reduction = total_width - output_width
      for index in range(selection.columns.len()) {
        if selection.columns[index] == "COMMAND" {
          let minimum = tui.cell_width(heading(selection, selection.columns[index]))
          widths[index] = if widths[index] - minimum > reduction { widths[index] - reduction } else { minimum }
          break
        }
      }
    }
  }
  var out = ""
  if !selection.noheadings and selection.output_mode != "pretty" {
    let labels: List[Str] = collect {
      for index in range(selection.columns.len()) {
        let label = heading(selection, selection.columns[index])
        if selection.columns[index] in RIGHT_COLUMNS {
          yield tui.left_pad(label, widths[index])
        } else if index == selection.columns.len() - 1 {
          yield label
        } else {
          yield tui.right_pad(label, widths[index])
        }
      }
    }
    out += labels.join(" ") + "\n"
  }
  for row in rows {
    if selection.output_mode == "pretty" {
      for index in range(selection.columns.len()) {
        let cell = row.cells[index]
        if cell.present {
          let label = pretty_label(selection.columns[index])
          out += f"{label}:{tui.right_pad("", 35 - label.count_chars())}{tui.right_pad(cell.text, 36)}\n"
        }
      }
      if !row.elements.is_empty() {
        out += "Elements:\n\n"
        let names = ["SEMNUM", "VALUE", "NCOUNT", "ZCOUNT", "PID", "COMMAND"]
        var element_widths: List[Int] = [name.count_chars() for name in names]
        for element in row.elements {
          let values = [f"{element.number}", f"{element.value}", f"{element.waiting_for_increase}", f"{element.waiting_for_zero}", f"{element.pid}", element.command]
          for index in range(names.len()) {
            if values[index].count_chars() > element_widths[index] { element_widths[index] = values[index].count_chars() }
          }
        }
        out += [tui.left_pad(names[index], element_widths[index]) for index in range(names.len())].join(" ") + "\n"
        for element in row.elements {
          let values = [f"{element.number}", f"{element.value}", f"{element.waiting_for_increase}", f"{element.waiting_for_zero}", f"{element.pid}", element.command]
          out += [tui.left_pad(values[index], element_widths[index]) for index in range(names.len())].join(" ") + "\n"
        }
      }
      continue
    }
    let values: List[Str] = collect {
      for index in range(selection.columns.len()) {
        let escaped = escape_cell(row.cells[index].text)
        let shown = if !selection.notruncate and selection.columns[index] == "COMMAND" and tui.cell_width(escaped) > widths[index] { truncate_cells(escaped, widths[index]) } else { escaped }
        if selection.columns[index] in RIGHT_COLUMNS {
          yield tui.left_pad(shown, widths[index])
        } else if index == selection.columns.len() - 1 {
          yield shown
        } else {
          yield tui.right_pad(shown, widths[index])
        }
      }
    }
    out += values.join(" ") + "\n"
  }
  out
}

pure render_raw(selection: Selection, rows: List[Row]) -> Str {
  var out = ""
  if !selection.noheadings {
    out += [heading(selection, column) for column in selection.columns].join(" ") + "\n"
  }
  for row in rows {
    out += [cell.text for cell in row.cells].join(" ") + "\n"
  }
  out
}

pure render_export(selection: Selection, rows: List[Row]) -> Str {
  var out = ""
  let separator = if selection.output_mode == "newline" { "\n" } else { " " }
  for row in rows {
    let entries: List[Str] = collect {
      for index in range(selection.columns.len()) {
        let column = heading(selection, selection.columns[index])
        let value = escape_export(row.cells[index].text)
        yield f"{column}=\"{value}\""
      }
    }
    out += entries.join(separator) + "\n"
  }
  out
}

pure render_json(selection: Selection, rows: List[Row]) -> Str {
  let table = if selection.global { "ipclimits" } else if selection.resources == ["queues"] { "messages" } else if selection.resources == ["posix_mqueues"] { "posix-messages" } else if selection.resources == ["shmems"] { "sharedmemory" } else if selection.resources == ["posix_shmems"] { "posix-sharedmemory" } else if selection.resources == ["semaphores"] { "semaphores" } else if selection.resources == ["posix_semaphores"] { "posix-semaphores" } else { "table" }
  var out = f"{{\n   \"{table}\": [\n"
  for row_index in range(rows.len()) {
    let row = rows[row_index]
    out += "      {\n"
    for index in range(selection.columns.len()) {
      let cell = row.cells[index]
      let value = if !cell.present { "null" } else { f"\"{escape_json(cell.text)}\"" }
      let comma = if index == selection.columns.len() - 1 { "" } else { "," }
      out += f"         \"{heading(selection, selection.columns[index]).lower()}\": {value}{comma}\n"
    }
    let suffix = if row_index == rows.len() - 1 { "      }\n" } else { "      },\n" }
    out += suffix
  }
  out + "   ]\n}\n"
}

export pure render(selection: Selection, rows: List[Row], output_width: Int) -> Str {
  match selection.output_mode {
    "json" => render_json(selection, rows)
    "export" | "newline" => render_export(selection, rows)
    "raw" => render_raw(selection, rows)
    else => render_table(selection, rows, output_width)
  }
}

pure parse_id(value: Str?) -> Result[Int?, OptionError] {
  if let text = value {
    let data = bytes.from_text(text)
    var offset = 0
    while offset < data.len() {
      let byte = data.byte_at(offset) ?? 0
      if byte == 32 or byte == 9 or byte == 10 or byte == 11 or byte == 12 or byte == 13 {
        offset += 1
      } else {
        break
      }
    }
    var negative = false
    if offset < data.len() {
      let byte = data.byte_at(offset) ?? 0
      if byte == 43 or byte == 45 {
        negative = byte == 45
        offset += 1
      }
    }
    let limit = if negative { 2147483648 } else { 2147483647 }
    var number = 0
    var digits = 0
    var out_of_range = false
    while offset < data.len() {
      let byte = data.byte_at(offset) ?? 0
      if byte < 48 or byte > 57 { break }
      digits += 1
      let digit = byte - 48
      if number > (limit - digit) / 10 {
        out_of_range = true
      } else if !out_of_range {
        number = number * 10 + digit
      }
      offset += 1
    }
    if digits == 0 or offset != data.len() {
      return Err(Invalid(detail: f"failed to parse IPC identifier: '{text}'"))
    }
    if out_of_range {
      return Err(Invalid(detail: f"failed to parse IPC identifier: '{text}': Numerical result out of range"))
    }
    let parsed = if negative { -number } else { number }
    return Ok(null) when parsed == -1
    return Ok(parsed)
  }
  Ok(null)
}

pure option_name(argument: Str) -> Str? {
  return null when !argument.starts_with("--")
  let typed = (argument.byte_slice(2)).split("=")[0]
  return typed when typed in LONG_OPTIONS
  let matching = [name for name in LONG_OPTIONS if name.starts_with(typed)]
  if matching.len() == 1 { matching[0] } else { null }
}

pure option_code(name: Str) -> Str? {
  match name {
    "json" => "J"
    "export" => "e"
    "list" => "l"
    "newline" => "n"
    "raw" => "r"
    "name" => "N"
    "global" => "g"
    "id" => "i"
    "creator" => "c"
    "output" => "o"
    "time" => "t"
    "posix-shmems" => "M"
    "posix-mqueues" => "Q"
    "posix-semaphores" => "S"
    "shmems" => "m"
    "queues" => "q"
    "semaphores" => "s"
    else => null
  }
}

pure conflict_group(code: Str) -> Int {
  return 0 when code in ["J", "e", "l", "n", "r"]
  return 1 when code in ["N", "g", "i"]
  return 2 when code in ["c", "o", "t"]
  return 3 when code in ["M", "Q", "S", "m", "q", "s"]
  -1
}

pure option_display(code: Str) -> Str {
  match code {
    "J" => "--json"
    "e" => "--export"
    "l" => "--list"
    "n" => "--newline"
    "r" => "--raw"
    "N" => "--name"
    "g" => "--global"
    "i" => "--id"
    "c" => "--creator"
    "o" => "--output"
    "t" => "--time"
    "M" => "--posix-shmems"
    "Q" => "--posix-mqueues"
    "S" => "--posix-semaphores"
    "m" => "--shmems"
    "q" => "--queues"
    else => "--semaphores"
  }
}

pure id_columns(selection: Selection) -> List[Str] {
  if selection.output_mode == "pretty" and !selection.explicit_columns and !selection.creator and !selection.time {
    return ALL_COLUMNS
  }
  if selection.global and !selection.explicit_resources {
    return GLOBAL_COLUMNS
  }
  var columns: List[Str] = []
  for resource in selection.resources {
    for mode in RESOURCE_OPTIONS {
      if mode.name == resource {
        if selection.time and !selection.global and resource == "shmems" {
          columns = columns.extend(mode.columns[0..mode.columns.len() - 1])
          columns = columns.extend(["ATTACH", "DETACH", "COMMAND"])
        } else {
          columns = columns.extend(mode.columns)
        }
      }
    }
  }
  if selection.global { columns = columns.extend(GLOBAL_COLUMNS) }
  if selection.creator {
    columns = columns.extend(["CUID", "CGID"])
    if !selection.resources.contains("posix_mqueues") and !selection.resources.contains("posix_shmems") and !selection.resources.contains("posix_semaphores") {
      columns = columns.extend(["UID", "GID"])
    }
  }
  if selection.time {
    if selection.resources.contains("queues") { columns = columns.extend(["SEND", "RECV", "CTIME"]) }
    if selection.global and selection.resources.contains("shmems") { columns = columns.extend(["ATTACH", "DETACH"]) }
    if selection.resources.contains("semaphores") { columns = columns.extend(["OTIME", "CTIME"]) }
  }
  columns
}

pure allowed_column(column: Str, selection: Selection) -> Bool {
  return true when column in GENERIC_COLUMNS
  if selection.global { return column in GLOBAL_COLUMNS }
  return false when !selection.explicit_resources
  return true when selection.resources.contains("posix_mqueues") and column in ["NAME", "MTIME"]
  return true when selection.resources.contains("posix_shmems") and column in ["NAME", "MTIME"]
  return true when selection.resources.contains("posix_semaphores") and column in ["NAME", "MTIME"]
  return true when selection.resources.contains("queues") and column in ["USEDBYTES", "MSGS", "SEND", "RECV", "LSPID", "LRPID"]
  return true when selection.resources.contains("shmems") and column in ["SIZE", "NATTCH", "STATUS", "ATTACH", "DETACH", "COMMAND", "CPID", "LPID"]
  return true when selection.resources.contains("semaphores") and column in ["NSEMS", "OTIME"]
  false
}

pure ascii_upper(text: Str) -> Str? {
  let data = bytes.from_text(text)
  var out = ""
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte >= 128 { return null }
    out += if byte >= 97 and byte <= 122 { "ABCDEFGHIJKLMNOPQRSTUVWXYZ".byte_slice(byte - 97, length: 1) } else { text.byte_slice(index, length: 1) }
  }
  out
}

pure output_columns(raw: Str, selection: Selection) -> Result[List[Str], OptionError] {
  let append = raw.starts_with("+")
  let body = if append { raw.byte_slice(1) } else { raw }
  if body.is_empty() { return Err(OptionError.EmptyColumn) }
  let names = body.split(",")
  var columns: List[Str] = if append { id_columns(selection) } else { [] }
  for index in range(names.len()) {
    let source = names[index]
    let trailing_empty = source.is_empty() and index + 1 == names.len() and body.ends_with(",")
    if columns.len() >= ALL_COLUMNS.len() * 2 and !trailing_empty {
      return Err(OptionError.TooManyColumns)
    }
    if source.is_empty() {
      if trailing_empty { continue }
      return Err(OptionError.EmptyColumn)
    }
    let remaining = names[index..].join(",")
    var column: Str? = null
    if let normalized = ascii_upper(source) {
      for candidate in ALL_COLUMNS {
        if candidate == normalized { column = candidate; break }
      }
    }
    if let selected = column {
      if !allowed_column(selected, selection) {
        return Err(Invalid(detail: f"column {remaining} does not apply to the specified IPC"))
      }
      columns += [selected]
    } else {
      return Err(Invalid(detail: f"unknown column: {remaining}"))
    }
  }
  Ok(columns)
}

pure select(options: Options) -> Result[Selection, OptionError] {
  if let failure = options.time_format {
    if failure not in ["short", "full", "iso"] {
      return Err(Invalid(detail: f"unknown time format: {failure}"))
    }
  }
  let id = parse_id(options.id)?
  var resources: List[Str] = []
  for mode in RESOURCE_OPTIONS {
    let selected = match mode.option {
      "shmems" => options.shmems
      "posix_shmems" => options.posix_shmems
      "queues" => options.queues
      "posix_mqueues" => options.posix_mqueues
      "semaphores" => options.semaphores
      else => options.posix_semaphores
    }
    if selected { resources += [mode.name] }
  }
  let explicit_resources = !resources.is_empty()
  let global = options.global or !explicit_resources
  if !explicit_resources and (options.creator or options.time or id != null or options.name != null) {
    return Err(Invalid(detail: "--global is mutually exclusive with --creator, --id, --name and --time"))
  }
  if !explicit_resources {
    resources = ["queues", "posix_mqueues", "shmems", "posix_shmems", "semaphores", "posix_semaphores"]
  }
  var output_mode = "list"
  if options.json { output_mode = "json" }
  if options.export { output_mode = "export" }
  if options.list { output_mode = "list" }
  if options.newline { output_mode = "newline" }
  if options.raw { output_mode = "raw" }
  let output_mode_explicit = options.json or options.export or options.list or options.newline or options.raw
  let pretty = ((id ?? -1) != -1 or options.name != null) and !output_mode_explicit
  if pretty { output_mode = "pretty" }
  var time_format = if pretty { "full" } else { "short" }
  if let explicit_time_format = options.time_format { time_format = explicit_time_format }
  var selection: Selection = {
    resources: resources,
    explicit_resources: explicit_resources,
    global: global,
    id: id,
    name: options.name,
    output_mode: output_mode,
    output_mode_explicit: output_mode_explicit,
    columns: [],
    explicit_columns: options.output != null,
    noheadings: options.noheadings,
    notruncate: options.notruncate,
    shell: options.shell,
    bytes: options.bytes,
    numeric_perms: options.numeric_perms,
    creator: options.creator,
    time: options.time,
    time_format: time_format,
  }
  let columns = if let output = options.output { output_columns(output, selection)? } else { id_columns(selection) }
  selection = {...selection, columns: columns}
  Ok(selection)
}

export pure validate_cli_options(argv: List[Str], permute: Bool) -> Result[Unit, OptionError] {
  var first: List[Str?] = [null, null, null, null]
  var index = 0
  while index < argv.len() {
    let argument = argv[index]
    if argument == "--" { break }
    if let name = option_name(argument) {
      if name == "help" or name == "version" { break }
      if let code = option_code(name) {
        let group = conflict_group(code)
        if group >= 0 {
          if let previous = first[group] {
            if previous != code {
              return Err(Invalid(detail: f"options {option_display(previous)} and {option_display(code)} cannot be combined"))
            }
          }
          first[group] = code
        }
      }
      let equals = argument.find("=")
      if name == "id" {
        let value = if let offset = equals { argument.byte_slice(offset + 1) } else { index += 1; argv[index] }
        let _ = parse_id(value)?
      } else if name == "time-format" {
        let value = if let offset = equals { argument.byte_slice(offset + 1) } else { index += 1; argv[index] }
        if value not in ["short", "full", "iso"] {
          return Err(Invalid(detail: f"unknown time format: {value}"))
        }
      } else if name in ["name", "output"] and equals == null {
        index += 1
      }
      index += 1
      continue
    }
    if argument.starts_with("-") and argument != "-" {
      let shorts = argument.byte_slice(1)
      var offset = 0
      var stop_options = false
      while offset < shorts.byte_len() {
        let code = shorts.byte_slice(offset, length: 1)
        if code in ["h", "V"] {
          stop_options = true
          break
        }
        let name = match code {
          "b" => "bytes"
          "c" => "creator"
          "e" => "export"
          "g" => "global"
          "i" => "id"
          "J" => "json"
          "l" => "list"
          "m" => "shmems"
          "M" => "posix-shmems"
          "n" => "newline"
          "N" => "name"
          "o" => "output"
          "P" => "numeric-perms"
          "Q" => "posix-mqueues"
          "q" => "queues"
          "r" => "raw"
          "S" => "posix-semaphores"
          "s" => "semaphores"
          "t" => "time"
          "y" => "shell"
          else => ""
        }
        if let mapped = option_code(name) {
          let group = conflict_group(mapped)
          if group >= 0 {
            if let previous = first[group] {
              if previous != mapped {
                return Err(Invalid(detail: f"options {option_display(previous)} and {option_display(mapped)} cannot be combined"))
              }
            }
            first[group] = mapped
          }
        }
        if code in ["i", "N", "o"] {
          let value = if offset + 1 < shorts.byte_len() { shorts.byte_slice(offset + 1) } else { index += 1; argv[index] }
          if code == "i" {
            let _ = parse_id(value)?
          }
          break
        }
        offset += 1
      }
      if stop_options { break }
      index += 1
      continue
    }
    if !permute { break }
    index += 1
  }
  Ok()
}

export pure resolve(options: Options, argv: List[Str], permute: Bool) -> Result[Selection, OptionError] {
  validate_cli_options(argv, permute)?
  select(options)
}
