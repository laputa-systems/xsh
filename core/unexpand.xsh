#!/bin/xsh
use lib.gnu

const USAGE = """
Usage: unexpand [OPTION]... [FILE]...
Convert blanks in each FILE to tabs, writing to standard output.
  -a, --all          convert all blanks, not just leading blanks
  -f, --first-only   convert only leading blanks (default)
  -t, --tabs=N       have tabs N characters apart
      --help         display this help and exit
      --version      output version information and exit
"""
const MAX_TABSTOP = 9223372036854775807

type Config = {stops: List[Int], interval: Int, repeat: Bool, mode: Str}

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
      var bad_started = false
      var misplaced = ""
      var misplaced_marker = ""

      for char in token {
        if misplaced_marker != "" {
          misplaced = f"{misplaced}{char}"
        } else if bad_started {
          bad = f"{bad}{char}"
        } else if char in ["+", "/"] {
          if digits != "" {
            misplaced_marker = char
            misplaced = char
          } else {
            marker = char
          }
        } else if char >= "0" and char <= "9" {
          digits = f"{digits}{char}"
        } else {
          bad = f"{bad}{char}"
          bad_started = true
        }
      }

      if misplaced_marker != "" {
        return Err(f"'{misplaced_marker}' specifier not at start of number: '{misplaced}'")
      }
      if bad != "" { return Err(f"tab size contains invalid character(s): '{bad}'") }
      continue when digits == ""
      let number = digits.parse_int() ?? -1
      if number < 0 { return Err("tab stop is too large") }
      if number == 0 and (marker == "" or stops.len() == 0) { return Err("tab size cannot be 0") }
      if marker == "" and stops.len() > 0 and number <= (stops.get(stops.len() - 1) ?? 0) {
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
    let step = if interval > 0 { interval } else { 8 }
    return Ok({stops: [step], interval: step, repeat: true, mode: mode})
  }

  if marked_at == stops.len() {
    if mode == "+" and interval > 0 {
      let last = stops.get(stops.len() - 1) ?? 0
      if interval > MAX_TABSTOP - last { return Err("tab stop is too large") }
      stops += [last + interval]
    }
  }

  if stops.len() == 1 {
    let one = stops[0]
    let step = if interval > 0 { interval } else { one }
    return Ok({stops: stops, interval: step, repeat: true, mode: mode})
  }

  let step = if mode == "/" { interval } else { (stops.get(stops.len() - 1) ?? 0) - (stops.get(stops.len() - 2) ?? 0) }
  Ok({stops: stops, interval: step, repeat: mode == "/" or mode == "+", mode: mode})
}

pure stop_after(config: Config, column: Int) -> Int {
  for stop in config.stops {
    return stop when stop > column
  }

  if ! config.repeat { return column }

  let last = config.stops.get(config.stops.len() - 1) ?? 8
  let step = if config.interval > 0 { config.interval } else { last }
  return column - column % step + step when config.mode == "/"

  last + (column - last) / step * step + step
}

pure tab_advance(config: Config, column: Int) -> Int {
  let next = stop_after(config, column)
  return next when next > column
  column - column % 8 + 8
}

type UnitWidth = {size: Int, columns: Int, wide_blank: Bool}

pure unit_width(data: Bytes, at: Int) -> UnitWidth {
  let lead = data.byte_at(at) ?? 0
  let size = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  let valid = size > 1 and at + size <= data.len() and (data[at..at + size].utf8() ?? "") != ""
  let count = if valid { size } else { 1 }
  let text = data[at..at + count].utf8() ?? ""
  let wide_blank = text == "　"
  let zero = rx"[\x{300}-\x{36f}\x{200b}-\x{200f}\x{fe00}-\x{fe0f}]".matches(text)
  let wide = rx"[\x{1100}-\x{115f}\x{2e80}-\x{a4cf}\x{ac00}-\x{d7a3}\x{f900}-\x{faff}\x{fe10}-\x{fe6f}\x{ff01}-\x{ff60}\x{1f300}-\x{1faff}\x{20000}-\x{3fffd}]".matches(text)
  {size: count, columns: if zero { 0 } else if wide { 2 } else { 1 }, wide_blank: wide_blank}
}

type BlankUnit = {data: Bytes, tab: Bool, width: Int}

