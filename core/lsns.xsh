#!/bin/xsh
use lib.gnu

const USAGE = """Usage:
 lsns [options]

List system namespaces.

Options:
 -J, --json             use JSON output format
 -n, --noheadings       don't print headings
 -o, --output <list>    define which output columns to use
 -p, --task <pid>       print process namespaces
 -r, --raw              use the raw output format
 -t, --type <name>      namespace type (mnt, net, ipc, user, pid, uts, cgroup, time)

 -h, --help             display this help
 -V, --version          display version

Output columns: NS, TYPE, PATH, NPROCS, PID, PPID, COMMAND, UID, USER,
NETNSID, NSFS, PNS, ONS.
"""

const COLUMNS = ["NS", "TYPE", "PATH", "NPROCS", "PID", "PPID", "COMMAND", "UID", "USER", "NETNSID", "NSFS", "PNS", "ONS"]
const NUMERIC = ["NS", "NPROCS", "PID", "PPID", "UID", "PNS", "ONS"]
# The columns the table formats align to the right.
const RIGHT = ["NS", "NPROCS", "PID", "PPID", "UID", "NETNSID", "PNS", "ONS"]
const TYPES = ["mnt", "net", "ipc", "user", "pid", "uts", "cgroup", "time"]
const HEX = "0123456789abcdef"

type Options = {
  type: Str?, task: Str?, output: Str?, noheadings: Bool, json: Bool, raw: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

type Namespace = {
  ns: Int, type: Str, path: Path, nprocs: Int, pid: Int, ppid: Int, uid: Int, command: Str,
  pns: Int, ons: Int, netnsid: Int?, nsfs: List[Path],
}

# One output cell: its text, whether the column is numeric, and the lines a
# multi-line cell spreads over in the table format.
type Cell = {text: Str, numeric: Bool, right: Bool, lines: List[Str]}

pure hex_escape(byte: Int) -> Str {
  f"\\x{HEX.byte_slice(byte / 16, length: 1)}{HEX.byte_slice(byte % 16, length: 1)}"
}

# Escapes the bytes the table format cannot show as they are: controls
# everywhere, and in the raw format also blanks, backslash, and non-ASCII.
pure escape(text: Str, raw: Bool) -> Str {
  let data = bytes.from_text(text)
  var out = ""
  var start = 0
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    let special = byte < 32 or byte == 127 or (raw and (byte == 32 or byte == 92 or byte >= 128))
    if special {
      out += text.byte_slice(start, length: index - start)
      out += hex_escape(byte)
      start = index + 1
    }
  }
  out + text.byte_slice(start)
}

pure json_escape(text: Str) -> Str {
  let data = bytes.from_text(text)
  var out = ""
  var start = 0
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte < 32 or byte == 34 or byte == 92 {
      out += text.byte_slice(start, length: index - start)
      out += if byte == 34 { "\\\"" } else if byte == 92 { "\\\\" } else if byte == 8 { "\\b" } else if byte == 12 { "\\f" } else if byte == 10 { "\\n" } else if byte == 13 { "\\r" } else if byte == 9 { "\\t" } else { f"\\u00{HEX.byte_slice(byte / 16, length: 1)}{HEX.byte_slice(byte % 16, length: 1)}" }
      start = index + 1
    }
  }
  out + text.byte_slice(start)
}

pure parse_pid(text: Str) -> Int? {
  return null when text.starts_with("+")
  if let Ok(number) = text.parse_int() { number } else { null }
}

proc owner(uid: Int) [fs, error] -> Str {
  if let Ok(account) = user.by_uid(uid) { account.name } else { f"{uid}" }
}

proc cell(column: Str, entry: Namespace) [fs, error] -> Cell {
  let number = column in NUMERIC
  let text = match column {
    "NS" => f"{entry.ns}"
    "TYPE" => entry.type
    "PATH" => entry.path.display()
    "NPROCS" => f"{entry.nprocs}"
    "PID" => f"{entry.pid}"
    "PPID" => f"{entry.ppid}"
    "COMMAND" => entry.command
    "UID" => f"{entry.uid}"
    "USER" => owner(entry.uid)
    "NETNSID" => if entry.type != "net" { "" } else if let id = entry.netnsid { f"{id}" } else { "unassigned" }
    "NSFS" => [mount.display() for mount in entry.nsfs].join(",")
    "PNS" => f"{entry.pns}"
    else => f"{entry.ons}"
  }
  let lines = if column == "NSFS" { [mount.display() for mount in entry.nsfs] } else { [text] }
  {text: text, numeric: number, right: column in RIGHT, lines: lines}
}

