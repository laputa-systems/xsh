#!/bin/xsh
use lib.gnu

const USAGE = """
Usage: pr [OPTION]... [FILE]...
Paginate or columnate FILE(s) for printing.
  +FIRST_PAGE[:LAST_PAGE]  begin and stop printing at page range
  -COLUMN                  output COLUMN columns
  -a                        print columns across rather than down
  -d                        double space the output
  -D FORMAT                 use FORMAT for the date in the header
  -h HEADER                 use HEADER instead of the file name
  -l PAGE_LENGTH            set page length
  -m                        merge files, one per column
  -n[SEP[WIDTH]]            number lines
  -o MARGIN                 set each line's left margin
  -s[CHAR]                  separate columns with CHAR
  -t, -T                    omit headers and pagination
  -w PAGE_WIDTH             set page width
      --help                display this help and exit
      --version             output version information and exit
"""

type PrOptions = {
  across: Bool, double: Bool, expand: Bool, formfeed: Bool, join: Bool,
  merge: Bool, no_header: Bool, no_pagination: Bool, number: Bool,
  number_width: Int, number_separator: Bytes, start_number: Int, omit_header: Bool,
  page_length: Int, page_width: Int, columns: Int, margin: Int,
  column_separator: Bytes, header: Str, date_format: Str,
  first_page: Int, last_page: Int, files: List[Bytes], help: Bool, version: Bool,
}

pure number(value: Str, fallback: Int) -> Int {
  match value.parse_int() { Ok(parsed) => parsed, Err(_) => fallback }
}

pure first_digit(value: Str) -> Int? {
  for at in range(value.byte_len()) {
    let byte = value.byte_at(at) ?? 0
    if byte >= 48 and byte <= 57 { return at }
  }
  null
}

pure parse_option_value(arg: Str, name: Str) -> Str? {
  if arg.starts_with(name + "=") { arg[name.byte_len() + 1..] } else { null }
}

proc parse_args(argv: List[Str], raw: List[Bytes]) [env, process] -> PrOptions {
  var result: PrOptions = {
    across: false, double: false, expand: false, formfeed: false, join: false,
    merge: false, no_header: false, no_pagination: false, number: false,
    number_width: 5, number_separator: b"\t", start_number: 1, omit_header: false,
    page_length: 66, page_width: 72, columns: 1, margin: 0,
    column_separator: b"\t", header: "", date_format: "%Y-%m-%d %H:%M",
    first_page: 1, last_page: 2147483647, files: [], help: false, version: false,
  }
  var stopped = false
  var i = 0
  while i < argv.len() {
    let arg = argv[i]
    if ! stopped and arg == "--" { stopped = true; i += 1; continue }
    if ! stopped and arg == "--help" { result.help = true; i += 1; continue }
    if ! stopped and arg == "--version" { result.version = true; i += 1; continue }
    if ! stopped and arg.starts_with("+") and arg.byte_len() > 1 {
      let spec = arg[1..]
      let colon = spec.find(":")
      result.first_page = number(if colon == null { spec } else { spec[..colon ?? 0] }, 1)
      if colon != null { result.last_page = number(spec[(colon ?? 0) + 1..], 2147483647) }
      i += 1
      continue
    }
    if ! stopped and arg.starts_with("--") {
      let equals = arg.find("=")
      let name = if equals == null { arg } else { arg[..equals ?? 0] }
      let inline = parse_option_value(arg, name)
      if name == "--across" { result.across = true } else if name == "--double-space" { result.double = true } else if name == "--form-feed" { result.formfeed = true } else if name == "--join-lines" { result.join = true } else if name == "--merge" { result.merge = true } else if name == "--omit-header" { result.no_header = true } else if name == "--omit-pagination" { result.no_pagination = true; result.no_header = true } else if name == "--number-lines" { result.number = true; if inline != null { result.number_width = number(inline ?? "5", 5) } } else if name == "--columns" or name == "--page-length" or name == "--page-width" or name == "--width" or name == "--length" or name == "--indent" or name == "--header" or name == "--date-format" or name == "--pages" or name == "--first-line-number" {
        let value = if inline != null { inline ?? "" } else if i + 1 < argv.len() { i += 1; argv[i] } else { "" }
        if name == "--columns" { result.columns = number(value, -1) } else if name == "--page-length" or name == "--length" { result.page_length = number(value, -1) } else if name == "--page-width" or name == "--width" { result.page_width = number(value, -1) } else if name == "--indent" { result.margin = number(value, -1) } else if name == "--header" { result.header = value } else if name == "--date-format" { result.date_format = value } else if name == "--first-line-number" { result.start_number = number(value, -1) } else if name == "--pages" {
          let colon = value.find(":")
          result.first_page = number(if colon == null { value } else { value[..colon ?? 0] }, 1)
          if colon != null { result.last_page = number(value[(colon ?? 0) + 1..], 2147483647) }
        }
      } else { gnu.usage_error(f"unrecognized option {gnu.quote_value(arg)}") }
      i += 1
      continue
    }
    if ! stopped and arg.starts_with("-") and arg != "-" {
      if rx"^-[0-9]+$".matches(arg) { result.columns = number(arg[1..], -1); i += 1; continue }
      var at = 1
      while at < arg.byte_len() {
        let flag = arg.byte_slice(at, length: 1)
        let rest = arg.byte_slice(at + 1)
        if flag == "a" { result.across = true; at += 1 } else if flag == "b" or flag == "c" { at += 1 } else if flag == "d" { result.double = true; at += 1 } else if flag == "f" { result.formfeed = true; at += 1 } else if flag == "J" { result.join = true; at += 1 } else if flag == "m" { result.merge = true; at += 1 } else if flag == "r" { at += 1 } else if flag == "t" { result.no_header = true; result.no_pagination = true; at += 1 } else if flag == "T" { result.no_header = true; result.no_pagination = true; at += 1 } else if flag == "n" {
          result.number = true
          var value = rest
          if value == "" and i + 1 < argv.len() and rx"^[0-9]+$".matches(argv[i + 1]) { i += 1; value = argv[i] }
          if value != "" {
            let digit = first_digit(value)
            if digit == 0 { result.number_width = number(value, 5) } else if digit != null { result.number_separator = bytes.from_text(value[..digit ?? 0]); result.number_width = number(value[digit ?? 0..], 5) } else { result.number_separator = bytes.from_text(value) }
            at = arg.byte_len()
          } else { at += 1 }
        } else if flag == "N" {
          var value = rest
          if value == "" and i + 1 < argv.len() { i += 1; value = argv[i] }
          result.start_number = number(value, -1)
          at = arg.byte_len()
        } else if flag == "s" or flag == "S" {
          result.column_separator = if rest == "" { if flag == "s" { b"\t" } else { b" " } } else { bytes.from_text(rest) }
          at = arg.byte_len()
        } else if flag == "e" or flag == "i" { result.expand = true; at = arg.byte_len() } else if flag == "l" or flag == "o" or flag == "w" or flag == "W" or flag == "h" or flag == "D" {
          var value = rest
          if value == "" and i + 1 < argv.len() { i += 1; value = argv[i] }
          if flag == "l" { result.page_length = number(value, -1) } else if flag == "o" { result.margin = number(value, -1) } else if flag == "w" or flag == "W" { result.page_width = number(value, -1) } else if flag == "h" { result.header = value } else if flag == "D" { result.date_format = value }
          at = arg.byte_len()
        } else { gnu.usage_error(f"invalid option -- '{flag}'") }
      }
      i += 1
      continue
    }
    result.files += [raw[i]]
    i += 1
  }
  result
}

