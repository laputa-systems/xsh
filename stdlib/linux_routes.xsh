##! Embedded implementation of the Linux-only `linux.routes` entry.
# Internal implementation module. The public contract (name, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# read-only route-table interpretation lives here.
#
# Acquisition stays native: the entry reads `/proc/net/route` and
# `/proc/net/ipv6_route` through the ordinary file API at call time, so a
# missing source still contributes no rows and a real read failure is still
# reported with `linux-routes`. Route mutation, link configuration, DHCP
# sockets, and interface ABI handling are not part of this module.
#
# Shape of the interpretation, and why it is written this way.
#
# The two sources keep their own field layouts and are interpreted by separate
# functions; the IPv4 file's first row is a header and is dropped, and the IPv6
# file has no header. A row whose fields do not parse is skipped, not reported:
# the baseline filters malformed rows silently, so a short row, an unreadable
# address, an unusable prefix length, and an out-of-range flag word all
# contribute nothing rather than failing the stream.
#
# The IPv4 destination, gateway, and mask are hexadecimal words exactly as the
# kernel writes them, and the baseline renders them by reinterpreting the
# parsed word through `u32::from_le`, which is the identity on every supported
# target. The rendering below is therefore the parsed word as a dotted quad,
# and the prefix length is the mask's population count. That is deliberately
# not the rendering the IPv6 path uses, and neither is standardized against the
# open-file socket records.
#
# The IPv6 address is thirty-two hexadecimal characters read as sixteen bytes
# and then rendered in compressed form: eight groups of four hexadecimal
# digits, the longest run of two or more zero groups replaced by `::` (the
# first such run when several are equally long), no leading zeros in a group,
# and lowercase digits. A single zero group is written as `0`.
#
# An IPv4 metric is a decimal integer and defaults to zero when it does not
# parse, while an IPv6 metric and both flag words are hexadecimal and default to
# zero the same way; an IPv4 flag word that does not parse skips the row
# instead, because the baseline requires it. Both flag words are read as
# unsigned sixteen-bit values, so a word that does not fit sixteen bits is
# unusable.
#
# The gate, the dry-run record, and the log line repeat what `linux_text.xsh`
# defines for its own entries. Embedded implementation modules do not reach
# each other's private helpers, and duplicating the few small predicates is the
# price of that seal; the shared helper is the public API, not this module.

# The error kinds this entry reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spellings
# `linux-unimplemented`, `linux-routes`, and `linux-dry-run-log` visible.
error LinuxRouteError = Failure(kind: Str, message: Str)

# The rows of the two route sources, kept apart because their layouts differ.
type RouteRows = {ipv4: List[Str], ipv6: List[Str]}

# Whether `text` spells one of the accepted true values.
pure is_flag(text: Str) -> Bool {
  return text == "1" or text == "true" or text == "yes" or text == "on"
}

# Whether the dry-run gate is open.
proc linux_dry_run() [env] -> Bool {
  return is_flag(env.get("XSH_LINUX_DRY_RUN") ?? "")
}

# Whether the real gate is open.
proc linux_real() [env] -> Bool {
  return is_flag(env.get("XSH_LINUX_REAL") ?? "")
}

# The failure every entry reports while neither gate is open.
pure unimplemented() -> LinuxRouteError {
  return LinuxRouteError.Failure(
    kind: "linux-unimplemented",
    message: "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
  )
}

# The failure a route source reports for a read it could not complete.
pure routes_failure(message: Str) -> LinuxRouteError {
  return LinuxRouteError.Failure(kind: "linux-routes", message: message)
}

# The failure a row that carries no route reports.
#
# Only the parsers below see this; the stream skips such a row, so the message
# never reaches a caller.
pure skipped_row() -> LinuxRouteError {
  return LinuxRouteError.Failure(kind: "linux-routes", message: "row is not a route line")
}

# Whether a read failure means the source is not there.
#
# The file API reports a reason as text and offers no separate reason to test,
# so the missing-source case is recognized by the spelling the baseline's
# `ErrorKind::NotFound` carries into its message.
pure is_missing(message: Str) -> Bool {
  return message.starts_with("No such file or directory")
}

