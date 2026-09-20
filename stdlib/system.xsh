##! Embedded implementation of the public `system` module.
#
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# Linux text policy behind `system.memory` and `system.os_release` lives here.
#
# Raw acquisition stays native: `system.hostname` and `system.uname` keep their
# runtime bodies. Each entry below reads one host text file, reports a read
# failure with its own kind and the operating system's message, and interprets
# the text the way the baseline did.
#
# The source paths are fixed: `system.memory` reads `/proc/meminfo`, and
# `system.os_release` reads `/etc/os-release` and falls back to
# `/usr/lib/os-release`. No environment variable, parameter, or other switch
# changes which file is read.

# The error kinds the entries below report.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spellings
# `system-memory` and `system-os-release` visible to callers.
error SystemTextError = Failure(kind: Str, message: Str)

# Saturating `value * 1024`.
#
# The baseline scales with `saturating_mul`, so a value whose byte count does
# not fit reports the nearest `Int` bound instead of failing. `9007199254740991`
# is the largest kilobytes count whose byte count still fits, and
# `0 - 9007199254740992` is the smallest; both bounds are exact, so the
# multiplication below cannot trap.
pure kilobytes_to_bytes(value: Int) -> Int {
  if value > 9007199254740991 {
    return 9223372036854775807
  }
  if value < 0 - 9007199254740992 {
    return 0 - 9223372036854775807 - 1
  }
  return value * 1024
}

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
  let first = text.byte_at(0, 0)
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
    let byte = text.byte_at(index, 0)
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

# The failure `/proc/meminfo` reports for a value it cannot read.
pure invalid_memory_value(name: Str) -> SystemTextError {
  return SystemTextError.Failure(
    kind: "system-memory",
    message: f"invalid numeric value for `${name}` in /proc/meminfo",
  )
}

# The failure `/proc/meminfo` reports for a value it must carry.
pure missing_memory_value(name: Str) -> SystemTextError {
  return SystemTextError.Failure(
    kind: "system-memory",
    message: f"missing `${name}` in /proc/meminfo",
  )
}

# Interpret `/proc/meminfo` text.
#
# A line contributes when it has a `key: value unit` shape with two or more
# white-space separated fields after the colon and a unit of exactly `kB`; the
# value is scaled to bytes. A value that is not an integer fails the whole
# call, even for a key this entry does not report, and the first such line in
# file order decides the failure. A later line for the same key replaces an
# earlier one. After the whole text is read, every required key is looked up in
# the baseline's order — `MemTotal`, `MemAvailable`, `MemFree`, `SwapTotal`,
# `SwapFree` — so a missing key is reported only once every line has been
# accepted.
pure parse_memory(text: Str) -> Result[SystemMemory] {
  var total: Int? = null
  var available: Int? = null
  var free: Int? = null
  var swap_total: Int? = null
  var swap_free: Int? = null
  for line in text.lines() {
    let parts = line.split(":", 1)
    if parts.len() < 2 {
      continue
    }
    let fields = parts[1].words()
    if fields.len() < 2 or fields[1] != "kB" {
      continue
    }
    let key = parts[0]
    let value = parse_field_int(fields[0])
    if value == null {
      return Err(invalid_memory_value(key))
    }
    let byte_count = kilobytes_to_bytes(value ?? 0)
    if key == "MemTotal" {
      total = byte_count
    } else if key == "MemAvailable" {
      available = byte_count
    } else if key == "MemFree" {
      free = byte_count
    } else if key == "SwapTotal" {
      swap_total = byte_count
    } else if key == "SwapFree" {
      swap_free = byte_count
    }
  }
  if total == null {
    return Err(missing_memory_value("MemTotal"))
  }
  if available == null {
    return Err(missing_memory_value("MemAvailable"))
  }
  if free == null {
    return Err(missing_memory_value("MemFree"))
  }
  if swap_total == null {
    return Err(missing_memory_value("SwapTotal"))
  }
  if swap_free == null {
    return Err(missing_memory_value("SwapFree"))
  }
  return Ok({
    total: total ?? 0,
    available: available ?? 0,
    free: free ?? 0,
    swap_total: swap_total ?? 0,
    swap_free: swap_free ?? 0,
  })
}

# The byte width of the character that starts at `index`.
#
# Text is always valid UTF-8, so the leading byte decides. The widths are used
# only to keep `Str.byte_slice` on character boundaries.
pure scalar_width(byte: Int) -> Int {
  if byte < 128 {
    return 1
  }
  if byte >= 240 {
    return 4
  }
  if byte >= 224 {
    return 3
  }
  return 2
}

