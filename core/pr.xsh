#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {columns: Str, across: Bool, merge: Bool, double: Bool, omit_header: Bool, omit_pages: Bool, formfeed: Bool, header: Str?, date_format: Str?, length: Str, width: Str, page_width: Str, offset: Str, numbers: Str?, first_number: Str, separator: Str?, separator_string: Str?, expand_tabs: Str?, output_tabs: Str?, page_range: Str, join: Bool, control: Bool, nonprinting: Bool, quiet: Bool, back_compat: Bool, help: Bool, version: Bool, paths: List[Str]}

proc size(value: Str, message: Str, minimum = 1) -> Int {
  if value == "-2147483648" and minimum == -2147483648 { return -2147483648 }
  let numeric = rx"^[+-]?[0-9]+$".matches(value)
  match value.parse_int() {
    Ok(number) => {
      if number >= minimum and number <= 2147483647 { return number }
      let overflow = number < -2147483648 or number > 2147483647
      let reason = if overflow { ": Value too large for defined data type" } else if number == 0 and minimum == 1 { ": Result not representable" } else { "" }
      gnu.error(f"{message}: {gnu.quote_value(value)}{reason}")
    }
    Err(_) => gnu.error(f"{message}: {gnu.quote_value(value)}{if numeric { ": Value too large for defined data type" } else { "" }}")
  }
  exit 1
}

proc page_error(value: Str, quiet: Bool) {
  if ! quiet { gnu.error(f"invalid page range {gnu.quote_value(value)}") }
  exit 1
}

pure page_decimal(value: Str) -> Str {
  var at = if value.starts_with("+") { 1 } else { 0 }
  while at + 1 < value.byte_len() and value.byte_slice(at, length: 1) == "0" { at += 1 }
  value.byte_slice(at)
}

pure decimal_greater(left: Str, right: Str) -> Bool {
  if left.byte_len() != right.byte_len() { return left.byte_len() > right.byte_len() }
  let a = bytes.from_text(left)
  let b = bytes.from_text(right)
  for at in range(a.len()) {
    if a.byte_at(at) != b.byte_at(at) { return (a.byte_at(at) ?? 0) > (b.byte_at(at) ?? 0) }
  }
  false
}

type PageNumber = {value: Int, display: Str}
proc page_number(value: Str, range: Str, quiet: Bool) -> PageNumber {
  if rx"^-[0-9]+$".matches(value) {
    gnu.error(f"invalid --pages argument {gnu.quote_value(range)}")
    exit 1
  }
  if ! rx"^\+?[0-9]+$".matches(value) { page_error(range, quiet) }
  let digits = page_decimal(value)
  if digits == "0" { page_error(range, quiet) }
  if decimal_greater(digits, "18446744073709551615") {
    gnu.error(f"invalid page range {gnu.quote_value(range)}: Value too large for defined data type")
    exit 1
  }
  match digits.parse_int() {
    Ok(number) => {value: number, display: digits},
    Err(_) => {value: 9223372036854775807, display: digits},
  }
}

pure modernize(argv: List[Str]) -> List[Str] {
  var options = true
  var at = 0
  collect {
    while at < argv.len() {
      let item = argv[at]
      if item == "--" { options = false }
      if options and rx"^-t[0-9]+$".matches(item) { yield @["-t", "--columns", item.byte_slice(2)] } else if options and rx"^-[0-9]+$".matches(item) { yield @["--columns", item.byte_slice(1)] } else if options and rx"^\+0*[1-9][0-9]*(:[0-9]+)?$".matches(item) { yield @["--pages", item.byte_slice(1)] } else if options and item in ["-n", "-e", "-i"] and at + 1 < argv.len() and rx"^([0-9]+|[^0-9][0-9]*)$".matches(argv[at + 1]) and ! argv[at + 1].starts_with("-") {
        yield item + argv[at + 1]
        at += 1
      } else { yield item }
      at += 1
    }
  }
}