pure split_lines(data: Bytes) -> List[Bytes] {
  var rows: List[Bytes] = []
  var start = 0
  for at in range(data.len()) {
    if (data.byte_at(at) ?? -1) == 10 {
      rows += [data[start..at]]
      start = at + 1
    }
  }
  if start < data.len() { rows += [data[start..]] }
  rows
}

pure repeat_byte(value: Bytes, count: Int) -> Bytes {
  var out: List[Bytes] = []
  for _ in range(if count > 0 and count < 100000 { count } else { 0 }) { out += [value] }
  bytes.concat(out)
}

pure pad_right(value: Bytes, width: Int) -> Bytes {
  bytes.concat([value, repeat_byte(b" ", if width > value.len() { width - value.len() } else { 0 })])
}

pure number_line(line: Bytes, index: Int, opts: PrOptions) -> Bytes {
  return line when ! opts.number
  let digits = f"{index}"
  let fill = if opts.number_width > digits.byte_len() and opts.number_width < 10000 { opts.number_width - digits.byte_len() } else { 0 }
  bytes.concat([repeat_byte(b" ", fill), bytes.from_text(digits), opts.number_separator, line])
}

pure column_page(lines: List[Bytes], opts: PrOptions, col_width: Int) -> Bytes {
  var out: List[Bytes] = []
  let per_column = (lines.len() + opts.columns - 1) / opts.columns
  let row_count = if opts.across { per_column } else { per_column }
  for row in range(row_count) {
    var last = -1
    for col in range(opts.columns) {
      let index = if opts.across { row * opts.columns + col } else { col * per_column + row }
      if index < lines.len() { last = col }
    }
    for col in range(last + 1) {
      let index = if opts.across { row * opts.columns + col } else { col * per_column + row }
      if col > 0 { out += [opts.column_separator] }
      let cell = if index < lines.len() { number_line(lines[index], opts.start_number + index, opts) } else { b"" }
      out += [pad_right(cell, col_width)]
    }
    out += [b"\n"]
    if opts.double { out += [b"\n"] }
  }
  bytes.concat(out)
}

