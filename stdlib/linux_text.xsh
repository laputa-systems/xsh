##! Embedded implementation of the public `linux` text-backed entries.
#
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# `/proc` text interpretation, the dry-run gate, the canned dry-run values, and
# the dry-run log line live here.
#
# A script-backed entry replaces the native body, and with it the dispatch that
# carried the gate, the canned values, and the log append, so this module
# reproduces all three. Without `XSH_LINUX_DRY_RUN` or `XSH_LINUX_REAL` every
# entry fails with `linux-unimplemented` before any file is opened. With
# `XSH_LINUX_DRY_RUN` set, each entry reports its canned value and appends one
# JSON line to `XSH_LINUX_DRY_RUN_LOG` when that variable is set — staying in
# dry-run even when `XSH_LINUX_REAL` is also set. Only `XSH_LINUX_REAL` without
# `XSH_LINUX_DRY_RUN` reads host text.
#
# Host text is read when the call is made, and a read failure is the call's
# `Err`, exactly as the native entries report it. The paths are fixed:
# `linux.meminfo` reads `/proc/meminfo` and `linux.modules` reads
# `/proc/modules`. No environment variable, parameter, or other switch changes
# which file is read.
#
# The malformed-line failure of the stream entry is the one place where this
# port cannot reproduce the baseline's timing. The baseline reports it from the
# stream's `next`, so a consumer that stops early never reaches it; the
# producer below is written to interpret each retained line as it is consumed,
# but the runtime evaluates a producer body when the producer is called, so the
# failure surfaces at the call. The records, their order, and the failure's
# kind and message are the same either way.

# The error kind the entries below report.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spellings
# `linux-unimplemented`, `linux-meminfo`, `linux-modules`, and
# `linux-dry-run-log` visible to callers.
error LinuxTextError = Failure(kind: Str, message: Str)

## Append bytes to a file in place, creating the file and its missing parents.
##
## Host-mechanism bridge: lowering replaces every call with the private
## operation, which performs the baseline's create/open/append/write. Nothing in
## XSH can append without reading the file first, and the baseline appends to
## the open file, so a log that holds bytes which are not valid UTF-8 has to
## survive an append here too. The body below is unreachable and raises if it is
## ever reached.
export proc append_bytes(target: Path, payload: Bytes) [fs, error] -> Result[Unit] {
  return [Ok()][1]
}

# Whether `text` spells one of the accepted true values.
#
# The baseline compares the raw value without trimming or case folding, so only
# these four spellings open a gate.
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
pure unimplemented() -> LinuxTextError {
  return LinuxTextError.Failure(
    kind: "linux-unimplemented",
    message: "linux.* boot primitives require XSH_LINUX_DRY_RUN=1 or XSH_LINUX_REAL=1",
  )
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
      return Err(LinuxTextError.Failure(kind: "linux-dry-run-log", message: failure.message))
    }
  }
}

# Append one line to `target`, creating missing parent directories first.
#
# The line is composed here and appended through the private operation, which
# performs the baseline's own create/open/append/write: the prior bytes are
# never read, so an existing log that is not valid UTF-8 keeps its bytes, an
# unreadable file is an append error rather than an empty prior file, and two
# appenders cannot drop each other's lines.
proc append_line(target: Path, line: Str) [fs, error] -> Result[Unit] {
  let payload = bytes.from_text(line + "\n")
  return append_bytes(target, payload)
}

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

# The failure `/proc/meminfo` reports for a value it must carry.
pure missing_meminfo(name: Str) -> LinuxTextError {
  return LinuxTextError.Failure(
    kind: "linux-meminfo",
    message: f"missing `${name}` in /proc/meminfo",
  )
}

