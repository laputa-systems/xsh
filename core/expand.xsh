#!/bin/xsh
use lib.gnu

const USAGE = """
Usage: expand [OPTION]... [FILE]...
Convert tabs in each FILE to spaces, writing to standard output.
  -i, --initial       do not convert tabs after non-whitespace
  -t, --tabs=N        have tabs N characters apart
  -t, --tabs=LIST     use comma- or blank-separated tab stops
      --help          display this help and exit
      --version       output version information and exit
"""

type Config = {stops: List[Int], interval: Int, repeat: Bool}

pure stop_after(config: Config, column: Int) -> Int {
  for stop in config.stops {
    return stop when stop > column
  }

  if ! config.repeat { return column }

  let last = config.stops.get(config.stops.len() - 1) ?? 8
  let step = if config.interval > 0 { config.interval } else { last }
  last + (column - last) / step * step + step
}

proc parse_tabs(values: List[Str]) [env] -> Result[Config, Str] {
  var stops: List[Int] = []
  var marked_at = -1
  var mode = ""
  var interval = 0

  for value in values {
    for item in value.replace(",", " ").replace("\t", " ").split(" ") {
      let token = item.trim()
      continue when token == ""
      var digits = ""
      var marker = ""
      var bad = ""

      for char in token {
        if char in ["+", "/"] {
          if digits != "" {
            return Err(f"'{char}' specifier not at start of number: '{char}'")
          }

          marker = char
        } else if char >= "0" and char <= "9" {
          digits = f"{digits}{char}"
        } else {
          bad = f"{bad}{char}"
        }
      }

      if bad != "" {
        return Err(f"tab size contains invalid character(s): '{bad}'")
      }

      continue when digits == ""
      let number = digits.parse_int() ?? -1
      if number < 0 { return Err(f"tab stop is too large '{digits}'") }
      if number == 0 { return Err("tab size cannot be 0") }
      if stops.len() > 0 and number <= (stops.get(stops.len() - 1) ?? 0) {
        return Err("tab sizes must be ascending")
      }
      if marker != "" {
        if marked_at >= 0 { return Err(f"'{marker}' specifier only allowed with the last value") }
        marked_at = stops.len()
        mode = marker
        interval = number
      } else {
        if marked_at >= 0 { return Err(f"'{mode}' specifier only allowed with the last value") }
        stops += [number]
      }
    }
  }

  if stops.len() == 0 {
    return Ok({stops: [8], interval: 8, repeat: true})
  }

  if marked_at == stops.len() {
    let last = stops.get(stops.len() - 1) ?? 0
    let next = last + interval
    if next <= last { return Err("tab stop is too large") }
    stops += [next]
  }

  if stops.len() == 1 {
    let one = stops[0]
    return Ok({stops: stops, interval: one, repeat: true})
  }

  let step = if mode == "/" { interval } else { (stops.get(stops.len() - 1) ?? 0) - (stops.get(stops.len() - 2) ?? 0) }
  Ok({stops: stops, interval: step, repeat: mode == "/" or mode == "+"})
}

pure expand_bytes(data: Bytes, config: Config, initial: Bool) -> Bytes {
  var out: List[Bytes] = []
  var column = 0
  var leading = true
  var index = 0

  while index < data.len() {
    let byte = data.byte_at(index) ?? 0

    if byte == 9 {
      if ! initial or leading {
        let next = stop_after(config, column)

        if next == column {
          out += [b"\t"]
          column += 1
        } else {
          for _ in range(next - column) {
            out += [b" "]
          }

          column = next
        }
      } else {
        out += [b"\t"]
        column = if stop_after(config, column) == column { column + 1 } else { stop_after(config, column) }
      }
      index += 1
    } else {
      var width = 1
      var columns = 1

      if byte >= 194 and byte <= 244 {
        let candidate = if byte < 224 { 2 } else if byte < 240 { 3 } else { 4 }
        if index + candidate <= data.len() and (data[index..index + candidate].utf8() ?? "") != "" {
          width = candidate
          let ch = data[index..index + candidate].utf8() ?? ""
          columns = if rx"[\x{300}-\x{36f}\x{200b}-\x{200f}\x{fe00}-\x{fe0f}]".matches(ch) { 0 } else if rx"[\x{1100}-\x{115f}\x{2e80}-\x{a4cf}\xac00-\xd7a3\xf900-\xfaff\xfe10-\xfe6f\xff01-\xff60\x{1f300}-\x{1faff}\x{20000}-\x{3fffd}]".matches(ch) { 2 } else { 1 }
        }
      }

      out += [data[index..index + width]]

      if byte == 10 or byte == 12 {
        column = 0
        leading = true
      } else if byte != 32 and byte != 13 {
        column += columns
        leading = false
      } else {
        column += 1
      }

      index += width
    }
  }

  bytes.concat(out)
}

type ExpandOptions = {initial: Bool, tabs: List[Str], help: Bool, version: Bool, files: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  var args: List[Str] = []

  for arg in argv {
    if rx"^-[0-9,]+$".matches(arg) {
      args += [f"--tabs={arg[1..]}"]
    } else {
      args += [arg]
    }
  }

  let opts: ExpandOptions = cli.applet(
    args,
    {
      gnu: {status: 1, unsupported: {"-U": "multibyte locale processing is unavailable"}},
      initial: {form: "-i --initial", default: false},
      tabs: {form: "-t --tabs LIST", default: [], repeated: true},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("expand"); return }

  guard let config = parse_tabs(opts.tabs) else { |message|
    gnu.error(message)
    gnu.try_help()
    exit 1
  }
  var failed = false

  for name in if opts.files.len() == 0 { ["-"] } else { opts.files } {
    if name != "-" {
      if let Ok(meta) = fp"{name}".metadata() {
        if meta.mode / 4096 % 16 == 4 {
          gnu.error(f"{gnu.quote_maybe(name)}: Is a directory")
          failed = true
          continue
        }
      }
    }

    guard let data = gnu.read_operand(name) else { |failure|
      gnu.name_error(name, failure)
      failed = true
      continue
    }

    gnu.write_bytes(expand_bytes(data, config, opts.initial))
  }

  if failed { exit 1 }
}
