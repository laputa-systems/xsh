#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {body: Str, header: Str, footer: Str, delimiter: Str, increment: Str, blanks: Str, format: Str, no_reset: Bool, separator: Str, start: Str, width: Str, help: Bool, version: Bool, paths: List[Str]}

# The integer parser rejects the unsigned magnitude of the minimum signed
# value; preserve the full signed range until parse_int accepts it directly.
proc number(value: Str, what: Str, minimum: Int, maximum: Int) -> Int {
  if value == "-9223372036854775808" and minimum == -9223372036854775807 - 1 { return minimum }
  guard let parsed = value.parse_int() else { |_| gnu.usage_error(f"invalid {what}: {gnu.quote_value(value)}"); exit 1 }
  if parsed < minimum or parsed > maximum {
    gnu.usage_error(f"invalid {what}: {gnu.quote_value(value)}")
  }
  parsed
}

proc check_style(style: Str) {
  if style not in ["a", "t", "n"] and ! style.starts_with("p") {
    gnu.usage_error(f"invalid numbering style: {gnu.quote_value(style)}")
  }
  if style.starts_with("p") { if let Err(failure) = regex.compile(style.byte_slice(1)) { gnu.error(f"invalid regular expression: {failure.message}"); exit 1 } }
}

pure formatted(value: Int, width: Int, style: Str) -> Str {
  let digits = f"{value}"
  let count = if width > digits.byte_len() { width - digits.byte_len() } else { 0 }
  if style == "ln" { digits + text.padding(count) } else if style == "rz" and value < 0 { "-" + text.padding(count, "0") + digits.byte_slice(1) } else { text.padding(count, if style == "rz" { "0" } else { " " }) + digits }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    body: {form: "-b --body-numbering STYLE", default: "t"},
    header: {form: "-h --header-numbering STYLE", default: "n"},
    footer: {form: "-f --footer-numbering STYLE", default: "n"},
    delimiter: {form: "-d --section-delimiter CC", default: "\\:"},
    increment: {form: "-i --line-increment N", default: "1"},
    blanks: {form: "-l --join-blank-lines N", default: "1"},
    format: {form: "-n --number-format FORMAT", default: "rn"},
    no_reset: {form: "-p --no-renumber", default: false},
    separator: {form: "-s --number-separator STRING", default: "\t"},
    start: {form: "-v --starting-line-number N", default: "1"},
    width: {form: "-w --number-width N", default: "6"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: nl [OPTION]... [FILE]...\nNumber lines of files or standard input."); return }
  if opts.version { gnu.version("nl"); return }
  check_style(opts.body); check_style(opts.header); check_style(opts.footer)
  if opts.format not in ["ln", "rn", "rz"] { gnu.usage_error(f"invalid line numbering format: {gnu.quote_value(opts.format)}") }
  let width = number(opts.width, "number width", 1, 2147483647)
  let increment = number(opts.increment, "line number increment", -9223372036854775807 - 1, 9223372036854775807)
  let start = number(opts.start, "starting line number", -9223372036854775807 - 1, 9223372036854775807)
  let blanks = number(opts.blanks, "blank line count", 0, 9223372036854775807)
  let delimiter = if opts.delimiter.count_chars() == 1 { opts.delimiter + ":" } else { opts.delimiter }
  var lines: List[Bytes] = []
  var failed = false
  for name in if opts.paths.is_empty() { ["-"] } else { opts.paths } {
    match gnu.read_operand(name) {
      Ok(data) => { lines += text.records(data) }
      Err(failure) => { gnu.name_error(name, failure); failed = true }
    }
  }
  var value = start
  var overflow = false
  var style = opts.body
  var empty = 0
  for line in lines {
    let decoded = line.utf8() ?? ""
    if delimiter != "" and decoded in [delimiter, delimiter + delimiter, delimiter + delimiter + delimiter] {
      style = if decoded == delimiter { opts.footer } else if decoded == delimiter + delimiter { opts.body } else { opts.header }
      if ! opts.no_reset { value = start; overflow = false }
      empty = 0
      gnu.write_text("\n")
      continue
    }
    if line.is_empty() { empty += 1 } else { empty = 0 }
    var numbered = style == "t" and ! line.is_empty()
    if style == "a" { numbered = ! line.is_empty() or (blanks == 0 or empty % blanks == 0) }
    if style.starts_with("p") { numbered = regex.compile(style.byte_slice(1))?.matches(decoded) }
    if numbered {
      if overflow { gnu.error("line number overflow"); exit 1 }
      gnu.write_text(formatted(value, width, opts.format) + opts.separator)
      overflow = (increment > 0 and value > 9223372036854775807 - increment) or (increment < 0 and value < -9223372036854775807 - 1 - increment)
      if ! overflow { value += increment }
    } else { gnu.write_text(text.padding(width + opts.separator.count_chars())) }
    gnu.write_bytes(line)
    gnu.write_text("\n")
  }
  exit text.finish(failed)
}
