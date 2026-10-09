#!/bin/xsh
use lib.gnu

const USAGE = """Usage: sleep NUMBER[SUFFIX]...
  or:  sleep OPTION
Pause for NUMBER seconds, where NUMBER is an integer or floating-point.
SUFFIX may be 's','m','h', or 'd', for seconds, minutes, hours, days.
With multiple arguments, pause for the sum of their values.

      --help        display this help and exit
      --version     output version information and exit
"""

type SleepOptions = {help: Bool, version: Bool, intervals: List[Str]}

type Scanned = {value: Float, rest: Str}

const DECIMAL = rx"^([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?"
const HEXADECIMAL = rx"^0[xX](?:([0-9a-fA-F]+)(?:\.([0-9a-fA-F]*))?|\.([0-9a-fA-F]+))(?:[pP]([+-]?[0-9]+))?"

pure is_space(text: Str) -> Bool {
  text == " " or text == "\t" or text == "\n" or text == "\u{b}" or text == "\u{c}" or text == "\r"
}

# `inf` and `nan` as the parser spells them.
pure special(name: Str) -> Float {
  name.parse_float() ?? 0.0
}

pure hex_digit(text: Str) -> Float {
  let at = "0123456789abcdef".find(text.lower()) ?? 0

  at.float()
}

pure hex_value(whole: Str, fraction: Str, exponent: Int) -> Float {
  var value = 0.0

  for index in range(whole.byte_len()) {
    value = value * 16.0 + hex_digit(whole.byte_slice(index, length: 1))
  }

  var scale = 1.0 / 16.0

  for index in range(fraction.byte_len()) {
    value += hex_digit(fraction.byte_slice(index, length: 1)) * scale
    scale = scale / 16.0
  }

  value * 2.0.pow(exponent.float())
}

# The leading number of TEXT the way `strtod` reads it: blanks, a sign,
# decimal or hexadecimal digits with an optional exponent, `inf`, or `nan`.
# What follows the number is returned unread; no number leaves TEXT as the rest.
pure scan_number(text: Str) -> Scanned {
  var start = 0

  while start < text.byte_len() and is_space(text.byte_slice(start, length: 1)) {
    start += 1
  }

  var body = text.byte_slice(start)
  var negative = false

  if body.starts_with("-") or body.starts_with("+") {
    negative = body.starts_with("-")
    body = body.byte_slice(1)
  }

  let lower = body.lower()
  let sign = if negative { -1.0 } else { 1.0 }

  return {value: sign * special("inf"), rest: body.byte_slice(8)} when lower.starts_with("infinity")
  return {value: sign * special("inf"), rest: body.byte_slice(3)} when lower.starts_with("inf")
  return {value: special("nan"), rest: body.byte_slice(3)} when lower.starts_with("nan")

  let hex = HEXADECIMAL.captures(body)

  if hex.len() > 0 {
    let value = hex_value(hex[1], hex[2] + hex[3], hex[4].parse_int() ?? 0)

    return {value: sign * value, rest: body.byte_slice(hex[0].byte_len())}
  }

  let decimal = DECIMAL.captures(body)

  return {value: 0.0, rest: text} when decimal.len() == 0

  {value: sign * (decimal[0].parse_float() ?? 0.0), rest: body.byte_slice(decimal[0].byte_len())}
}

# The seconds named by one interval, or null when it is not a valid
# non-negative NUMBER with an optional s, m, h, or d suffix.
pure interval_seconds(text: Str) -> Float? {
  let scanned = scan_number(text)

  return null when scanned.rest == text
  return null when ! (scanned.value >= 0.0)
  return scanned.value when scanned.rest == "" or scanned.rest == "s"
  return scanned.value * 60.0 when scanned.rest == "m"
  return scanned.value * 3600.0 when scanned.rest == "h"
  return scanned.value * 86400.0 when scanned.rest == "d"

  null
}

proc main(...argv: List[Str]) [process, env, time, error, io] {
  # Standalone sleep keeps GNU's default signal termination behavior; the
  # interpreter's general handlers exist to clean up shell-owned resources.
  if process.signal_action("TERM")? != "ignore" {
    process.set_signal_action("TERM", "default")?
  }
  if process.signal_action("INT")? != "ignore" {
    process.set_signal_action("INT", "default")?
  }

  let opts: SleepOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      intervals: {form: "...NUMBER"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("sleep")
    return
  }

  if opts.intervals.len() == 0 {
    gnu.missing_operand()
  }

  var total = 0.0
  var valid = true

  for text in opts.intervals {
    let seconds = interval_seconds(text)

    if seconds == null {
      gnu.error(f"invalid time interval {gnu.quote_value(text)}")
      valid = false
    } else {
      total += seconds
    }
  }

  if ! valid {
    gnu.try_help()
    exit 1
  }

  var remaining = total * 1000.0

  while remaining > 0.0 {
    let chunk = if remaining > 3600000.0 { 3600000.0 } else { remaining }

    time.sleep(time.millis(chunk.ceil()?))
    remaining -= chunk
  }
}