type TabOption = {character: Int, width: Int}
# The -e width diagnostic omits the overflow note that the -n diagnostic carries.
proc optional_number(digits: Str, option: Str, minimum = 1, overflow_note = true) -> Int {
  let number = digits.parse_int() ?? -1
  if number < minimum or number > 2147483647 {
    let overflow = overflow_note and rx"^[0-9]+$".matches(digits) and (number < 0 or number > 2147483647)
    gnu.usage_error(f"'{option}' extra characters or invalid number in the argument: {gnu.quote_value(digits)}{if overflow { ": Value too large for data type" } else { "" }}")
  }
  number
}

proc tab_option(spec: Str, option: Str) -> TabOption {
  if spec == "" { gnu.usage_error(f"'{option}' extra characters or invalid number in the argument") }
  let negative_width = rx"^-[0-9]+$".matches(spec)
  let numeric = rx"^[0-9]".matches(spec)
  if negative_width { gnu.usage_error(f"'{option}' extra characters or invalid number in the argument: {gnu.quote_value(spec)}") }
  let character = if numeric { 9 } else { bytes.from_text(spec[..1]).byte_at(0) ?? 9 }
  if ! numeric and bytes.from_text(spec[..1]).len() > 1 { gnu.usage_error(f"'{option}' extra characters or invalid number in the argument") }
  let digits = if numeric { spec } else { spec[1..] }
  let width = if digits == "" { 8 } else { optional_number(digits, option, 1, false) }
  {character: character, width: width}
}

proc controls(data: Bytes, caret: Bool) -> Bytes {
  let chunks: List[Bytes] = collect {
    for at in range(data.len()) {
      let byte = data.byte_at(at) ?? 0
      if byte == 10 or byte == 12 { yield data[at..at + 1] } else if caret and (byte < 32 or byte == 127) { yield bytes.concat([b"^", bytes.from_ints([if byte == 127 { 63 } else { byte + 64 }])?]) } else if byte < 32 or byte >= 127 { yield bytes.from_text(f"\\{byte / 64}{byte / 8 % 8}{byte % 8}") } else { yield data[at..at + 1] }
    }
  }
  bytes.concat(chunks)
}

pure print_records(data: Bytes, omit_pages: Bool) -> List[Bytes] {
  var start = 0
  collect {
    for at in range(data.len()) {
      let byte = data.byte_at(at) ?? 0
      if byte == 10 {
        if at > start or data.byte_at(at - 1) != 12 { yield data[start..at] }
        start = at + 1
      } else if byte == 12 {
        yield data[start..at] when at > start
        yield b"\x0c" when ! omit_pages
        start = at + 1
      }
    }
    yield data[start..] when start < data.len()
  }
}

pure display_columns(data: Bytes, start = 0) -> Int {
  var column = start
  var at = 0
  while at < data.len() {
    let unit = text.character(data, at)
    let byte = data.byte_at(at) ?? 0
    if byte == 9 { column += 8 - column % 8 } else if byte == 8 { if column > 0 { column -= 1 } } else { column += unit.width }
    at += unit.size
  }
  column - start
}

# Cells of a multi-column page are padded by byte count, with each tab counted as
# eight columns, so that the separator after a padded cell lands on the next
# column start.
pure cell_width(data: Bytes) -> Int {
  var tabs = 0
  for at in range(data.len()) {
    if data.byte_at(at) == 9 { tabs += 1 }
  }
  data.len() + 7 * tabs
}

pure expansion_overflows(data: Bytes, tab_width: Int, tab_byte: Int) -> Bool {
  var column = 0
  var at = 0
  while at < data.len() {
    let unit = text.character(data, at)
    let byte = data.byte_at(at) ?? 0
    if byte == tab_byte or byte == 9 {
      let width = if byte == 9 and tab_byte != 9 { 8 - column % 8 } else { tab_width - column % tab_width }
      if column > 2147483647 - width { return true }
      column += width
    } else if byte == 10 { column = 0 } else if byte == 8 { if column > 0 { column -= 1 } } else {
      if column > 2147483647 - unit.width { return true }
      column += unit.width
    }
    at += unit.size
  }
  false
}

