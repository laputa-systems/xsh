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
  across: Bool, double: Bool, expand: Bool, expand_char: Int, expand_width: Int, formfeed: Bool, join: Bool,
  merge: Bool, no_header: Bool, no_pagination: Bool, number: Bool,
  number_width: Int, number_separator: Bytes, start_number: Int, omit_header: Bool,
  page_length: Int, page_width: Int, columns: Int, margin: Int,
  page_length_option: Str, page_width_option: Str,
  column_separator: Bytes, header: Str, date_format: Str, date_format_given: Bool,
  first_page: Int, last_page: Int, invalid_page_range: Str?, suppress_errors: Bool,
  files: List[Bytes], help: Bool, version: Bool,
}

type PrInputPage = {data: Bytes, feed_after: Bool}

pure number(value: Str, fallback: Int) -> Int {
  match value.parse_int() { Ok(parsed) => parsed, Err(_) => fallback }
}

proc checked_number(value: Str, context: Str, with_help: Bool) [process, env] -> Int {
  let parsed = match value.parse_int() {
    Ok(parsed) => parsed
    Err(_) => {
      let suffix = if rx"^-?[0-9]+$".matches(value) { ": Value too large for defined data type" } else { "" }
      gnu.error(f"{context}: '{value}'{suffix}")
      if with_help { gnu.try_help() }
      exit 1
    }
  }
  if parsed > 2147483647 or parsed < -2147483648 {
    gnu.error(f"{context}: '{value}': Value too large for defined data type")
    if with_help { gnu.try_help() }
    exit 1
  }
  parsed
}

pure first_digit(value: Str) -> Int? {
  for at in range(value.byte_len()) {
    let byte = value.byte_at(at) ?? 0
    if byte >= 48 and byte <= 57 { return at }
  }
  null
}

pure number_spec(value: Str) -> Bool {
  if value == "-" { return false }
  let digit = first_digit(value)
  if digit == null { return value.byte_len() == 1 }
  if digit == 0 { return rx"^[0-9]+$".matches(value) }
  if digit == 1 { return rx"^[0-9]*$".matches(value[1..]) }
  false
}

pure parse_option_value(arg: Str, name: Str) -> Str? {
  if arg.starts_with(name + "=") { arg[name.byte_len() + 1..] } else { null }
}

