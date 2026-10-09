#!/bin/xsh
use lib.gnu

const USAGE = """
Usage: paste [OPTION]... [FILE]...
Write lines consisting of the sequentially corresponding lines from each FILE,
separated by TABs, to standard output.
  -d, --delimiters=LIST  reuse characters from LIST instead of TABs
  -s, --serial           paste one file at a time instead of in parallel
  -z, --zero-terminated  line delimiter is NUL, not newline
      --help             display this help and exit
      --version          output version information and exit
"""

type PasteOptions = {serial: Bool, delimiters: List[Str], zero: Bool, help: Bool, version: Bool, files: List[Str]}
type PasteArgs = {files: List[Bytes], delimiter: Bytes, has_delimiter: Bool}

pure matching(data: Bytes, at: Int, marker: Int) -> Bool {
  at < data.len() and (data.byte_at(at) ?? -1) == marker
}

pure split_records(data: Bytes, separator: Int) -> List[Bytes] {
  var records: List[Bytes] = []
  var start = 0
  for at in range(data.len()) {
    if matching(data, at, separator) {
      records += [data[start..at]]
      start = at + 1
    }
  }
  if start < data.len() { records += [data[start..]] }
  records
}

pure utf8_size(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0
  let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if size > 1 and at + size <= data.len() and (data[at..at + size].utf8() ?? "") != "" { size } else { 1 }
}

proc delimiter_unit_size(raw: Bytes, at: Int) [env] -> Int {
  let locale = env.get_or("LC_ALL", "") ?? ""
  let category = if locale == "" { env.get_or("LC_CTYPE", "") ?? "" } else { locale }
  let language = if category == "" { env.get_or("LANG", "") ?? "" } else { category }
  if "gb18030" in language.lower() {
    let lead = raw.byte_at(at) ?? 0
    let trail = raw.byte_at(at + 1) ?? 0
    if lead >= 129 and lead <= 254 and trail >= 64 and trail <= 254 and trail != 127 { return 2 }
  }
  utf8_size(raw, at)
}

proc delimiters(raw: Bytes) [env] -> Result[List[Bytes], Str] {
  var out: List[Bytes] = []
  var at = 0
  while at < raw.len() {
    let byte = raw.byte_at(at) ?? 0
    if byte == 92 {
      if at + 1 >= raw.len() {
        return Err(f"delimiter list ends with an unescaped backslash: {raw.utf8() ?? "?"}")
      }
      let escaped = raw.byte_at(at + 1) ?? 0
      let value = if escaped == 98 { 8 } else if escaped == 102 { 12 } else if escaped == 110 { 10 } else if escaped == 114 { 13 } else if escaped == 116 { 9 } else if escaped == 118 { 11 } else { -1 }
      if escaped == 48 {
        out += [b""]
        at += 2
      } else if value >= 0 {
        out += [bytes.from_ints([value]) ?? b""]
        at += 2
      } else {
        let size = delimiter_unit_size(raw, at + 1)
        out += [raw[at + 1..at + 1 + size]]
        at += 1 + size
      }
    } else {
      let size = delimiter_unit_size(raw, at)
      out += [raw[at..at + size]]
      at += size
    }
  }
  Ok(out)
}

pure raw_arguments(argv: List[Str], raw: List[Bytes]) -> PasteArgs {
  var files: List[Bytes] = []
  var delimiter = b""
  var has_delimiter = false
  var index = 0
  var stopped = false
  while index < argv.len() {
    let arg = argv[index]
    if ! stopped and arg == "--" {
      stopped = true
      index += 1
    } else if ! stopped and (arg == "-d" or arg == "--delimiters") {
      if index + 1 < raw.len() { delimiter = raw[index + 1]; has_delimiter = true }
      index += 2
    } else if ! stopped and arg.starts_with("--delimiters=") {
      delimiter = raw[index].slice((arg.find("=") ?? 0) + 1)
      has_delimiter = true
      index += 1
    } else if ! stopped and ! arg.starts_with("--") and arg.starts_with("-") and arg.find("d") != null {
      let marker_at = arg.find("d") ?? 0
      if marker_at + 1 < arg.byte_len() { delimiter = raw[index].slice(marker_at + 1); has_delimiter = true; index += 1 } else { if index + 1 < raw.len() { delimiter = raw[index + 1]; has_delimiter = true }; index += 2 }
    } else if ! stopped and arg.starts_with("-") and arg != "-" {
      index += 1
    } else {
      files += [raw[index]]
      index += 1
    }
  }
  {files: files, delimiter: delimiter, has_delimiter: has_delimiter}
}

