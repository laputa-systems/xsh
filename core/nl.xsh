#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """
Usage: nl [OPTION]... [FILE]...
Write each FILE to standard output, with line numbers added.
  -b, --body-numbering=STYLE   use STYLE for body lines
  -d, --section-delimiter=CC   use CC to separate logical pages
  -f, --footer-numbering=STYLE use STYLE for footer lines
  -h, --header-numbering=STYLE use STYLE for header lines
  -i, --line-increment=NUMBER  line number increment
  -l, --join-blank-lines=NUMBER group blank lines
  -n, --number-format=FORMAT   use FORMAT (ln, rn, or rz)
  -p, --no-renumber            do not reset at each section
  -s, --number-separator=STRING separate number and text
  -v, --starting-line-number=NUMBER
  -w, --number-width=NUMBER    use NUMBER columns for numbers
      --help                   display this help and exit
      --version                output version information and exit
"""

type NlOptions = {
  body: List[Str], footer: List[Str], header: List[Str], section: List[Str],
  increment: List[Str], join: List[Str], format: List[Str], no_renumber: Bool,
  separator: List[Str], start: List[Str], width: List[Str], help: Bool,
  version: Bool, files: List[Str],
}
type NlConfig = {body: Str, footer: Str, header: Str, section: Bytes, increment: Int,
  join: Int, format: Str, no_renumber: Bool, separator: Bytes, start: Int, width: Int,
  blank_start: Int}
type NlOutput = {data: Bytes, overflow: Bool, next_number: Int, next_blank: Int}

pure last(values: List[Str], fallback: Str) -> Str {
  if values.len() == 0 { fallback } else { values[values.len() - 1] }
}

pure signed_integer(value: Str) -> Result[Int, Error] {
  if value == "-9223372036854775808" { return Ok(-9223372036854775807 - 1) }
  value.parse_int()
}

pure raw_option(argv: List[Str], raw: List[Bytes], short: Str, long: Str, fallback: Bytes) -> Bytes {
  var value = fallback
  for i in range(argv.len()) {
    let arg = argv[i]
    if arg == long and i + 1 < raw.len() { value = raw[i + 1] } else if arg.starts_with(long + "=") { value = raw[i].slice(long.byte_len() + 1) } else if arg == f"-{short}" and i + 1 < raw.len() { value = raw[i + 1] } else if arg.starts_with(f"-{short}") and arg != f"-{short}" { value = raw[i].slice(2) }
  }
  value
}

pure input_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var i = 0
  var stop = false
  while i < argv.len() {
    let arg = argv[i]
    if ! stop and arg == "--" { stop = true; i += 1 } else if ! stop and arg in ["-b", "--body-numbering", "-d", "--section-delimiter", "-f", "--footer-numbering", "-h", "--header-numbering", "-i", "--line-increment", "-l", "--join-blank-lines", "-n", "--number-format", "-s", "--number-separator", "-v", "--starting-line-number", "-w", "--number-width"] { i += 2 } else if ! stop and arg.starts_with("--") and arg.find("=") != null { i += 1 } else if ! stop and arg.starts_with("-") and arg != "-" { i += 1 } else { files += [raw[i]]; i += 1 }
  }
  files
}

proc read_input(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  Path.parse_bytes(name)?.read_bytes()
}

pure style_matches(style: Str, line: Bytes) -> Bool {
  return true when style == "a"
  return line.len() > 0 when style == "t"
  return false when style == "n"
  return false when ! style.starts_with("p")
  if let Ok(expression) = regex.compile(style[1..]) { expression.matches(line.utf8() ?? "") } else { false }
}

pure repeat(text: Str, count: Int) -> Str {
  var out = ""
  for _ in range(count) { out += text }
  out
}

pure left_number(number: Int, width: Int) -> Bytes {
  let value = f"{number}"
  bytes.from_text(value + repeat(" ", if width > value.byte_len() { width - value.byte_len() } else { 0 }))
}

pure number_field(number: Int, width: Int, format: Str) -> Bytes {
  return left_number(number, width) when format == "ln"
  let value = f"{number}"
  let needed = if width > value.byte_len() { width - value.byte_len() } else { 0 }
  let pad = repeat(if format == "rz" { "0" } else { " " }, needed)
  return bytes.from_text(f"-{pad}{value[1..]}") when format == "rz" and number < 0 and needed > 0
  bytes.from_text(pad + value)
}

pure section_kind(line: Bytes, delimiter: Bytes) -> Int {
  return 0 when delimiter.len() == 0
  return 3 when line == bytes.concat([delimiter, delimiter, delimiter])
  return 2 when line == bytes.concat([delimiter, delimiter])
  return 1 when line == delimiter
  0
}

pure format_line(line: Bytes, number: Int, numbered: Bool, config: NlConfig) -> Bytes {
  let field = if numbered { number_field(number, config.width, config.format) } else { bytes.from_text(repeat(" ", config.width)) }
  bytes.concat([field, if numbered { config.separator } else { b" " }, line, b"\n"])
}

proc number_lines(data: Bytes, config: NlConfig) [process, env] -> NlOutput {
  let ends = tio.line_ends(data, false)
  var out: List[Bytes] = []
  var start_at = 0
  var number = config.start
  var section = 2
  var blank_count = config.blank_start
  var overflow_pending = false
  var overflow = false
  for end in ends {
    let line = data[start_at..end - 1]
    start_at = end
    let kind = section_kind(line, config.section)
    if kind > 0 {
      section = kind
      blank_count = 0
      if kind == 2 and ! config.no_renumber { number = config.start; overflow_pending = false }
      blank_count = 0
      out += [b"\n"]
      continue
    }
    let style = if section == 3 { config.header } else if section == 1 { config.footer } else { config.body }
    let blank = line.len() == 0
    var numbered = style_matches(style, line)
    if blank and config.join > 1 {
      blank_count += 1
      numbered = numbered and blank_count % config.join == 0
    } else if ! blank { blank_count = 0 }
    if numbered and overflow_pending { overflow = true; break }
    out += [format_line(line, number, numbered, config)]
    if numbered {
      if (config.increment > 0 and number > 9223372036854775807 - config.increment) or (config.increment < 0 and number < -9223372036854775807 - 1 - config.increment) {
        overflow_pending = true
      } else { number += config.increment }
    }
  }
  if start_at < data.len() {
    let line = data[start_at..]
    let kind = section_kind(line, config.section)
    if kind > 0 { out += [b"\n"] } else {
      let style = if section == 3 { config.header } else if section == 1 { config.footer } else { config.body }
      let numbered = style_matches(style, line)
      if numbered and overflow_pending { overflow = true } else {
        out += [format_line(line, number, numbered, config)]
        if numbered {
          if (config.increment > 0 and number > 9223372036854775807 - config.increment) or (config.increment < 0 and number < -9223372036854775807 - 1 - config.increment) { overflow_pending = true } else { number += config.increment }
        }
      }
    }
  }
  {data: bytes.concat(out), overflow: overflow, next_number: number, next_blank: blank_count}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: NlOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      body: {form: "-b --body-numbering STYLE", default: [], repeated: true},
      section: {form: "-d --section-delimiter CC", default: [], repeated: true},
      footer: {form: "-f --footer-numbering STYLE", default: [], repeated: true},
      header: {form: "-h --header-numbering STYLE", default: [], repeated: true},
      increment: {form: "-i --line-increment NUMBER", default: [], repeated: true},
      join: {form: "-l --join-blank-lines NUMBER", default: [], repeated: true},
      format: {form: "-n --number-format FORMAT", default: [], repeated: true},
      no_renumber: {form: "-p --no-renumber", default: false},
      separator: {form: "-s --number-separator STRING", default: [], repeated: true},
      start: {form: "-v --starting-line-number NUMBER", default: [], repeated: true},
      width: {form: "-w --number-width NUMBER", default: [], repeated: true},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("nl"); return }
  let body = last(opts.body, "t")
  let header = last(opts.header, "n")
  let footer = last(opts.footer, "n")
  let number_format = last(opts.format, "rn")
  if ! (body in ["a", "t", "n"] or body.starts_with("p")) { gnu.usage_error(f"invalid numbering style: {gnu.quote_value(body)}") }
  if ! (header in ["a", "t", "n"] or header.starts_with("p")) { gnu.usage_error(f"invalid numbering style: {gnu.quote_value(header)}") }
  if ! (footer in ["a", "t", "n"] or footer.starts_with("p")) { gnu.usage_error(f"invalid numbering style: {gnu.quote_value(footer)}") }
  if number_format not in ["ln", "rn", "rz"] { gnu.usage_error(f"invalid line numbering format: {gnu.quote_value(number_format)} (invalid value '{number_format}')") }
  for style in [body, header, footer] {
    if style.starts_with("p") {
      if let Err(_) = regex.compile(style[1..]) { gnu.usage_error(f"invalid regular expression: {gnu.quote_value(style[1..])}") }
    }
  }
  let start_text = last(opts.start, "1")
  let inc_text = last(opts.increment, "1")
  let join_text = last(opts.join, "1")
  let width_text = last(opts.width, "6")
  let start_number = match signed_integer(start_text) {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid starting line number: {gnu.quote_value(start_text)} (invalid value '{start_text}')"); exit 1 }
  }
  let increment = match signed_integer(inc_text) {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid line number increment: {gnu.quote_value(inc_text)} (invalid value '{inc_text}')"); exit 1 }
  }
  let joined = match join_text.parse_int() {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid number of blank lines: {gnu.quote_value(join_text)} (invalid value '{join_text}')"); exit 1 }
  }
  let number_width = match width_text.parse_int() {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid line number field width: {gnu.quote_value(width_text)} (invalid value '{width_text}')"); exit 1 }
  }
  if joined < 0 { gnu.usage_error(f"invalid number of blank lines: {gnu.quote_value(join_text)}") }
  if number_width < 1 or number_width > 2147483647 { gnu.usage_error(f"line number field width is not in 1..=2147483647: {gnu.quote_value(width_text)}") }
  let delimiter_text = last(opts.section, "\\:")
  let raw = cli.argv_bytes()
  var section_bytes = raw_option(argv, raw, "d", "--section-delimiter", bytes.from_text(delimiter_text))
  let section_text = section_bytes.utf8()
  let one_character = match section_text { Ok(text) => text.count_chars() == 1, Err(_) => false }
  if section_bytes.len() == 1 or one_character {
    section_bytes = bytes.concat([section_bytes, b":"])
  }
  let separator = last(opts.separator, "\t")
  let number_separator = raw_option(argv, raw, "s", "--number-separator", bytes.from_text(separator))
  var config = {body: body, footer: footer, header: header, section: section_bytes, increment: increment,
    join: joined, format: number_format, no_renumber: opts.no_renumber, separator: number_separator,
    start: start_number, width: number_width, blank_start: 0}
  let names = input_files(argv, cli.argv_bytes())
  var failed = false
  var output: List[Bytes] = []
  for name in if names.len() == 0 { [b"-"] } else { names } {
    if name != b"-" {
      if let Ok(meta) = fs.stat(Path.parse_bytes(name)?, follow_symlinks: true) {
        if meta.kind == "dir" { gnu.error(f"{gnu.quote_bytes(name, always: false)}: Is a directory"); failed = true; continue }
      }
    }
    guard let input = read_input(name) else { |failure|
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    let rendered = number_lines(input, config)
    output += [rendered.data]
    if rendered.overflow { gnu.error("line number overflow"); failed = true }
    config.start = rendered.next_number
    config.blank_start = rendered.next_blank
  }
  gnu.write_bytes(bytes.concat(output))
  if failed { exit 1 }
}