# The failure `/proc/meminfo` reports for a value it cannot read.
pure invalid_meminfo(name: Str) -> LinuxTextError {
  return LinuxTextError.Failure(
    kind: "linux-meminfo",
    message: f"invalid numeric value for `${name}` in /proc/meminfo",
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
# the baseline's order — `MemTotal`, `MemFree`, `MemAvailable`, `Buffers`,
# `Cached`, `SwapTotal`, `SwapFree` — so a missing key is reported only once
# every line has been accepted.
pure parse_meminfo(text: Str) -> Result[LinuxMemInfo] {
  var total: Int? = null
  var free: Int? = null
  var available: Int? = null
  var buffers: Int? = null
  var cached: Int? = null
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
      return Err(invalid_meminfo(key))
    }
    let byte_count = kilobytes_to_bytes(value ?? 0)
    if key == "MemTotal" {
      total = byte_count
    } else if key == "MemFree" {
      free = byte_count
    } else if key == "MemAvailable" {
      available = byte_count
    } else if key == "Buffers" {
      buffers = byte_count
    } else if key == "Cached" {
      cached = byte_count
    } else if key == "SwapTotal" {
      swap_total = byte_count
    } else if key == "SwapFree" {
      swap_free = byte_count
    }
  }
  if total == null {
    return Err(missing_meminfo("MemTotal"))
  }
  if free == null {
    return Err(missing_meminfo("MemFree"))
  }
  if available == null {
    return Err(missing_meminfo("MemAvailable"))
  }
  if buffers == null {
    return Err(missing_meminfo("Buffers"))
  }
  if cached == null {
    return Err(missing_meminfo("Cached"))
  }
  if swap_total == null {
    return Err(missing_meminfo("SwapTotal"))
  }
  if swap_free == null {
    return Err(missing_meminfo("SwapFree"))
  }
  return Ok({
    total: total ?? 0,
    free: free ?? 0,
    available: available ?? 0,
    buffers: buffers ?? 0,
    cached: cached ?? 0,
    swap_total: swap_total ?? 0,
    swap_free: swap_free ?? 0,
  })
}

# The failure `/proc/modules` reports for a line it cannot read.
pure malformed_modules_line() -> LinuxTextError {
  return LinuxTextError.Failure(kind: "linux-modules", message: "malformed line in /proc/modules")
}

# The failure `/proc/modules` reports for a field that is not there.
pure missing_module_field(name: Str) -> LinuxTextError {
  return LinuxTextError.Failure(
    kind: "linux-modules",
    message: f"missing ${name} in /proc/modules",
  )
}

# The failure `/proc/modules` reports for a field it cannot read.
pure invalid_module_field(name: Str, value: Str) -> LinuxTextError {
  return LinuxTextError.Failure(
    kind: "linux-modules",
    message: f"invalid ${name} `${value}` in /proc/modules",
  )
}

# Drop trailing commas from a used-by field.
#
# The baseline trims every trailing `,` before splitting, so `a,,` and `a` have
# the same list.
pure trim_used_by(text: Str) -> Str {
  var end = text.byte_len()
  # 44 is `,`.
  while end > 0 and text.byte_at(end - 1, 0) == 44 {
    end = end - 1
  }
  return text.byte_slice(0, end)
}

# Split a used-by field into module names.
#
# Empty fields and the `-` placeholder that marks an unused module are dropped,
# and the remaining names keep their order.
pure split_used_by(text: Str) -> List[Str] {
  return [
    item
    for item in trim_used_by(text).split(",", -1)
    if item != "" and item != "-"
  ]
}

# Interpret one `/proc/modules` line.
#
# The baseline consumes the fields in order and reports the first one that is
# wrong, so the shape of a short line decides its failure: a line with a name
# but no size is a missing size, a line with a name and size but no use count
# is a missing use count, and a line without a used-by field — or without the
# two fields after it, the module state and address — is a malformed line.
# Fields after the address are ignored, and only the name, size, and used-by
# list are reported.
pure parse_module_line(line: Str) -> Result[LinuxModule] {
  let fields = line.words()
  if fields.len() == 0 {
    return Err(malformed_modules_line())
  }
  if fields.len() < 2 {
    return Err(missing_module_field("size"))
  }
  let size = parse_field_int(fields[1])
  if size == null {
    return Err(invalid_module_field("size", fields[1]))
  }
  if fields.len() < 3 {
    return Err(missing_module_field("use count"))
  }
  let use_count = parse_field_int(fields[2])
  if use_count == null {
    return Err(invalid_module_field("use count", fields[2]))
  }
  if fields.len() < 6 {
    return Err(malformed_modules_line())
  }
  return Ok({name: fields[0], size: size ?? 0, used_by: split_used_by(fields[3])})
}