proc print_table(names: List[Str], rows: List[List[Cell]], headings: Bool) [process, env, io] {
  var widths: List[Int] = [name.count_chars() for name in names]
  for row in rows {
    for index in range(names.len()) {
      for line in row[index].lines {
        let shown = escape(line, false).count_chars()
        if shown > widths[index] { widths[index] = shown }
      }
    }
  }
  var out = ""
  if headings {
    let parts: List[Str] = collect {
      for index in range(names.len()) {
        let last = index == names.len() - 1
        if names[index] in RIGHT {
          yield tui.left_pad(names[index], widths[index])
        } else if last {
          yield names[index]
        } else {
          yield tui.right_pad(names[index], widths[index])
        }
      }
    }
    out += parts.join(" ") + "\n"
  }
  for row in rows {
    var depth = 1
    for item in row {
      if item.lines.len() > depth { depth = item.lines.len() }
    }
    for level in range(depth) {
      var started = level == 0
      let parts: List[Str] = collect {
        for index in range(names.len()) {
          let item = row[index]
          let last = index == names.len() - 1
          let shown = if level < item.lines.len() { escape(item.lines[level], false) } else { "" }
          if level > 0 and shown != "" { started = true }
          if level > 0 and started and shown == "" {
            yield ""
          } else if item.right {
            yield tui.left_pad(shown, widths[index])
          } else if last {
            yield shown
          } else {
            yield tui.right_pad(shown, widths[index])
          }
        }
      }
      out += parts.join(" ") + "\n"
    }
  }
  gnu.write_text(out)
}

proc print_raw(names: List[Str], rows: List[List[Cell]], headings: Bool) [process, env, io] {
  var out = ""
  if headings { out += names.join(" ") + "\n" }
  for row in rows {
    out += [escape(item.text, true) for item in row].join(" ") + "\n"
  }
  gnu.write_text(out)
}

proc print_json(names: List[Str], rows: List[List[Cell]]) [process, env, io] {
  var out = "{\n   \"namespaces\": [\n"
  if rows.is_empty() {
    out += "\n"
  }
  var first = true
  for row in rows {
    out += if first { "      {\n" } else { "},{\n" }
    first = false
    for index in range(names.len()) {
      let item = row[index]
      let key = names[index].lower()
      let value = if item.numeric {
        item.text
      } else if item.text == "" and (names[index] == "NSFS" or names[index] == "NETNSID") {
        "null"
      } else {
        f"\"{json_escape(if names[index] == "NSFS" and item.lines.len() > 1 { item.lines[0] } else { item.text })}\""
      }
      let last = index == names.len() - 1
      out += f"         \"{key}\": {value}{if last { "" } else { "," }}\n"
    }
    # A multi-line NSFS cell leaves a stray blank line in the reference JSON.
    var multiline = false
    for item in row {
      if item.lines.len() > 1 { multiline = true }
    }
    if multiline { out += " \n" }
    out += "      "
  }
  if !rows.is_empty() { out += "}\n" }
  out += "   ]\n}\n"
  gnu.write_text(out)
}

proc main(...argv: List[Str]) [fs, error, process, env, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    type: {form: "-t --type NAME"},
    task: {form: "-p --task PID"},
    output: {form: "-o --output LIST"},
    noheadings: {form: "-n --noheadings", default: false},
    json: {form: "-J --json", default: false},
    raw: {form: "-r --raw", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...NAMESPACE"},
  })?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("lsns"); return }
  if !opts.operands.is_empty() {
    gnu.error("namespace ID operands are not supported")
    exit 1
  }

  var wanted: Str? = null
  if let name = opts.type {
    if name not in TYPES {
      gnu.error(f"unknown namespace type: {name}")
      exit 1
    }
    wanted = name
  }
  var task: Int? = null
  if let text = opts.task {
    let parsed = parse_pid(text)
    if parsed == null {
      gnu.error(f"invalid PID argument: {gnu.quote_value(text)}")
      exit 1
    }
    if parsed <= 0 {
      gnu.error(f"invalid PID argument: {gnu.quote_value(text)}: Result not representable")
      exit 1
    }
    task = parsed
  }

  # The network namespace table shows its IDs and mount points by default.
  var names: List[Str] = ["NS", "TYPE", "NPROCS", "PID", "USER", "COMMAND"]
  if wanted == "net" { names = ["NS", "TYPE", "NPROCS", "PID", "USER", "NETNSID", "NSFS", "COMMAND"] }
  if let list = opts.output {
    var chosen: List[Str] = if list.starts_with("+") { names } else { [] }
    let body = if list.starts_with("+") { list.byte_slice(1) } else { list }
    for word in body.split(",") {
      let column = word.upper()
      if column not in COLUMNS {
        gnu.error(f"unknown column: {word}")
        exit 1
      }
      chosen += [column]
    }
    names = chosen
  }

  let found = linux.namespaces(task) ?? { |failure|
    gnu.error(f"failed to read namespaces: {gnu.strerror(failure)}")
    exit 1
  }
  let rows: List[List[Cell]] = collect {
    for entry in found {
      continue when wanted != null and entry.type != wanted
      yield [cell(column, entry) for column in names]
    }
  }

  if opts.json {
    print_json(names, rows)
  } else if rows.is_empty() {
    return
  } else if opts.raw {
    print_raw(names, rows, !opts.noheadings)
  } else {
    print_table(names, rows, !opts.noheadings)
  }
}