proc parse_args(argv: List[Str], raw: List[Bytes]) [env, process] -> PrOptions {
  var result: PrOptions = {
    across: false, double: false, expand: false, expand_char: -1, expand_width: 8, formfeed: false, join: false,
    merge: false, no_header: false, no_pagination: false, number: false,
    number_width: 5, number_separator: b"\t", start_number: 1, omit_header: false,
    page_length: 66, page_width: 72, columns: 1, margin: 0,
    page_length_option: "--length", page_width_option: "--width",
    column_separator: b"\t", header: "", date_format: "%Y-%m-%d %H:%M", date_format_given: false,
    first_page: 1, last_page: 2147483647, invalid_page_range: null, suppress_errors: false,
    files: [], help: false, version: false,
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
      let first = number(if colon == null { spec } else { spec[..colon ?? 0] }, -1)
      let last = if colon == null { 2147483647 } else { number(spec[(colon ?? 0) + 1..], -1) }
      if first <= 0 or last < first { result.invalid_page_range = f"invalid page range '{spec}'" }
      result.first_page = first
      result.last_page = last
      i += 1
      continue
    }
    if ! stopped and arg.starts_with("--") {
      let equals = arg.find("=")
      let name = if equals == null { arg } else { arg[..equals ?? 0] }
      let inline = parse_option_value(arg, name)
      if name == "--across" { result.across = true } else if name == "--double-space" { result.double = true } else if name == "--form-feed" { result.formfeed = true } else if name == "--join-lines" { result.join = true } else if name == "--merge" { result.merge = true } else if name == "--omit-header" { result.no_header = true } else if name == "--omit-pagination" { result.no_pagination = true; result.no_header = true } else if name == "--number-lines" {
        result.number = true
        if inline != null {
          let value = inline ?? ""
          if value == "" { gnu.usage_error("'-n' extra characters or invalid number in the argument") }
          result.number_width = checked_number(value, "'-n' extra characters or invalid number in the argument", true)
        }
      } else if name == "--columns" or name == "--page-length" or name == "--page-width" or name == "--width" or name == "--length" or name == "--indent" or name == "--header" or name == "--date-format" or name == "--pages" or name == "--first-line-number" {
        let value = if inline != null { inline ?? "" } else if i + 1 < argv.len() { i += 1; argv[i] } else { "" }
      if name == "--columns" { result.columns = checked_number(value, "invalid number of columns", false) } else if name == "--page-length" or name == "--length" { result.page_length = checked_number(value, "'-l PAGE_LENGTH' invalid number of lines", false); result.page_length_option = name } else if name == "--page-width" or name == "--width" { result.page_width = checked_number(value, "'-w PAGE_WIDTH' invalid number of characters", false); result.page_width_option = name } else if name == "--indent" { result.margin = checked_number(value, "'-o MARGIN' invalid line offset", false) } else if name == "--header" { result.header = value } else if name == "--date-format" { result.date_format = value; result.date_format_given = true } else if name == "--first-line-number" { result.start_number = checked_number(value, "'-N NUMBER' invalid starting line number", false) } else if name == "--pages" {
          let colon = value.find(":")
          let first = number(if colon == null { value } else { value[..colon ?? 0] }, -1)
          let last = if colon == null { 2147483647 } else { number(value[(colon ?? 0) + 1..], -1) }
          if first <= 0 or last < first { result.invalid_page_range = f"invalid --pages argument '{value}'" }
          result.first_page = first
          result.last_page = last
        }
      } else { gnu.usage_error(f"unrecognized option {gnu.quote_value(arg)}") }
      i += 1
      continue
    }
    if ! stopped and arg.starts_with("-") and arg != "-" {
      if rx"^-[0-9]+$".matches(arg) { result.columns = checked_number(arg[1..], "invalid number of columns", false); i += 1; continue }
      var at = 1
      while at < arg.byte_len() {
        let flag = arg.byte_slice(at, length: 1)
        let rest = arg.byte_slice(at + 1)
        if flag == "a" { result.across = true; at += 1 } else if flag == "b" or flag == "c" { at += 1 } else if flag == "d" { result.double = true; at += 1 } else if flag == "f" { result.formfeed = true; at += 1 } else if flag == "J" { result.join = true; at += 1 } else if flag == "m" { result.merge = true; at += 1 } else if flag == "r" { result.suppress_errors = true; at += 1 } else if flag == "t" { result.no_header = true; result.no_pagination = true; at += 1 } else if flag == "T" { result.no_header = true; result.no_pagination = true; at += 1 } else if flag == "n" {
          result.number = true
          var value = rest
          if value == "" and i + 1 < argv.len() and number_spec(argv[i + 1]) { i += 1; value = argv[i] }
          if value != "" {
            let digit = first_digit(value)
            if digit == 0 { result.number_width = checked_number(value, "'-n' extra characters or invalid number in the argument", true) } else if digit != null {
              result.number_separator = bytes.from_text(value[..digit ?? 0])
              if result.number_separator.len() > 1 { gnu.usage_error(f"'-n' extra characters or invalid number in the argument: ‘{value[..digit ?? 0]}’") }
              result.number_width = checked_number(value[digit ?? 0..], "'-n' extra characters or invalid number in the argument", true)
            } else {
              result.number_separator = bytes.from_text(value)
              if result.number_separator.len() > 1 { gnu.usage_error(f"'-n' extra characters or invalid number in the argument: ‘{value}’") }
            }
            at = arg.byte_len()
          } else { at += 1 }
        } else if flag == "N" {
          var value = rest
          if value == "" and i + 1 < argv.len() { i += 1; value = argv[i] }
          result.start_number = checked_number(value, "'-N NUMBER' invalid starting line number", false)
          at = arg.byte_len()
        } else if flag == "s" or flag == "S" {
          result.column_separator = if rest == "" { if flag == "s" { b"\t" } else { b" " } } else { bytes.from_text(rest) }
          at = arg.byte_len()
        } else if flag == "e" {
          result.expand = true
          if rest == "" { at += 1 } else {
            if rest.starts_with("=") { gnu.usage_error(f"'-e' extra characters or invalid number in the argument: ‘{rest[1..]}’") }
            let digit = first_digit(rest)
            let char = if digit == 0 { "" } else if digit != null { rest[..digit ?? 0] } else { rest[..1] }
            let width = if digit == 0 { rest } else if digit != null { rest[digit ?? 0..] } else { "" }
            if char.byte_len() > 1 or (digit == null and rest.byte_len() > 1) {
              let extra = if digit == null { rest[1..] } else { rest[..digit ?? 0][1..] }
              gnu.usage_error(f"'-e' extra characters or invalid number in the argument: ‘{extra}’")
            }
            if char != "" { result.expand_char = char.byte_at(0) ?? -1 }
            if width != "" {
              let parsed = number(width, -1)
              if ! rx"^[0-9]+$".matches(width) or parsed <= 0 or parsed > 2147483647 {
                let invalid = if rx"^[0-9]+$".matches(width) and parsed > 2147483647 { f"{width}" } else { width }
                gnu.usage_error(f"'-e' extra characters or invalid number in the argument: ‘{invalid}’")
              }
              result.expand_width = parsed
            }
            at = arg.byte_len()
          }
        } else if flag == "i" { result.expand = true; at = arg.byte_len() } else if flag == "l" or flag == "o" or flag == "w" or flag == "W" or flag == "h" or flag == "D" {
          var value = rest
          if value == "" and i + 1 < argv.len() { i += 1; value = argv[i] }
          if flag == "l" { result.page_length = checked_number(value, "'-l PAGE_LENGTH' invalid number of lines", false); result.page_length_option = "--length" } else if flag == "o" { result.margin = checked_number(value, "'-o MARGIN' invalid line offset", false) } else if flag == "w" or flag == "W" { result.page_width = checked_number(value, f"'-{flag} PAGE_WIDTH' invalid number of characters", false); result.page_width_option = if flag == "W" { "--page-width" } else { "--width" } } else if flag == "h" { result.header = value } else if flag == "D" { result.date_format = value; result.date_format_given = true }
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

pure split_formfeeds(data: Bytes) -> List[PrInputPage] {
  var pages: List[PrInputPage] = []
  var start = 0
  var at = 0
  while at < data.len() {
    if data.byte_at(at) == 12 {
      pages += [{data: data[start..at], feed_after: true}]
      start = at + 1
      if start < data.len() and data.byte_at(start) == 10 { start += 1; at += 1 }
    }
    at += 1
  }
  if start < data.len() { pages += [{data: data[start..], feed_after: false}] }
  if pages.len() == 0 and data.len() > 0 { pages += [{data: data, feed_after: false}] }
  pages
}

pure repeat_byte(value: Bytes, count: Int) -> Bytes {
  var out: List[Bytes] = []
  for _ in range(if count > 0 and count < 100000 { count } else { 0 }) { out += [value] }
  bytes.concat(out)
}

pure pad_right(value: Bytes, width: Int) -> Bytes {
  bytes.concat([value, repeat_byte(b" ", if width > value.len() { width - value.len() } else { 0 })])
}

pure expand_tabs(value: Bytes, opts: PrOptions) -> Bytes {
  var out: List[Bytes] = []
  var column = 0
  for at in range(value.len()) {
    let byte = value.byte_at(at) ?? 0
    if opts.expand and (byte == 9 or byte == opts.expand_char) {
      let width = if byte == 9 and opts.expand_char >= 0 { 8 } else { opts.expand_width }
      let spaces = width - column % width
      out += [repeat_byte(b" ", spaces)]
      column += spaces
    } else {
      out += [value[at..at + 1]]
      column += if byte == 8 { if column > 0 { -1 } else { 0 } } else if byte == 13 { -column } else { 1 }
    }
  }
  bytes.concat(out)
}

pure number_line(line: Bytes, index: Int, opts: PrOptions) -> Bytes {
  return line when ! opts.number
  var digits_value = index
  var wrapped = false
  if opts.number_width > 0 and opts.number_width <= 9 {
    var limit = 1
    for _ in range(opts.number_width) { limit *= 10 }
    if digits_value >= limit { digits_value %= limit; wrapped = true }
  }
  let digits = f"{digits_value}"
  let fill = if opts.number_width > digits.byte_len() { opts.number_width - digits.byte_len() } else { 0 }
  bytes.concat([repeat_byte(if wrapped { b"0" } else { b" " }, fill), bytes.from_text(digits), opts.number_separator, line])
}

pure column_page(lines: List[Bytes], opts: PrOptions, col_width: Int, first_line: Int) -> Bytes {
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
      let raw = if index < lines.len() { if opts.merge { lines[index] } else { number_line(lines[index], first_line + index, opts) } } else { b"" }
      let cell = expand_tabs(raw, opts)
      let cell_width = if opts.merge and opts.number and col == 0 {
        let overhead = opts.number_width + opts.number_separator.len() + 1
        if col_width > overhead { col_width - overhead } else { 1 }
      } else { col_width }
      let clipped = if cell.len() > cell_width { cell[..cell_width] } else { cell }
      var tab_adjust = 0
      if opts.merge and ! (opts.number and col == 0) {
        for at in range(clipped.len()) { if clipped.byte_at(at) == 9 { tab_adjust += 7 } }
      }
      let occupied = clipped.len() + tab_adjust
      let padding = if cell_width > occupied { cell_width - occupied } else { 0 }
      out += [repeat_byte(b" ", opts.margin), clipped, repeat_byte(b" ", padding)]
    }
    out += [b"\n"]
    if opts.double { out += [b"\n"] }
  }
  bytes.concat(out)
}

pure simple_page(lines: List[Bytes], start: Int, opts: PrOptions) -> Bytes {
  var out: List[Bytes] = []
  for at in range(lines.len()) {
    out += [expand_tabs(number_line(lines[at], start + at, opts), opts), b"\n"]
    if opts.double { out += [b"\n"] }
  }
  bytes.concat(out)
}

proc title_date(name: Bytes, opts: PrOptions) [fs, error, time, env] -> Result[Str] {
  let all = env.get_or("LC_ALL", "") ?? ""
  let time_locale = env.get_or("LC_TIME", "") ?? ""
  let posix = (env.get_or("POSIXLY_CORRECT", "") ?? "") != "" and (all == "POSIX" or time_locale == "POSIX")
  let raw_format = if ! opts.date_format_given and posix { "%b %e %H:%M %Y" } else { opts.date_format }
  let format = if raw_format.starts_with("+") { raw_format[1..] } else { raw_format }
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
  let room = width - date.byte_len() - title.byte_len() - page_text.byte_len()
  if title == "" { return bytes.from_text(date + repeat_str(" ", if room > 0 { room } else { 1 }) + page_text) }
  let before = if room > 1 { room / 2 } else { 1 }
  let after = if room > 1 { room - before } else { 1 }
  bytes.from_text(date + repeat_str(" ", before) + title + repeat_str(" ", after) + page_text)
}

pure repeat_str(value: Str, count: Int) -> Str {
  var out = ""
  for _ in range(if count > 0 and count < 100000 { count } else { 0 }) { out += value }
  out
}

proc render_page(lines: List[Bytes], name: Bytes, page: Int, first_line: Int, opts: PrOptions) [fs, error, time, env] -> Result[Bytes] {
  let width = if opts.page_width <= opts.margin { 72 } else { opts.page_width - opts.margin }
  let header_width = if opts.page_width > 0 and opts.page_width < 10000 { opts.page_width } else { 72 }
  let title = if opts.header != "" { opts.header } else if opts.merge or name == b"-" { "" } else { name.utf8() ?? "" }
  var out: List[Bytes] = []
  if ! opts.no_header {
    let date = title_date(name, opts)?
    out += [b"\n\n", header_line(date, title, page, header_width), b"\n\n\n"]
  }
  if opts.formfeed and opts.no_pagination and lines.len() == 0 and ! opts.no_header { out += [b"\n"] }
  let gap = opts.column_separator.len() * (opts.columns - 1)
  let default_col_width = if width > gap and opts.columns > 0 { (width - gap) / opts.columns } else { 1 }
  var col_width = default_col_width
  if opts.number and opts.columns > 1 and ! opts.merge {
    var max_cell_width = 0
    for at in range(lines.len()) {
      let size = number_line(lines[at], first_line + at, opts).len()
      if size > max_cell_width { max_cell_width = size }
    }
    let tabbed_width = (max_cell_width + 7) / 8 * 8
    if tabbed_width > 0 and tabbed_width < col_width { col_width = tabbed_width }
  }
  let content = if opts.columns > 1 { column_page(lines, opts, col_width, first_line) } else { simple_page(lines, first_line, opts) }
  if opts.columns > 1 { out += [content] } else { out += [repeat_byte(b" ", opts.margin), content] }
  if ! opts.no_pagination {
    let row_count = if opts.columns > 1 { (lines.len() + opts.columns - 1) / opts.columns } else { lines.len() }
    let used = row_count * (if opts.double { 2 } else { 1 })
    let overhead = if opts.no_header { 0 } else { 5 }
    let blanks = opts.page_length - overhead - used
    if blanks > 0 { out += [repeat_byte(b"\n", if blanks < 10000 { blanks } else { 0 })] }
  }
  if opts.formfeed { out += [b"\x0c"] }
  Ok(bytes.concat(out))
}

proc read_data(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  Path.parse_bytes(name)?.read_bytes()
}

proc print_merged(names: List[Bytes], opts: PrOptions) [fs, process, env, error, io, time] -> Result[Bool] {
  if opts.across { gnu.error("cannot specify both printing across and printing in parallel"); exit 1 }
  if opts.columns > 1 { gnu.error("cannot specify number of columns when printing in parallel"); exit 1 }
  var inputs: List[List[Bytes]] = []
  var longest = 0
  var failed = false
  for name in names {
    guard let data = read_data(name) else { |failure|
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
      failed = true
      inputs += [[]]
      continue
    }
    let lines = split_lines(data)
    if lines.len() > longest { longest = lines.len() }
    inputs += [lines]
  }
  let column_count = if opts.join { 1 } else { names.len() }
  var render_opts = opts
  render_opts.columns = column_count
  render_opts.across = true
  render_opts.number = opts.number and ! opts.join
  let slots = opts.page_length - (if opts.no_header { 0 } else { 10 })
  let rows_per_page = if slots > 0 { slots / (if opts.double { 2 } else { 1 }) } else { 1 }
  let pages = if longest == 0 { 0 } else { (longest + rows_per_page - 1) / rows_per_page }
  if pages > 0 and opts.first_page > pages { gnu.error(f"starting page number {opts.first_page} exceeds page count {pages}") }
  var page_number = 0
  var page_start = 0
  while page_start < longest {
    page_number += 1
    let page_end = if page_start + rows_per_page < longest { page_start + rows_per_page } else { longest }
    if page_number >= opts.first_page and page_number <= opts.last_page {
      var page_lines: List[Bytes] = []
      for row in range(page_start, page_end) {
        var row_cells: List[Bytes] = []
        for col in range(inputs.len()) {
          let source = inputs[col]
          let line = if row < source.len() { source[row] } else { b"" }
          row_cells += [if opts.number and col == 0 { number_line(line, opts.start_number + row, opts) } else { line }]
        }
        if opts.join { page_lines += [bytes.concat(row_cells)] } else { page_lines += row_cells }
      }
      if ! opts.no_pagination {
        for _ in range(page_end - page_start, rows_per_page) {
          if opts.join { page_lines += [b""] } else {
            for _ in range(names.len()) { page_lines += [b""] }
          }
        }
      }
      gnu.write_bytes(render_page(page_lines, if names.len() > 0 { names[0] } else { b"-" }, page_number, page_start, render_opts)?)
    }
    page_start = page_end
  }
  Ok(failed)
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  var opts = parse_args(argv, cli.argv_bytes())
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("pr"); return }
  if opts.invalid_page_range != null {
    if ! opts.suppress_errors { gnu.error(opts.invalid_page_range ?? "invalid page range") }
    exit 1
  }
  if opts.page_length > 0 and opts.page_length <= 10 { opts.no_header = true; opts.no_pagination = true }
  if opts.columns <= 0 { gnu.error(f"invalid --columns argument '{opts.columns}'"); exit 1 }
  if opts.page_length == 0 { gnu.error(f"invalid {opts.page_length_option} argument '0'"); exit 1 }
  if opts.page_width == 0 { gnu.error(f"invalid {opts.page_width_option} argument '0'"); exit 1 }
  if opts.margin < 0 { gnu.error(f"'-o MARGIN' invalid line offset: '{opts.margin}'"); exit 1 }
  if opts.page_width < 0 or opts.page_length < 0 { gnu.error("invalid line or page width"); exit 1 }
  let names = if opts.files.len() == 0 { [b"-"] } else { opts.files }
  if opts.merge { if print_merged(names, opts)? { exit 1 }; return }
  var failed = false
  var page_number = 0
  for name in names {
    guard let data = read_data(name) else { |failure|
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    for source_page in split_formfeeds(data) {
      let lines = split_lines(source_page.data)
      let slots = opts.page_length - (if opts.no_header { 0 } else { 10 })
      let rows = if slots > 0 { slots / (if opts.double { 2 } else { 1 }) } else { 1 }
      let per_page = if opts.columns > 1 { rows * opts.columns } else { rows }
      let pages = if lines.len() == 0 { if source_page.feed_after { 1 } else { 0 } } else { (lines.len() + per_page - 1) / per_page }
      if pages > 0 and opts.first_page > pages { gnu.error(f"starting page number {opts.first_page} exceeds page count {pages}") }
      for p in range(pages) {
        page_number += 1
        if page_number < opts.first_page or page_number > opts.last_page { continue }
        let start = p * per_page
        let finish = if start + per_page < lines.len() { start + per_page } else { lines.len() }
        let page_lines = if lines.len() == 0 { [] } else { lines[start..finish] }
        var render_opts = opts
        if source_page.feed_after and p == pages - 1 and opts.formfeed { render_opts.no_pagination = true }
        gnu.write_bytes(render_page(page_lines, name, page_number, start + opts.start_number, render_opts)?)
      }
    }
  }
  if failed { exit 1 }
}