# Remove one layer of quotes from a raw release value.
#
# The value is trimmed first. It is unquoted only when it is at least two bytes
# long and both ends carry the same quote character, `"` or `'`. Inside the
# quotes a backslash escapes the character after it, which is kept as written,
# and a backslash with nothing after it is dropped. Anything else is kept
# exactly as written, quotes included. The scan steps by whole characters, so a
# value with non-ASCII text stays valid UTF-8.
pure unquote_value(raw: Str) -> Str {
  let value = raw.trim()
  let end = value.byte_len()
  if end < 2 {
    return value
  }
  # 34 is `"` and 39 is `'`.
  let first = value.byte_at(0, 0)
  let last = value.byte_at(end - 1, 0)
  if first != last or (first != 34 and first != 39) {
    return value
  }
  var result = ""
  var escaped = false
  var index = 1
  while index < end - 1 {
    let byte = value.byte_at(index, 0)
    let width = scalar_width(byte)
    # 92 is `\`.
    if escaped {
      escaped = false
    } else if byte == 92 and width == 1 {
      escaped = true
      index = index + 1
      continue
    }
    result = result + value.byte_slice(index, width)
    index = index + width
  }
  return result
}

# The value of `name` in release text, or null when no line carries it.
#
# A line contributes when it is not empty after trimming and does not start
# with `#`, and when it contains `=`. The key is the text before the first `=`
# and is not trimmed; the value is the text after it, unquoted. A later line
# for the same key replaces an earlier one, so the last match wins.
pure release_value(text: Str, name: Str) -> Str? {
  var found: Str? = null
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == "" or trimmed.starts_with("#") {
      continue
    }
    let parts = trimmed.split("=", 1)
    if parts.len() < 2 {
      continue
    }
    if parts[0] == name {
      found = unquote_value(parts[1])
    }
  }
  return found
}

# Interpret release text into the reported record.
#
# `NAME` defaults to `Linux`, `ID` to `linux`, and `VERSION` and `VERSION_ID`
# to the empty string. `PRETTY_NAME` defaults to the resolved name rather than
# to the raw `NAME` value, so a text without `PRETTY_NAME` reports the same
# default name in both fields.
pure os_release_record(text: Str) -> SystemOsRelease {
  let name = release_value(text, "NAME") ?? "Linux"
  let pretty_name = release_value(text, "PRETTY_NAME") ?? name
  return {
    name: name,
    pretty_name: pretty_name,
    version: release_value(text, "VERSION") ?? "",
    version_id: release_value(text, "VERSION_ID") ?? "",
    id: release_value(text, "ID") ?? "linux",
  }
}

# Read release text, falling back to `second` when `first` cannot be read.
#
# Every failure of the first read is followed by the second, and the failure of
# the second is the one that is reported, exactly as the baseline's `or_else`
# reports it.
proc read_release_text(first: Path, second: Path) [fs] -> Result[Str] {
  match fs.read_text(first) {
    Ok(text) => {
      return Ok(text)
    }
    Err(_) => {
      return fs.read_text(second)
    }
  }
}

## Report the host memory values in bytes.
##
## `/proc/meminfo` lines with a `key: value unit` shape whose unit is `kB`
## contribute their value scaled to bytes, later lines replace earlier ones,
## and a value that is not an integer fails the call with `system-memory`. The
## record carries `total`, `available`, `free`, `swap_total`, and `swap_free`,
## each of which must be present in the text; the first missing key in that
## order is reported with `system-memory` and a message naming it. A read
## failure is reported with `system-memory` as well.
export proc memory() [env, fs, error] -> Result[SystemMemory] {
  match fs.read_text(p"/proc/meminfo") {
    Ok(text) => {
      return parse_memory(text)
    }
    Err(failure) => {
      return Err(SystemTextError.Failure(kind: "system-memory", message: failure.message))
    }
  }
}

## Report the release identification of the host.
##
## `/etc/os-release` is read first and `/usr/lib/os-release` is read when the
## first file cannot be read at all; a failure of both reads is reported with
## `system-os-release` and the message of the second one. Lines that are empty
## after trimming, start with `#`, or carry no `=` are ignored; the key is the
## text before the first `=` and the value is the text after it, with one layer
## of matching `"` or `'` quotes removed and backslash escapes resolved. The
## last line for a key wins.
##
## `NAME` defaults to `Linux`, `ID` to `linux`, and `VERSION` and `VERSION_ID`
## to the empty string; `PRETTY_NAME` defaults to the resolved name.
export proc os_release() [env, fs, error] -> Result[SystemOsRelease] {
  match read_release_text(p"/etc/os-release", p"/usr/lib/os-release") {
    Ok(text) => {
      return Ok(os_release_record(text))
    }
    Err(failure) => {
      return Err(SystemTextError.Failure(kind: "system-os-release", message: failure.message))
    }
  }
}
