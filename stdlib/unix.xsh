##! Embedded implementation of the public `unix` module.
#
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# `/proc/uptime` reading policy behind `unix.uptime_seconds` lives here.
#
# Raw acquisition stays native for every other entry. This one reads host text
# when the call is made and reports a read failure with its own kind and the
# operating system's message.
#
# The source path is fixed: `/proc/uptime`. No environment variable, parameter,
# or other switch changes which file is read.
# The error kinds this entry reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spelling
# `unix-uptime` visible to callers.
error UnixTextError = Failure(kind: Str, message: Str)

# The integer text the baseline's `parse::<i64>` accepts, or null.
#
# The accepted spelling is an optional leading `+` or `-`, then one or more
# ASCII digits, and nothing else; leading zeros are allowed, and a value outside
# `Int` range fails, exactly as the baseline's parse fails. `Str.parse_int`
# accepts more than that (white space, radix prefixes, `_` separators) and
# rejects the smallest `Int` spelling `-9223372036854775808`, because it reads
# the magnitude before it applies the sign, so the digits are accumulated here
# under the bounds of `Int` instead. The accumulation cannot trap: a digit is
# only multiplied once the value is known to be in range.
pure parse_field_int(text: Str) -> Int? {
  let end = text.byte_len()
  if end == 0 {
    return null
  }

  # 43 is `+` and 45 is `-`.
  let first = text.byte_at(0) ?? 0
  var start = 0
  var negative = false
  if first == 43 or first == 45 {
    start = 1
    negative = first == 45
  }

  if start == end {
    return null
  }

  var value = 0
  var index = start
  while index < end {
    let byte = text.byte_at(index) ?? 0
    if byte < 48 or byte > 57 {
      return null
    }

    let digit = byte - 48

    # 922337203685477580 is `Int` max without its last digit, so any value above
    # it is out of range on the next step.
    if value > 922337203685477580 {
      return null
    }

    if value == 922337203685477580 {
      if digit == 8 and negative and index + 1 == end {
        # The magnitude of `Int` min is one past `Int` max, so it is reported
        # here rather than built by the multiplication below.
        return 0 - 9223372036854775807 - 1
      }

      if digit > 7 {
        return null
      }
    }

    value = value * 10 + digit
    index = index + 1
  }

  if negative {
    return 0 - value
  }

  return value
}

# Interpret `/proc/uptime` text as whole seconds.
#
# The first white-space separated field is the uptime in seconds with a
# fractional part; only the part before the first `.` is read, and text that is
# not an integer there reads as zero. Empty text, a leading fraction, and a
# value outside `Int` range all read as zero as well.
pure uptime_from_text(text: Str) -> Int {
  let fields = text.words()
  if fields.len() == 0 {
    return 0
  }

  return parse_field_int(fields[0].split(".", 1)[0]) ?? 0
}

## Report the host uptime in whole seconds.
##
## `/proc/uptime` carries the uptime in seconds with a fractional part in its
## first field; the part before the first `.` is read, and text that is not an
## integer there — empty text, a fraction with no whole part, or a value
## outside `Int` range — reads as zero. A read failure is reported with
## `unix-uptime` and the operating system's message.
export proc uptime_seconds() [fs, error] -> Result[Int] {
  match p"/proc/uptime".read_text() {
    Ok(text) => return Ok(uptime_from_text(text))
    Err(failure) => return Err(UnixTextError.Failure(kind: "unix-uptime", message: failure.message))
  }
}
