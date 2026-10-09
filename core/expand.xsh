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

type Config = {stops: List[Int], interval: Int}

pure stop_after(config: Config, column: Int) -> Int {
  for stop in config.stops {
    return stop when stop > column
  }

  let last = config.stops.get(config.stops.len() - 1) ?? 8
  let step = if config.interval > 0 { config.interval } else { last }
  column - column % step + step
}

pure parse_tabs(values: List[Str]) -> Config {
  var stops: List[Int] = []

  for value in values {
    for item in value.replace(",", " ").split(" ") {
      let number = item.trim().parse_int() ?? 0

      if number > 0 and (stops.len() == 0 or number > (stops.get(stops.len() - 1) ?? 0)) {
        stops += [number]
      }
    }
  }

  if stops.len() == 0 {
    return {stops: [8], interval: 8}
  }

  let last = stops.get(stops.len() - 1) ?? 8
  let interval = if stops.len() > 1 { last - (stops.get(stops.len() - 2) ?? 0) } else { last }
  {stops: stops, interval: interval}
}

pure expand_bytes(data: Bytes, config: Config, initial: Bool) -> Bytes {
  var out: List[Bytes] = []
  var column = 0
  var leading = true

  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0

    if byte == 9 {
      if ! initial or leading {
        let next = stop_after(config, column)

        for _ in range(next - column) {
          out += [b" "]
        }

        column = next
      } else {
        out += [b"\t"]
        column = stop_after(config, column)
      }
    } else {
      out += [data[index..index + 1]]

      if byte == 10 or byte == 12 {
        column = 0
        leading = true
      } else if byte != 32 and byte != 13 {
        column += 1
        leading = false
      } else {
        column += 1
      }
    }
  }

  bytes.concat(out)
}

type ExpandOptions = {initial: Bool, tabs: List[Str], help: Bool, version: Bool, files: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  var args: List[Str] = []

  for arg in argv {
    if rx"^-[0-9]+$".matches(arg) {
      args += ["--tabs", arg[1..]]
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

  let config = parse_tabs(opts.tabs)
  var failed = false

  for name in if opts.files.len() == 0 { ["-"] } else { opts.files } {
    guard let data = gnu.read_operand(name) else { |failure|
      gnu.cannot_open(name, failure)
      failed = true
      continue
    }

    gnu.write_bytes(expand_bytes(data, config, opts.initial))
  }

  if failed { exit 1 }
}