pure simple_page(lines: List[Bytes], start: Int, opts: PrOptions) -> Bytes {
  var out: List[Bytes] = []
  for at in range(lines.len()) {
    out += [number_line(lines[at], start + at, opts), b"\n"]
    if opts.double { out += [b"\n"] }
  }
  bytes.concat(out)
}

proc title_date(name: Bytes, opts: PrOptions) [fs, error, time] -> Result[Str] {
  let format = if opts.date_format.starts_with("+") { opts.date_format[1..] } else { opts.date_format }
  if name != b"-" {
    if let Ok(meta) = fs.stat(Path.parse_bytes(name)?, follow_symlinks: true) {
      return time.format(meta.mtime_ns / 1000000000, meta.mtime_ns % 1000000000,
        format, "local", "gregorian", "locale")
    }
  }
  let now = time.wall_now()
  time.format(now.seconds, now.nanoseconds, format, "local", "gregorian", "locale")
}

pure header_line(date: Str, title: Str, page: Int, width: Int) -> Bytes {
  let page_text = f"Page {page}"
  let title_start = (width - title.byte_len()) / 2
  let date_end = date.byte_len() + 2
  let page_start = width - page_text.byte_len()
  let prefix = date + repeat_str(" ", if title_start > date_end { title_start - date_end } else { 1 })
  let middle = prefix + title
  let suffix = repeat_str(" ", if page_start > middle.byte_len() { page_start - middle.byte_len() } else { 1 })
  bytes.from_text(middle + suffix + page_text)
}

pure repeat_str(value: Str, count: Int) -> Str {
  var out = ""
  for _ in range(if count > 0 and count < 100000 { count } else { 0 }) { out += value }
  out
}

proc render_page(lines: List[Bytes], name: Bytes, page: Int, first_line: Int, opts: PrOptions) [fs, error, time, env] -> Result[Bytes] {
  let width = if opts.page_width <= opts.margin { 72 } else { opts.page_width - opts.margin }
  let title = if opts.header != "" { opts.header } else if name == b"-" { "" } else { name.utf8() ?? "" }
  var out: List[Bytes] = []
  if ! opts.no_header {
    let date = title_date(name, opts)?
    out += [b"\n\n", header_line(date, title, page, if width > 0 and width < 10000 { width } else { 72 }), b"\n\n\n"]
  }
  let gap = opts.column_separator.len() * (opts.columns - 1)
  let col_width = if width > gap and opts.columns > 0 { (width - gap) / opts.columns } else { 1 }
  let content = if opts.columns > 1 { column_page(lines, opts, col_width) } else { simple_page(lines, first_line, opts) }
  out += [repeat_byte(b" ", opts.margin), content]
  if ! opts.no_pagination {
    let used = lines.len() * (if opts.double { 2 } else { 1 })
    let overhead = if opts.no_header { 0 } else { 10 }
    let blanks = opts.page_length - overhead - used
    if blanks > 0 { out += [repeat_byte(b"\n", if blanks < 10000 { blanks } else { 0 })] }
    if opts.formfeed { out += [b"\x0c"] }
  }
  Ok(bytes.concat(out))
}

proc read_data(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  Path.parse_bytes(name)?.read_bytes()
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  var opts = parse_args(argv, cli.argv_bytes())
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("pr"); return }
  if opts.page_length > 0 and opts.page_length <= 10 { opts.no_header = true; opts.no_pagination = true }
  if opts.columns <= 0 { gnu.error("invalid number of columns"); exit 1 }
  if opts.page_width < 0 or opts.page_length < 0 or opts.margin < 0 { gnu.error("invalid line or page width"); exit 1 }
  let names = if opts.files.len() == 0 { [b"-"] } else { opts.files }
  var failed = false
  var page_number = 0
  for name in names {
    guard let data = read_data(name) else { |failure|
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    let lines = split_lines(data)
    let per_page = if opts.no_pagination { if lines.len() > 0 { lines.len() } else { 1 } } else {
      let slots = opts.page_length - (if opts.no_header { 0 } else { 10 })
      if slots > 0 { slots / (if opts.double { 2 } else { 1 }) } else { 1 }
    }
    let pages = if lines.len() == 0 { 0 } else { (lines.len() + per_page - 1) / per_page }
    for p in range(pages) {
      page_number += 1
      if page_number < opts.first_page or page_number > opts.last_page { continue }
      let start = p * per_page
      let finish = if start + per_page < lines.len() { start + per_page } else { lines.len() }
      let page_lines = lines[start..finish]
      gnu.write_bytes(render_page(page_lines, name, page_number, start + opts.start_number, opts)?)
    }
  }
  if failed { exit 1 }
}