# Whether the variable `name` names a log file to append to.
#
# Only an unset variable means no log: a value that is empty or not valid UTF-8
# still names a path, which the baseline opens and fails on.
proc log_configured(name: Str) [env] -> Bool {
  match env.get(name) {
    Ok(_) => {
      return true
    }
    Err(failure) => {
      return failure.message != "environment value is unset"
    }
  }
}

# Append the dry-run log line for `op`.
#
# An unset `name` means no log is configured and the append is skipped; a set
# value is a path, including an empty one, which the baseline opens and fails
# on. A failing append is reported with the log's own kind, so callers do not
# see the filesystem kind behind it.
proc dry_run_log(name: Str, op: Str) [env, fs, error] -> Result[Unit] {
  if !log_configured(name) {
    return Ok()
  }
  let log_path = env.path(name, p"")?
  let line = json.encode({op: op})?
  match append_line(log_path, line) {
    Ok(_) => {
      return Ok()
    }
    Err(failure) => {
      return Err(
        LinuxRouteError.Failure(kind: "linux-dry-run-log", message: failure.message),
      )
    }
  }
}

# Append one line to `target`, creating missing parent directories first.
#
# There is no append primitive, so the existing text is re-read and rewritten
# with the new line added. The baseline appends to the open file in place, so a
# file that is not valid UTF-8 is rewritten here instead of appended to.
proc append_line(target: Path, line: Str) [fs, error] -> Result[Unit] {
  let parent = target.parent()
  if parent.display() != "" {
    fs.mkdir(parent, parents: true)?
  }
  let existing = fs.read_text(target) ?? ""
  return fs.write(target, existing + line + "\n")
}

# The value of one hexadecimal digit.
#
# `-1` marks a byte that is not a hexadecimal digit. Uppercase digits count,
# because the baseline's radix parse accepts them.
pure hex_digit(byte: Int) -> Int {
  if byte >= 48 and byte <= 57 {
    return byte - 48
  }
  if byte >= 97 and byte <= 102 {
    return byte - 87
  }
  if byte >= 65 and byte <= 70 {
    return byte - 55
  }
  return 0 - 1
}

# Read `text` as an unsigned hexadecimal integer of at most `limit`.
#
# The accepted spelling is an optional leading `+`, then one or more
# hexadecimal digits, and nothing else; leading zeros count. The bound is
# checked before each multiply, so the accumulation cannot trap, and a value
# above `limit` is rejected exactly as the baseline's fixed-width parse rejects
# it.
pure parse_unsigned_hex(text: Str, limit: Int) -> Result[Int] {
  let end = text.byte_len()
  if end == 0 {
    return Err(skipped_row())
  }
  var start = 0
  # 43 is `+`, the only sign an unsigned radix parse accepts.
  if text.byte_at(0, 0) == 43 {
    start = 1
  }
  if start == end {
    return Err(skipped_row())
  }
  var value = 0
  var index = start
  while index < end {
    let digit = hex_digit(text.byte_at(index, 0))
    if digit < 0 {
      return Err(skipped_row())
    }
    if value > (limit - digit) / 16 {
      return Err(skipped_row())
    }
    value = value * 16 + digit
    index = index + 1
  }
  return Ok(value)
}

# Read `text` as a signed hexadecimal `Int`.
#
# Used for the IPv6 metric, where the baseline parses the field as a
# hexadecimal `Int`; the accepted spelling is an optional `+` or `-`, then one
# or more hexadecimal digits. The magnitude of the smallest `Int` is one past
# the largest, so it is returned directly rather than built by the
# multiplication.
pure parse_signed_hex(text: Str) -> Result[Int] {
  let end = text.byte_len()
  if end == 0 {
    return Err(skipped_row())
  }
  let first = text.byte_at(0, 0)
  var start = 0
  var negative = false
  # 43 is `+` and 45 is `-`.
  if first == 43 or first == 45 {
    start = 1
    negative = first == 45
  }
  if start == end {
    return Err(skipped_row())
  }
  var value = 0
  var index = start
  while index < end {
    let digit = hex_digit(text.byte_at(index, 0))
    if digit < 0 {
      return Err(skipped_row())
    }
    # 576460752303423487 is `Int` max without its last hexadecimal digit, so
    # any value above it is out of range on the next step.
    if value > 576460752303423487 {
      return Err(skipped_row())
    }
    if value == 576460752303423487 {
      if digit == 8 and negative and index + 1 == end {
        return Ok(0 - 9223372036854775807 - 1)
      }
      if digit > 7 {
        return Err(skipped_row())
      }
    }
    value = value * 16 + digit
    index = index + 1
  }
  if negative {
    return Ok(0 - value)
  }
  return Ok(value)
}

