#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {columns: Str, across: Bool, merge: Bool, double: Bool, omit_header: Bool, omit_pages: Bool, formfeed: Bool, header: Str?, date_format: Str, length: Str, width: Str, page_width: Str, offset: Str, numbers: Str?, first_number: Str, separator: Str?, separator_string: Str?, expand_tabs: Str?, output_tabs: Str?, page_range: Str, join: Bool, control: Bool, nonprinting: Bool, quiet: Bool, help: Bool, version: Bool, paths: List[Str]}

proc size(value: Str, what: Str, minimum = 1) -> Int {
  let number = value.parse_int() ?? -1
  if number < minimum or number > 2147483647 { gnu.error(f"invalid {what} argument {gnu.quote_value(value)}"); exit 1 }
  number
}

pure modernize(argv: List[Str]) -> List[Str] {
  var options = true
  var at = 0
  collect {
    while at < argv.len() {
      let item = argv[at]
      if item == "--" { options = false }
      if options and rx"^-[0-9]+$".matches(item) { yield @["--columns", item.byte_slice(1)] } else if options and rx"^\+[0-9]+(:[0-9]+)?$".matches(item) { yield @["--pages", item.byte_slice(1)] } else if options and item in ["-n", "-e", "-i"] and at + 1 < argv.len() and rx"^([0-9]+|[^0-9][0-9]*)$".matches(argv[at + 1]) and ! argv[at + 1].starts_with("-") {
        yield item + argv[at + 1]
        at += 1
      } else { yield item }
      at += 1
    }
  }
}

type TabOption = {character: Int, width: Int}
proc tab_option(spec: Str, option: Str) -> TabOption {
  if spec == "" { gnu.usage_error(f"'{option}' extra characters or invalid number in the argument") }
  let numeric = rx"^[0-9]+$".matches(spec)
  let character = if numeric { 9 } else { bytes.from_text(spec[..1]).byte_at(0) ?? 9 }
  if ! numeric and bytes.from_text(spec[..1]).len() > 1 { gnu.usage_error(f"'{option}' extra characters or invalid number in the argument") }
  let digits = if numeric { spec } else { spec[1..] }
  let width = if digits == "" { 8 } else { size(digits, "tab width") }
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

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(modernize(argv), {
    gnu: {status: 1},
    columns: {form: "--columns N", default: "1"},
    across: {form: "-a --across", default: false},
    merge: {form: "-m --merge", default: false},
    double: {form: "-d --double-space", default: false},
    omit_header: {form: "-t --omit-header", default: false},
    omit_pages: {form: "-T --omit-pagination", default: false},
    formfeed: {form: "-f -F --form-feed", default: false},
    header: {form: "-h --header HEADER"},
    date_format: {form: "-D --date-format FORMAT", default: "%Y-%m-%d %H:%M"},
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
  let columns = size(opts.columns, "--columns")
  if opts.merge and columns > 1 { gnu.usage_error("cannot specify number of columns when printing in parallel") }
  let length = size(opts.length, "--length")
  let width = size(if opts.page_width != "" { opts.page_width } else { opts.width }, if opts.page_width != "" { "--page-width" } else { "--width" })
  let offset = size(opts.offset, "--indent", 0)
  let first_number = size(opts.first_number, "--first-line-number", 0)
  var line_number = first_number
  let pages = opts.page_range.split(":")
  let first_page = size(pages[0], "--pages")
  let last_page = if pages.len() > 1 { size(pages[1], "--pages") } else { 2147483647 }
  if pages.len() > 2 or last_page < first_page { gnu.usage_error("invalid page range") }
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
    if digits != "" { number_width = size(digits, "number width", 0) }
  }
  let expansion = if let spec = opts.expand_tabs { tab_option(spec, "-e") } else { {character: 9, width: 8} }
  let compression = if let spec = opts.output_tabs { tab_option(spec, "-i") } else { {character: 9, width: 8} }
  let separator = opts.separator_string ?? opts.separator ?? "\t"
  let separated = opts.separator != null or opts.separator_string != null
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
    let expanded = if opts.expand_tabs != null or columns > 1 or opts.merge { text.expand(data, {stops: [], interval: expansion.width, relative: false}, tab_byte: expansion.character) } else { data }
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
      let row_count = if opts.merge { take } else { (take + count - 1) / count }
      let shown = page >= first_page and page <= last_page
      if shown and headers {
        let date = time.format(timestamps[batch], format: opts.date_format)?
        let label = opts.header ?? (if opts.merge { "" } else { labels[batch] })
        let page_text = f"Page {page}"
        let spare = width - date.byte_len() - label.byte_len() - page_text.byte_len()
        let left = if spare > 0 { spare / 2 } else { 1 }
        let right = if spare > 0 { spare - left } else { 1 }
        gnu.write_text("\n\n" + text.padding(offset) + date + text.padding(left) + label + text.padding(right) + page_text + "\n\n\n")
      }
      for row in range(row_count) {
        var chunks: List[Bytes] = [bytes.from_text(text.padding(offset))]
        for column in range(count) {
          let index = if opts.merge { at + row } else if opts.across { at + row * count + column } else { at + column * row_count + row }
          if opts.merge and index >= inputs[column].len() { continue }
          if ! opts.merge and index >= at + take { continue }
          let line = if opts.merge { inputs[column][index] } else { lines[index] }
          if column > 0 { chunks += [bytes.from_text(separator)] }
          if opts.numbers != null and (! opts.merge or column == 0) {
            let number = f"{if opts.merge { line_number } else { first_number + index }}"
            chunks += [bytes.from_text(text.padding(number_width - number.byte_len()) + number + number_sep)]
          }
          let body = if ! opts.join and ((count > 1 and ! separated) or opts.page_width != "") { line[..column_width] } else { line }
          chunks += [body]
          if count > 1 and ! opts.join and ! separated { chunks += [bytes.from_text(text.padding(column_width - body.len()))] }
          if opts.merge and column == 0 { line_number += 1 }
        }
        if shown {
          let raw = bytes.concat(chunks)
          if opts.output_tabs != null {
            let compressed = text.unexpand(raw, {stops: [], interval: compression.width, relative: false}, true)
            let remapped: List[Bytes] = collect {
              for at in range(compressed.len()) {
                yield if compressed.byte_at(at) == 9 { bytes.from_ints([compression.character])? } else { compressed[at..at + 1] }
              }
            }
            gnu.write_bytes(bytes.concat(remapped))
          } else { gnu.write_bytes(raw) }
          gnu.write_text("\n")
          if opts.double { gnu.write_text("\n") }
        }
      }
      if shown and ! opts.omit_pages and (headers or opts.formfeed) {
        if opts.formfeed { if forced and row_count == 0 { gnu.write_text("\n") }; gnu.write_text("\u{c}") } else { gnu.write_text(text.padding(length - 5 - row_count * (if opts.double { 2 } else { 1 }), "\n")) }
      }
      at += take + (if forced { 1 } else { 0 }); page += 1
    }
  }
  exit text.finish(failed)
}