proc read_input(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  let input_path = Path.parse_bytes(name)?
  input_path.read_bytes()
}

pure output_delimiter(delims: List[Bytes], position: Int) -> Bytes {
  return b"" when delims.len() == 0
  delims[position % delims.len()]
}

pure parallel_output(columns: List[List[Bytes]], delims: List[Bytes], mark: Bytes) -> Bytes {
  var rows = 0
  for column in columns { if column.len() > rows { rows = column.len() } }
  var out: List[Bytes] = []
  for row in range(rows) {
    var last = -1
    for col in range(columns.len()) {
      if row < columns[col].len() { last = col }
    }
    for col in range(last + 1) {
      if col > 0 { out += [output_delimiter(delims, col - 1)] }
      if row < columns[col].len() { out += [columns[col][row]] }
    }
    out += [mark]
  }
  bytes.concat(out)
}

pure serial_output(columns: List[List[Bytes]], empty_files: List[Bool], delims: List[Bytes], mark: Bytes) -> Bytes {
  var out: List[Bytes] = []
  for index in range(columns.len()) {
    let column = columns[index]
    if column.len() > 0 or empty_files[index] {
      for row in range(column.len()) {
        if row > 0 { out += [output_delimiter(delims, row - 1)] }
        out += [column[row]]
      }
      out += [mark]
    }
  }
  bytes.concat(out)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: PasteOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      serial: {form: "-s --serial", default: false},
      delimiters: {form: "-d --delimiters LIST", default: [], repeated: true},
      zero: {form: "-z --zero-terminated", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("paste"); return }

  let raw = raw_arguments(argv, cli.argv_bytes())
  var delims: List[Bytes] = [b"\t"]
  if raw.has_delimiter {
    match delimiters(raw.delimiter) {
      Ok(parsed) => { delims = parsed }
      Err(message) => { gnu.error(message); exit 1 }
    }
  }
  let names = if raw.files.len() == 0 { [b"-"] } else { raw.files }
  let sep = if opts.zero { 0 } else { 10 }
  let mark = bytes.from_ints([sep])?
  var wants_stdin = false
  for name in names { wants_stdin = wants_stdin or name == b"-" }
  var stdin_data = b""
  if wants_stdin {
    match io.stdin_bytes() {
      Ok(data) => { stdin_data = data }
      Err(failure) => { gnu.error(f"read error: {gnu.strerror(failure)}"); exit 1 }
    }
  }
  var columns: List[List[Bytes]] = []
  var empty_files: List[Bool] = []
  var stdin_occurrences = 0
  for name in names { if name == b"-" { stdin_occurrences += 1 } }
  var stdin_position = 0
  let stdin_records = split_records(stdin_data, sep)
  var failed = false
  for name in names {
    if name == b"-" {
      var records: List[Bytes] = []
      if opts.serial {
        if stdin_position == 0 { records = stdin_records }
        empty_files += [stdin_position > 0]
      } else {
        for index in range(stdin_records.len()) {
          if index % stdin_occurrences == stdin_position { records += [stdin_records[index]] }
        }
        empty_files += [false]
      }
      columns += [records]
      stdin_position += 1
    } else {
      guard let input = read_input(name) else { |failure|
        gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
        failed = true
        continue
      }
      columns += [split_records(input, sep)]
      empty_files += [false]
    }
  }
  let output = if opts.serial { serial_output(columns, empty_files, delims, mark) } else { parallel_output(columns, delims, mark) }
  gnu.write_bytes(output)
  if failed { exit 1 }
}