# Read `text` as a signed decimal `Int`, or report that it is not one.
#
# The accepted spelling is an optional leading `+` or `-`, then one or more
# ASCII digits, and nothing else; leading zeros count. `Str.parse_int` accepts
# more than that (white space, radix prefixes, `_` separators), so the digits
# are accumulated here under the bounds of `Int` instead.
pure parse_decimal(text: Str) -> Result[Int] {
  let end = text.byte_len()
  if end == 0 {
    return Err(skipped_row())
  }
  let first = text.byte_at(0, 0)
  var start = 0
  var negative = false
  if first == 43 or first == 45 {
    start = 1
    negative = first == 45
  }
  if start == end {
    return Err(skipped_row())
  }
  var value = 0
  var index = start
  while index < end {
    let byte = text.byte_at(index, 0)
    if byte < 48 or byte > 57 {
      return Err(skipped_row())
    }
    let digit = byte - 48
    if value > 922337203685477580 {
      return Err(skipped_row())
    }
    if value == 922337203685477580 {
      if digit == 8 and negative and index + 1 == end {
        return Ok(0 - 9223372036854775807 - 1)
      }
      if digit > 7 {
        return Err(skipped_row())
      }
    }
    value = value * 10 + digit
    index = index + 1
  }
  if negative {
    return Ok(0 - value)
  }
  return Ok(value)
}

# The number of set bits in `value`.
#
# The value is a non-negative word, so halving it shifts one bit out at a time
# and the low bit says whether that bit counts.
pure bit_count(value: Int) -> Int {
  var remaining = value
  var count = 0
  while remaining != 0 {
    count = count + remaining.bit_and(1)
    remaining = remaining / 2
  }
  return count
}

# Render a hexadecimal word as a dotted quad.
pure render_ipv4(text: Str) -> Result[Str] {
  let raw = parse_unsigned_hex(text, 4294967295)?
  let first = (raw / 16777216) % 256
  let second = (raw / 65536) % 256
  let third = (raw / 256) % 256
  let fourth = raw % 256
  return Ok(f"${first}.${second}.${third}.${fourth}")
}

# Render one hexadecimal group without leading zeros.
pure group_text(group: Int) -> Str {
  return f"${group}"
}

# The eight sixteen-bit groups of a thirty-two character hexadecimal address.
pure ipv6_groups(text: Str) -> Result[List[Int]] {
  if text.byte_len() != 32 {
    return Err(skipped_row())
  }
  var groups: List[Int] = []
  var index = 0
  while index < 8 {
    let high = hex_digit(text.byte_at(index * 4, 0))
    let mid_high = hex_digit(text.byte_at(index * 4 + 1, 0))
    let mid_low = hex_digit(text.byte_at(index * 4 + 2, 0))
    let low = hex_digit(text.byte_at(index * 4 + 3, 0))
    if high < 0 or mid_high < 0 or mid_low < 0 or low < 0 {
      return Err(skipped_row())
    }
    groups = groups.push(
      high * 4096 + mid_high * 256 + mid_low * 16 + low,
    )
    index = index + 1
  }
  return Ok(groups)
}

# The first and longest run of zero groups, if there is one to compress.
#
# A run of one zero group is not compressed, so the search requires at least
# two; among equally long runs the first wins, which is what the baseline's
# renderer does.
pure zero_run_start(groups: List[Int]) -> Int? {
  var best_start: Int? = null
  var best_length = 0
  var index = 0
  while index < groups.len() {
    if groups[index] != 0 {
      index = index + 1
      continue
    }
    let start = index
    while index < groups.len() and groups[index] == 0 {
      index = index + 1
    }
    let length = index - start
    if length >= 2 and length > best_length {
      best_start = start
      best_length = length
    }
  }
  return best_start
}