# A tab counts as one column when clipping, matching cell_width's byte-based
# padding, so a cell holding a tab is cut by character count.
pure clip_columns(data: Bytes, width: Int) -> Bytes {
  var columns = 0
  var at = 0
  while at < data.len() {
    let unit = text.character(data, at)
    let unit_width = if data.byte_at(at) == 9 { 1 } else { unit.width }
    break when columns + unit_width > width
    columns += unit_width; at += unit.size
  }
  data[..at]
}

proc write_padding(count: Int, character = " ") {
  var remaining = count
  let block = text.padding(if count < 65536 { count } else { 65536 }, character)
  while remaining > 0 {
    let take = if remaining < 65536 { remaining } else { 65536 }
    gnu.write_text(if take == 65536 { block } else { text.padding(take, character) })
    remaining -= take
  }
}

proc number_field(value: Int, width: Int, separator: Str) -> Bytes {
  let raw = f"{value}"
  let digits = raw.byte_slice(if raw.byte_len() > width { raw.byte_len() - width } else { 0 })
  bytes.from_text(text.padding(width - digits.byte_len()) + digits + separator)
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(modernize(argv), {
    gnu: {status: 1},
    columns: {form: "--columns N", default: "1"},
    across: {form: "-a --across", default: false},
    back_compat: {form: "-b", default: false},
    merge: {form: "-m --merge", default: false},
    double: {form: "-d --double-space", default: false},
    omit_header: {form: "-t --omit-header", default: false},
    omit_pages: {form: "-T --omit-pagination", default: false},
    formfeed: {form: "-f -F --form-feed", default: false},
    header: {form: "-h --header HEADER"},
    date_format: {form: "-D --date-format FORMAT"},
    length: {form: "-l --length N", default: "66"},
    width: {form: "-w --width N", default: "72"},
    page_width: {form: "-W --page-width N", default: ""},
    offset: {form: "-o --indent N", default: "0"},
    numbers: {form: "-n --number-lines[=SEPWIDTH]", optional_default: "\t5"},
    first_number: {form: "-N --first-line-number N", default: "1"},
    separator: {form: "-s --separator[=CHAR]", optional_default: "\t"},
    separator_string: {form: "-S --sep-string[=STRING]", optional_default: " "},
    expand_tabs: {form: "-e --expand-tabs[=CHARWIDTH]", optional_default: "\t8"},
    output_tabs: {form: "-i --output-tabs[=CHARWIDTH]", optional_default: "\t8"},
    page_range: {form: "--pages FIRST:LAST", default: "1"},
    join: {form: "-J --join-lines", default: false},
    control: {form: "-c --show-control-chars", default: false},
    nonprinting: {form: "-v --show-nonprinting", default: false},
    quiet: {form: "-r --no-file-warnings", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: pr [OPTION]... [FILE]...\nPaginate or columnate files for printing.\n  -a, --across\n  -m, --merge\n  -t, --omit-header\n  -T, --omit-pagination\n  -l, --length=N\n  -w, --width=N\n  -n, --number-lines[=SEPWIDTH]"); return }
  if opts.version { gnu.version("pr"); return }
  let columns = size(opts.columns, "invalid number of columns")
  if opts.merge and columns > 1 { gnu.error("cannot specify number of columns when printing in parallel"); exit 1 }
  if opts.merge and opts.across { gnu.error("cannot specify both printing across and printing in parallel"); exit 1 }
  let length = size(opts.length, "'-l PAGE_LENGTH' invalid number of lines")
  let width = size(if opts.page_width != "" { opts.page_width } else { opts.width }, if opts.page_width != "" { "'-W PAGE_WIDTH' invalid number of characters" } else { "'-w PAGE_WIDTH' invalid number of characters" })
  let offset = size(opts.offset, "'-o MARGIN' invalid line offset", 0)
  let first_number = size(opts.first_number, "'-N NUMBER' invalid starting line number", -2147483648)
  var line_number = first_number
  let pages = opts.page_range.split(":")
  if pages.len() > 2 or pages[0] == "" { page_error(opts.page_range, opts.quiet) }
  let first_page = page_number(pages[0], opts.page_range, opts.quiet)
  let last_page = if pages.len() == 1 or pages[1] == "" { {value: 9223372036854775807, display: "18446744073709551615"} } else { page_number(pages[1], opts.page_range, opts.quiet) }
  if decimal_greater(first_page.display, last_page.display) { page_error(opts.page_range, opts.quiet) }
  let posix_time = (env.get_or("LC_ALL", "") ?? "") == "POSIX" or (env.get_or("LC_TIME", "") ?? "") == "POSIX"
  let posix_format = (env.get_or("POSIXLY_CORRECT", "") ?? "") != "" and posix_time
  let date_format = opts.date_format ?? (if posix_format { "%b %e %H:%M %Y" } else { "%Y-%m-%d %H:%M" })
  let headers = ! opts.omit_header and ! opts.omit_pages and length > 10
  let capacity = if opts.omit_pages { 2147483647 } else { if headers { length - 10 } else if length > 0 { length } else { 66 } }
  let rows_per_page = if opts.double { if capacity / 2 > 0 { capacity / 2 } else { 1 } } else { capacity }
  var number_width = 5
  var number_sep = "\t"
  if let spec = opts.numbers {
    if spec == "" { gnu.usage_error("'-n' extra characters or invalid number in the argument") }
    let numeric = rx"^[0-9]+$".matches(spec)
    if ! numeric {
      if bytes.from_text(spec[..1]).len() != 1 { gnu.usage_error("'-n' extra characters or invalid number in the argument") }
      number_sep = spec[..1]
    }
    let digits = if numeric { spec } else { spec[1..] }
    if digits != "" { number_width = optional_number(digits, "-n") }
  }
  let expansion = if let spec = opts.expand_tabs { tab_option(spec, "-e") } else { {character: 9, width: 8} }
  let output_tab = if let spec = opts.output_tabs { tab_option(spec, "-i") } else { {character: 9, width: 8} }
  let separator = if opts.join { "" } else { opts.separator_string ?? opts.separator ?? "\t" }
  let names = if opts.paths.is_empty() { ["-"] } else { opts.paths }
  var inputs: List[List[Bytes]] = []
  var labels: List[Str] = []
  var timestamps: List[Int] = []
  var failed = false
  for name in names {
    guard let data = gnu.read_operand(name) else { |failure|
      if ! opts.quiet { gnu.name_error(name, failure) }
      failed = true; continue
    }
    let expand_input = opts.expand_tabs != null
    if expand_input and expansion_overflows(data, expansion.width, expansion.character) { gnu.error("integer overflow"); exit 1 }
    let expanded = if expand_input { text.expand(data, {stops: [], interval: expansion.width, relative: false}, tab_byte: expansion.character) } else { data }
    let printable = if opts.control or opts.nonprinting { controls(expanded, opts.control) } else { expanded }
    inputs += [print_records(printable, opts.omit_pages)]
    labels += [if name == "-" { "" } else { name }]
    timestamps += [if name == "-" { time.now() * 1000000 } else { fs.stat(fp"{name}", true)?.mtime_ns }]
  }
  var groups = inputs.len()
  if opts.merge { groups = if inputs.is_empty() { 0 } else { 1 } }
  for batch in range(groups) {
    let count = if opts.merge { inputs.len() } else { columns }
    let column_width = if count > 1 { (width - count + 1) / count } else { width }
    if count > 1 and column_width <= 0 { gnu.usage_error("page width too narrow") }
    let lines = if opts.merge { [] } else { inputs[batch] }
    var total = lines.len()
    if opts.merge { for input in inputs { if input.len() > total { total = input.len() } } }
    let per_page = if opts.merge { rows_per_page } else { rows_per_page * count }
    var at = 0
    var page = 1
    while at < total {
      var take = if total - at < per_page { total - at } else { per_page }
      var forced = false
      if ! opts.merge {
        for index in range(at, at + take) {
          if lines[index] == b"\x0c" { take = index - at; forced = true; break }
        }
      }
      if ! opts.merge and at + take < total and lines[at + take] == b"\x0c" { forced = true }
      let row_count = if opts.merge { per_page } else { (take + count - 1) / count }
      let shown = page >= first_page.value and page <= last_page.value
      if shown and headers {
        let date = time.format(if opts.merge { time.now() * 1000000 } else { timestamps[batch] }, format: date_format)?
        let label = opts.header ?? (if opts.merge { "" } else { labels[batch] })
        let page_text = f"Page {page}"
        let spare = width - date.byte_len() - label.byte_len() - page_text.byte_len()
        let left = if spare > 0 { spare / 2 } else { 1 }
        let right = if spare > 0 { spare - left } else { 1 }
        gnu.write_text("\n\n")
        gnu.write_text(date + text.padding(left) + label + text.padding(right) + page_text + "\n\n\n")
      }
      for row in range(row_count) {
        var column_start = offset
        var last_column = -1
        for column in range(count) {
          let index = if opts.merge { at + row } else if opts.across { at + row * count + column } else { at + column * row_count + row }
          if (opts.merge and index < inputs[column].len()) or (! opts.merge and index < at + take) { last_column = column }
        }
        if opts.merge { last_column = count - 1 }
        var chunks: List[Bytes] = []
        for column in range(count) {
          let index = if opts.merge { at + row } else if opts.across { at + row * count + column } else { at + column * row_count + row }
          if column > last_column { break }
          let present = if opts.merge { index < inputs[column].len() } else { index < at + take }
          let line = if ! present { b"" } else if opts.merge { inputs[column][index] } else { lines[index] }
          if column > 0 {
            chunks += [bytes.from_text(separator), bytes.from_text(text.padding(offset))]
            column_start += bytes.from_text(separator).len() + offset
          }
          let prefix = if opts.numbers != null and (! opts.merge or column == 0) and present {
            number_field(if opts.merge { line_number } else { first_number + index }, number_width, number_sep)
          } else { b"" }
          let prefix_width = display_columns(prefix)
          let bounded = ! opts.join and (count > 1 or opts.page_width != "")
          let body = if bounded { clip_columns(line, column_width - prefix_width) } else { line }
          let field = bytes.concat([prefix, body])
          chunks += [field]
          let field_width = display_columns(field, column_start)
          let pad = if count > 1 and ! opts.join { column_width - cell_width(field) } else { 0 }
          chunks += [bytes.from_text(text.padding(pad))]
          column_start += field_width + (if pad > 0 { pad } else { 0 })
          if opts.merge and column == 0 { line_number += 1 }
        }
        if shown {
          write_padding(offset)
          let raw = bytes.concat(chunks)
          if opts.output_tabs != null {
            let compressed = text.unexpand(raw, {stops: [], interval: output_tab.width, relative: false}, true, column_start: offset)
            let remapped: List[Bytes] = collect {
              for at in range(compressed.len()) {
                yield if compressed.byte_at(at) == 9 { bytes.from_ints([output_tab.character])? } else { compressed[at..at + 1] }
              }
            }
            gnu.write_bytes(bytes.concat(remapped))
          } else { gnu.write_bytes(raw) }
          gnu.write_text("\n")
          if opts.double { gnu.write_text("\n") }
        }
      }
      if shown and ! opts.omit_pages and (headers or opts.formfeed) {
        if opts.formfeed { if forced and row_count == 0 { gnu.write_text("\n") }; gnu.write_text("\u{c}") } else { write_padding(length - 5 - row_count * (if opts.double { 2 } else { 1 }), "\n") }
      }
      at += take + (if forced { 1 } else { 0 }); page += 1
    }
    let page_count = if page > 1 { page - 1 } else { 1 }
    if first_page.value > page_count { gnu.error(f"starting page number {first_page.display} exceeds page count {page_count}") }
  }
  exit text.finish(failed)
}