pure unexpand_bytes(data: Bytes, config: Config, all: Bool) -> Bytes {
  var out: List[Bytes] = []
  var at = 0
  var column = 0
  var leading = true

  while at < data.len() {
    let byte = data.byte_at(at) ?? 0

    if byte == 10 or byte == 12 {
      out += [data[at..at + 1]]
      at += 1
      column = 0
      leading = true
      continue
    }

    let unit = unit_width(data, at)
    let is_tab = byte == 9
    let is_blank = byte == 32 or is_tab or unit.wide_blank

    if ! is_blank {
      out += [data[at..at + unit.size]]
      at += unit.size
      column += unit.columns
      leading = false
      continue
    }

    var end = at
    var units: List[BlankUnit] = []
    var scan_column = column
    var contains_tab = false

    while end < data.len() {
      let next_byte = data.byte_at(end) ?? 0
      let next_unit = unit_width(data, end)
      let tab = next_byte == 9
      break when next_byte != 32 and ! tab and ! next_unit.wide_blank
      let size = if tab or next_byte == 32 { 1 } else { next_unit.size }
      let width = if tab { tab_advance(config, scan_column) - scan_column } else if next_byte == 32 { 1 } else { next_unit.columns }
      units += [{data: data[end..end + size], tab: tab, width: width}]
      contains_tab = contains_tab or tab
      scan_column += width
      end += size
    }

    if all or leading or contains_tab {
      var item = 0
      while item < units.len() {
        let next = stop_after(config, column)
        var width = 0
        var probe_column = column
        var after = item

        if next > column {
          while after < units.len() and probe_column < next {
            let current = units[after]
            let advance = if current.tab { tab_advance(config, probe_column) - probe_column } else { current.width }
            if probe_column + advance > next { break }
            width += advance
            probe_column += advance
            after += 1
          }
        }

        if next > column and probe_column == next and after > item {
          out += [b"\t"]
          column = next
          item = after
        } else {
          out += [units[item].data]
          column += if units[item].tab { tab_advance(config, column) - column } else { units[item].width }
          item += 1
        }
      }
      at = end
    } else {
      out += [data[at..end]]
      column = scan_column
      at = end
    }
  }

  bytes.concat(out)
}

type UnexpandOptions = {all: Bool, first_only: Bool, tabs: List[Str], help: Bool, version: Bool, files: List[Str]}

pure raw_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var index = 0
  var after_separator = false

  while index < argv.len() {
    let arg = argv[index]

    if ! after_separator and arg == "--" {
      after_separator = true
      index += 1
      continue
    }

    if ! after_separator and (arg == "-t" or arg == "--tabs") {
      index += 2
      continue
    }

    if ! after_separator and arg.starts_with("-") and arg != "-" {
      index += 1
      continue
    }

    files += [raw[index]]
    index += 1
  }

  files
}

proc read_raw(raw: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when raw == b"-"

  let target = Path.parse_bytes(raw)?
  target.read_bytes()
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  var args: List[Str] = []
  var shortcut = false
  var explicit_all = false

  for arg in argv {
    if rx"^-[0-9,]+$".matches(arg) {
      args += [f"--tabs={arg[1..]}"]
      shortcut = true
    } else {
      args += [arg]
      explicit_all = explicit_all or arg == "-a" or arg == "--all"
    }
  }

  if shortcut and ! explicit_all { args += ["--first-only"] }

  let opts: UnexpandOptions = cli.applet(
    args,
    {
      gnu: {status: 1, unsupported: {"-U": "multibyte locale processing is unavailable"}},
      all: {form: "-a --all", default: false},
      first_only: {form: "-f --first-only", default: false},
      tabs: {form: "-t --tabs LIST", default: [], repeated: true},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("unexpand"); return }

  let config = match parse_tabs(opts.tabs) {
    Ok(value) => value
    Err(message) => {
      gnu.error(message)
      gnu.try_help()
      exit 1
    }
  }
  let all = (opts.all or opts.tabs.len() > 0) and ! opts.first_only
  var failed = false
  let files = raw_files(argv, cli.argv_bytes())

  for name in if files.len() == 0 { [b"-"] } else { files } {
    if name != b"-" {
      if let Ok(target) = Path.parse_bytes(name) {
        if let Ok(meta) = target.metadata() {
          if meta.mode / 4096 % 16 == 4 {
            gnu.error(f"{gnu.quote_bytes(name, always: false)}: Is a directory")
            failed = true
            continue
          }
        }
      }
    }

    guard let data = read_raw(name) else { |failure|
      gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }

    gnu.write_bytes(unexpand_bytes(data, config, all))
  }

  if failed { exit 1 }
}
