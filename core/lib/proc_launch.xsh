##! Process applet launch diagnostics and duration parsing.
use gnu

## Resolve launch failures before exec so the conventional 126/127 distinction
## remains visible even when the process API returns an untyped host error.
export proc launch_status(command: Str) [process] -> Int {
  match process.which(command) {
    Ok(_) => 0
    Err(is NotFound) => 127
    Err(_) => 126
  }
}

## Fail with the conventional launch status and diagnostic.
export proc check_command(command: Str) [process, env] {
  let status = launch_status(command)
  if status != 0 {
    let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }
    gnu.error(f"failed to run command {gnu.quote(command)}: {reason}")
    exit status
  }
}

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

  if ! hex.is_empty() {
    let value = hex_value(hex[1], hex[2] + hex[3], hex[4].parse_int() ?? 0)

    return {value: sign * value, rest: body.byte_slice(hex[0].byte_len())}
  }

  let decimal = DECIMAL.captures(body)

  return {value: 0.0, rest: text} when decimal.is_empty()

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

## Saturate durations above the host timer range and retain a positive deadline.
export pure interval(text: Str) -> Duration? {
  let seconds = interval_seconds(text)
  return null when seconds == null
  let millis = seconds * 1000.0
  return time.millis(9223372036854) when millis > 9223372036854.0
  # Floating point underflow must not turn a positive mantissa into no timeout.
  let scanned = scan_number(text)
  let numeric = text.byte_slice(0, text.byte_len() - scanned.rest.byte_len())
  let body = if numeric.trim().starts_with("+") { numeric.trim().byte_slice(1) } else { numeric.trim() }
  let hex = HEXADECIMAL.captures(body)
  let mantissa = if hex.is_empty() { body.lower().split("e", maxsplit: 1)[0] } else { hex[1] + hex[2] + hex[3] }
  # Zero is tested with two inequalities because -0.0 is not equal to 0.0 here.
  let zero = millis <= 0.0 and millis >= 0.0
  # A negative mantissa that underflows keeps its sign, so it is not a valid interval.
  return null when zero and body.starts_with("-") and rx"[1-9a-fA-F]".matches(mantissa)
  return 0ms when zero and ! rx"[1-9a-fA-F]".matches(mantissa)
  time.millis(if millis < 1.0 { 1 } else { millis.ceil() ?? 9223372036854 })
}