# The groups of `groups` before `position`, rendered as one colon-separated run.
pure groups_before(groups: List[Int], position: Int) -> Str {
  return [
    group_text(groups[index])
    for index in range(0, position)
  ].join(":")
}

# The groups of `groups` from `position` on, rendered as one colon-separated run.
pure groups_from(groups: List[Int], position: Int) -> Str {
  return [
    group_text(groups[index])
    for index in range(position, groups.len())
  ].join(":")
}

# Render a thirty-two character hexadecimal address in compressed form.
pure render_ipv6(text: Str) -> Result[Str] {
  let groups = ipv6_groups(text)?
  let zeroes = zero_run_start(groups)
  if zeroes == null {
    return Ok(groups_before(groups, groups.len()))
  }
  let start = zeroes ?? 0
  var end = start
  while end < groups.len() and groups[end] == 0 {
    end = end + 1
  }
  return Ok(groups_before(groups, start) + "::" + groups_from(groups, end))
}

# The destination text of a route record.
pure route_destination(address: Str, prefix_len: Int) -> Str {
  if prefix_len == 0 {
    return "default"
  }
  return f"${address}/${prefix_len}"
}

# The flag names of a sixteen-bit route flag word, in the baseline's order.
pure route_flags(flags: Int) -> List[Str] {
  var names: List[Str] = []
  if flags.bit_and(1) != 0 {
    names = names.push("UP")
  }
  if flags.bit_and(2) != 0 {
    names = names.push("GATEWAY")
  }
  if flags.bit_and(4) != 0 {
    names = names.push("HOST")
  }
  if flags.bit_and(16) != 0 {
    names = names.push("REJECT")
  }
  return names
}

# Interpret one `/proc/net/route` row.
#
# The row must carry at least eleven fields. The first field is the device, the
# second the destination word, the third the gateway word, the fourth the flag
# word, the seventh the metric, and the eighth the mask. A row whose
# destination, gateway, mask, or flag word cannot be read carries no route.
pure parse_ipv4_route_line(line: Str) -> Result[LinuxRoute] {
  let fields = line.words()
  if fields.len() < 11 {
    return Err(skipped_row())
  }
  let destination = render_ipv4(fields[1])?
  let gateway = render_ipv4(fields[2])?
  let flags = parse_unsigned_hex(fields[3], 65535)?
  let metric = parse_decimal(fields[6]) ?? 0
  let mask = parse_unsigned_hex(fields[7], 4294967295)?
  let prefix_len = bit_count(mask)
  return Ok({
    family: "inet",
    dst: route_destination(destination, prefix_len),
    prefix_len: prefix_len,
    gateway: gateway,
    dev: fields[0],
    metric: metric,
    flags: route_flags(flags),
  })
}

# Interpret one `/proc/net/ipv6_route` row.
#
# The row must carry at least ten fields. The first field is the destination
# address, the second the prefix length in hexadecimal, the fifth the gateway
# address, the sixth the metric, the ninth the flag word, and the tenth the
# device. A row whose destination or gateway address cannot be read, or whose
# prefix length does not fit one byte, carries no route; the metric and the
# flag word default to zero when they do not parse.
pure parse_ipv6_route_line(line: Str) -> Result[LinuxRoute] {
  let fields = line.words()
  if fields.len() < 10 {
    return Err(skipped_row())
  }
  let destination = render_ipv6(fields[0])?
  let prefix_len = parse_unsigned_hex(fields[1], 255)?
  let gateway = render_ipv6(fields[4])?
  let metric = parse_signed_hex(fields[5]) ?? 0
  let flags = parse_unsigned_hex(fields[8], 65535) ?? 0
  return Ok({
    family: "inet6",
    dst: route_destination(destination, prefix_len),
    prefix_len: prefix_len,
    gateway: gateway,
    dev: fields[9],
    metric: metric,
    flags: route_flags(flags),
  })
}