# The lines of `/proc/modules` that carry content, in file order.
#
# A line that is empty or only white space is dropped before it is
# interpreted, so it cannot fail the parse.
pure retained_lines(text: Str) -> List[Str] {
  return [line for line in text.lines() if line.trim() != ""]
}

# The records of `lines`, one per line, interpreted as they are consumed.
#
# A line that is not a module line fails the producer at the point it is
# reached; a consumer that stops before that point never sees the failure.
stream module_records(lines: List[Str]) [error] -> Stream[LinuxModule] {
  for line in lines {
    let parsed = parse_module_line(line)?
    yield parsed
  }
}

# The line the dry-run canned record is interpreted from.
#
# The record's fields are the ones the baseline reports in dry-run: the name
# `xsh_demo`, the size `4096`, and the single used-by entry `xsh_dep`. Reading
# it through the same interpreter keeps one definition of a module record.
pure dry_run_module_line() -> Str {
  return "xsh_demo 4096 1 xsh_dep, Live 0x0000000000000000"
}

## Report the memory values of `/proc/meminfo` in bytes.
##
## Every line with a `key: value unit` shape whose unit is `kB` contributes its
## value scaled to bytes, later lines replace earlier ones, and a value that is
## not an integer fails the call with `linux-meminfo`. The record carries
## `total`, `free`, `available`, `buffers`, `cached`, `swap_total`, and
## `swap_free`, each of which must be present in the text; the first missing
## key in that order is reported with `linux-meminfo` and a message naming it.
##
## A read failure of the text is reported with `linux-meminfo`. While neither
## `XSH_LINUX_DRY_RUN` nor `XSH_LINUX_REAL` is set the call fails with
## `linux-unimplemented` without opening any file, and while
## `XSH_LINUX_DRY_RUN` is set it reports fixed values and appends one line to
## the `XSH_LINUX_DRY_RUN_LOG` file when that variable names one.
export proc meminfo() [env, fs, error] -> Result[LinuxMemInfo] {
  if !linux_dry_run() and !linux_real() {
    return Err(unimplemented())
  }
  if linux_dry_run() {
    dry_run_log("XSH_LINUX_DRY_RUN_LOG", "meminfo")?
    return Ok({
      total: 1024 * 1024 * 1024,
      free: 256 * 1024 * 1024,
      available: 512 * 1024 * 1024,
      buffers: 64 * 1024 * 1024,
      cached: 128 * 1024 * 1024,
      swap_total: 512 * 1024 * 1024,
      swap_free: 384 * 1024 * 1024,
    })
  }
  match fs.read_text(p"/proc/meminfo") {
    Ok(text) => {
      return parse_meminfo(text)
    }
    Err(failure) => {
      return Err(LinuxTextError.Failure(kind: "linux-meminfo", message: failure.message))
    }
  }
}

## Report the loaded kernel modules of `/proc/modules` as a stream of records.
##
## The text is read, and blank lines are dropped, when the call is made; a read
## failure is reported with `linux-modules`. Each remaining line is interpreted
## as the stream is consumed and yields `{name, size, used_by}`, where the size
## is the line's size field read as an integer exactly as the text spells it
## (`/proc/modules` reports it in bytes) and `used_by` lists the modules that use
## this one, without its trailing comma and without the `-` placeholder. A line
## that is not a module line fails the stream with `linux-modules`, naming the
## field that is missing or invalid, or reporting the line as malformed.
##
## While neither `XSH_LINUX_DRY_RUN` nor `XSH_LINUX_REAL` is set the call fails
## with `linux-unimplemented` without opening any file, and while
## `XSH_LINUX_DRY_RUN` is set it yields one fixed record and appends one line
## to the `XSH_LINUX_DRY_RUN_LOG` file when that variable names one.
export proc modules() [env, fs, error] -> Result[Stream[LinuxModule]] {
  if !linux_dry_run() and !linux_real() {
    return Err(unimplemented())
  }
  if linux_dry_run() {
    dry_run_log("XSH_LINUX_DRY_RUN_LOG", "modules")?
    return Ok(module_records([dry_run_module_line()]))
  }
  match fs.read_text(p"/proc/modules") {
    Ok(text) => {
      return Ok(module_records(retained_lines(text)))
    }
    Err(failure) => {
      return Err(LinuxTextError.Failure(kind: "linux-modules", message: failure.message))
    }
  }
}