# The IPv4 rows of a source, without its header line.
pure ipv4_route_rows(text: Str) -> List[Str] {
  let lines = text.lines()
  if lines.len() == 0 {
    return []
  }
  return [lines[index] for index in range(1, lines.len())]
}

# The rows a source contributes, or the failure its read reported.
#
# A source that is not there contributes nothing; any other read failure is
# reported with `linux-routes`.
pure source_rows(outcome: Result[Str], ipv4: Bool) -> Result[List[Str]] {
  match outcome {
    Ok(text) => {
      if ipv4 {
        return Ok(ipv4_route_rows(text))
      }
      return Ok(text.lines())
    }
    Err(failure) => {
      if is_missing(failure.message) {
        return Ok([])
      }
      return Err(routes_failure(failure.message))
    }
  }
}

# The rows of both route sources, read when the call is made.
#
# The two sources are kept apart because their rows have different layouts.
proc route_rows() [fs, error] -> Result[RouteRows] {
  let ipv4 = source_rows(fs.read_text(p"/proc/net/route"), true)?
  let ipv6 = source_rows(fs.read_text(p"/proc/net/ipv6_route"), false)?
  return Ok({ipv4: ipv4, ipv6: ipv6})
}

# The recorded row the dry-run gate yields.
#
# The record's fields are the ones the baseline reports in dry-run: the default
# IPv4 route through `192.0.2.1` on `eth0`, with metric 100 and the `UP` and
# `GATEWAY` flags.
pure dry_run_route() -> LinuxRoute {
  return {
    family: "inet",
    dst: "default",
    prefix_len: 0,
    gateway: "192.0.2.1",
    dev: "eth0",
    metric: 100,
    flags: ["UP", "GATEWAY"],
  }
}

# The one row the dry-run gate yields, as a stream.
stream dry_run_records() -> Stream[LinuxRoute] {
  yield dry_run_route()
}

# The records of both sources, interpreted as they are consumed.
#
# Every row carries the source it came from, so a row is interpreted by its own
# layout; a row that carries no route is skipped where it is reached, and a
# consumer that stops early never interprets the rows after it.
stream route_records(ipv4: List[Str], ipv6: List[Str]) -> Stream[LinuxRoute] {
  for line in ipv4 {
    match parse_ipv4_route_line(line) {
      Ok(route) => {
        yield route
      }
      Err(_) => {}
    }
  }
  for line in ipv6 {
    match parse_ipv6_route_line(line) {
      Ok(route) => {
        yield route
      }
      Err(_) => {}
    }
  }
}

## Report the kernel's routing table as a stream of records.
##
## `/proc/net/route` and `/proc/net/ipv6_route` are read when the call is made,
## and every remaining row is interpreted as the stream is consumed. A source
## that is not there contributes no records; any other read failure is reported
## with `linux-routes`. A row that does not carry a route is skipped rather than
## reported, and every row of both sources is produced in file order, IPv4 rows
## first.
##
## An IPv4 record reports family `inet`, the destination as `default` or
## `address/prefix`, the mask's population count as `prefix_len`, the gateway,
## the device, the metric, and the flag names the flag word carries, in the
## order `UP`, `GATEWAY`, `HOST`, `REJECT`. An IPv6 record reports family
## `inet6` and the same fields, with both addresses rendered in compressed
## form.
##
## While neither `XSH_LINUX_DRY_RUN` nor `XSH_LINUX_REAL` is set the call fails
## with `linux-unimplemented` without opening either source, and while
## `XSH_LINUX_DRY_RUN` is set it yields one fixed record and appends one line
## to the `XSH_LINUX_DRY_RUN_LOG` file when that variable names one.
export proc routes() [env, fs, error] -> Result[Stream[LinuxRoute]] {
  if !linux_dry_run() and !linux_real() {
    return Err(unimplemented())
  }
  if linux_dry_run() {
    dry_run_log("XSH_LINUX_DRY_RUN_LOG", "routes")?
    return Ok(dry_run_records())
  }
  match route_rows() {
    Ok(rows) => {
      return Ok(route_records(rows.ipv4, rows.ipv6))
    }
    Err(failure) => {
      return Err(failure)
    }
  }
}
