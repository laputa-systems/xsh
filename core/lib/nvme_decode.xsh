##! NVMe data structure decoding shared by the nvme and smartctl applets.
##!
##! Structures are described by layout tables (`NAME OFFSET WIDTH STYLE`)
##! taken from the NVMe base specification, so a reader can check every
##! offset against one table and a consumer gets the same decoding of the same
##! bytes. Multi-byte fields are little-endian. The layouts follow the base
##! specification revision 2.0 and name each field as nvme-cli does.

const HEX = "0123456789abcdef"
# The printable ASCII characters from space (32) to tilde (126).
const PRINTABLE = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~"

## One decoded field. `value` is the integer value for fields up to four bytes
## and for eight-byte fields below 2^63; wider fields keep their exact
## magnitude in `decimal` and `hex`. `text` is the string for text-style
## fields and the hexadecimal string for identifier fields.
export type Field = {name: Str, value: Int, decimal: Str, hex: Str, text: Str, style: Str}

## One power state descriptor of the Identify Controller structure. Powers are
## in the unit the matching scale names: `max_scale` 0 is 0.01 W and 1 is
## 0.0001 W; idle and active scales 1 and 2 are 0.0001 W and 0.01 W (0 is not
## reported).
export type PowerState = {
  max_power: Int, max_scale: Int, non_operational: Bool, entry_latency: Int, exit_latency: Int,
  read_throughput: Int, read_latency: Int, write_throughput: Int, write_latency: Int,
  idle_power: Int, idle_scale: Int, active_power: Int, active_scale: Int, workload: Int,
}

## One LBA format descriptor of Identify Namespace.
export type LbaFormat = {metadata_size: Int, data_size_shift: Int, relative_performance: Int}

## One self-test log result entry.
export type SelfTestResult = {
  code: Int, result: Int, segment: Int, valid: Int, power_on_hours: Wide, nsid: Int,
  failing_lba: Wide, status_code_type: Int, status_code: Int, vendor_specific: Int,
}

## A 64- or 128-bit magnitude as exact decimal and minimal hexadecimal text.
export type Wide = {decimal: Str, hex: Str}

## Identify Controller, Identify Namespace, SMART / Health, and Error Log
## entry layouts. Styles: `x` prints as `%#x`, `x4` as `0x%04x`, `d` as a
## decimal, `s` is a space-padded ASCII string, `ieee` the 24-bit OUI,
## `hexstr` a byte string in hexadecimal.
export const CONTROLLER_LAYOUT = [
  "vid 0 2 x4", "ssvid 2 2 x4", "sn 4 20 s", "mn 24 40 s", "fr 64 8 s", "rab 72 1 d",
  "ieee 73 3 ieee", "cmic 76 1 x", "mdts 77 1 d", "cntlid 78 2 x", "ver 80 4 x",
  "rtd3r 84 4 x", "rtd3e 88 4 x", "oaes 92 4 x", "ctratt 96 4 x", "rrls 100 2 x",
  "cntrltype 111 1 d", "fguid 112 16 hexstr", "crdt1 128 2 d", "crdt2 130 2 d",
  "crdt3 132 2 d", "nvmsr 253 1 d", "vwci 254 1 d", "mec 255 1 d", "oacs 256 2 x",
  "acl 258 1 d", "aerl 259 1 d", "frmw 260 1 x", "lpa 261 1 x", "elpe 262 1 d",
  "npss 263 1 d", "avscc 264 1 x", "apsta 265 1 x", "wctemp 266 2 d", "cctemp 268 2 d",
  "mtfa 270 2 d", "hmpre 272 4 d", "hmmin 276 4 d", "tnvmcap 280 16 d", "unvmcap 296 16 d",
  "rpmbs 312 4 x", "edstt 316 2 d", "dsto 318 1 d", "fwug 319 1 d", "kas 320 2 d",
  "hctma 322 2 x", "mntmt 324 2 d", "mxtmt 326 2 d", "sanicap 328 4 x", "hmminds 332 4 d",
  "hmmaxd 336 2 d", "nsetidmax 338 2 d", "endgidmax 340 2 d", "anatt 342 1 d",
  "anacap 343 1 d", "anagrpmax 344 4 d", "nanagrpid 348 4 d", "pels 352 4 d",
  "domainid 356 2 d", "megcap 368 16 d", "sqes 512 1 x", "cqes 513 1 x", "maxcmd 514 2 d",
  "nn 516 4 d", "oncs 520 2 x", "fuses 522 2 x", "fna 524 1 x", "vwc 525 1 x",
  "awun 526 2 d", "awupf 528 2 d", "icsvscc 530 1 d", "nwpc 531 1 d", "acwu 532 2 d",
  "ocfs 534 2 x", "sgls 536 4 x", "mnan 540 4 d", "maxdna 544 16 d", "maxcna 560 4 d",
  "subnqn 768 256 s", "ioccsz 1792 4 d", "iorcsz 1796 4 d", "icdoff 1800 2 d",
  "fcatt 1802 1 d", "msdbd 1803 1 d", "ofcs 1804 2 d",
]

## The Identify Namespace fields before the preferred I/O sizes.
export const NAMESPACE_LAYOUT = [
  "nsze 0 8 x", "ncap 8 8 x", "nuse 16 8 x", "nsfeat 24 1 x", "nlbaf 25 1 d", "flbas 26 1 x",
  "mc 27 1 x", "dpc 28 1 x", "dps 29 1 x", "nmic 30 1 x", "rescap 31 1 x", "fpi 32 1 x",
  "dlfeat 33 1 d", "nawun 34 2 d", "nawupf 36 2 d", "nacwu 38 2 d", "nabsn 40 2 d",
  "nabo 42 2 d", "nabspf 44 2 d", "noiob 46 2 d", "nvmcap 48 16 d",
]
## NPWG through NOWS, meaningful only when NSFEAT bit 4 is set, so they are a
## separate table.
export const NAMESPACE_PREFERRED_IO_LAYOUT = [
  "npwg 64 2 d", "npwa 66 2 d", "npdg 68 2 d", "npda 70 2 d", "nows 72 2 d",
]
## The Identify Namespace fields after the preferred I/O sizes, up to the
## LBA format list.
export const NAMESPACE_TAIL_LAYOUT = [
  "mssrl 74 2 d", "mcl 76 4 d", "msrc 80 1 d", "nulbaf 81 1 d", "anagrpid 92 4 d",
  "nsattr 99 1 d", "nvmsetid 100 2 d", "endgid 102 2 d", "nguid 104 16 hexstr",
  "eui64 120 8 hexstr",
]

## The SMART / Health Information log page (log identifier 2).
export const SMART_LAYOUT = [
  "critical_warning 0 1 d", "temperature 1 2 d", "avail_spare 3 1 d", "spare_thresh 4 1 d",
  "percent_used 5 1 d", "endu_grp_crit_warn_sumry 6 1 d", "data_units_read 32 16 d",
  "data_units_written 48 16 d", "host_read_commands 64 16 d", "host_write_commands 80 16 d",
  "controller_busy_time 96 16 d", "power_cycles 112 16 d", "power_on_hours 128 16 d",
  "unsafe_shutdowns 144 16 d", "media_errors 160 16 d", "num_err_log_entries 176 16 d",
  "warning_temp_time 192 4 d", "critical_comp_time 196 4 d", "temp_sensor1 200 2 d",
  "temp_sensor2 202 2 d", "temp_sensor3 204 2 d", "temp_sensor4 206 2 d",
  "temp_sensor5 208 2 d", "temp_sensor6 210 2 d", "temp_sensor7 212 2 d",
  "temp_sensor8 214 2 d", "thm_temp1_trans_count 216 4 d", "thm_temp2_trans_count 220 4 d",
  "thm_temp1_total_time 224 4 d", "thm_temp2_total_time 228 4 d",
]

## One Error Information log entry.
export const ERROR_ENTRY_LAYOUT = [
  "error_count 0 8 d", "sqid 8 2 d", "cmdid 10 2 x", "status_field 12 2 x",
  "parm_error_location 14 2 x", "lba 16 8 x", "nsid 24 4 x", "vs 28 1 d", "trtype 29 1 d",
  "cs 32 8 x", "trtype_spec_info 40 2 x",
]

## Size in bytes of one Error Information log entry.
export const ERROR_ENTRY_BYTES = 64
## Size in bytes of the SMART / Health Information log page.
export const SMART_LOG_BYTES = 512
## Size in bytes of the Firmware Slot Information log page.
export const FIRMWARE_LOG_BYTES = 512
## Bytes before the result entries of the Device Self-test log.
export const SELF_TEST_HEADER_BYTES = 4
## Size in bytes of one Device Self-test result entry.
export const SELF_TEST_ENTRY_BYTES = 28
## Number of result entries in the Device Self-test log.
export const SELF_TEST_MAX_ENTRIES = 20

## The byte at `offset`, or 0 past the end.
export pure u8(data: Bytes, offset: Int) -> Int {
  data.byte_at(offset) ?? 0
}

## A little-endian unsigned value of up to eight bytes (values at or above
## 2^63 wrap; use `wide` for exact 64- and 128-bit magnitudes).
export pure le(data: Bytes, offset: Int, width: Int) -> Int {
  bytes.unpack_le(data, width, offset) ?? 0
}

## Lowercase hexadecimal digits without a prefix; zero is "0".
export pure hex_digits(value: Int) -> Str {
  var rest = value
  var out = ""
  while rest > 0 {
    out = HEX.byte_slice(rest % 16, length: 1) + out
    rest = rest / 16
  }
  if out == "" { "0" } else { out }
}

pure two_digits(byte: Int) -> Str {
  HEX.byte_slice(byte / 16, length: 1) + HEX.byte_slice(byte % 16, length: 1)
}

## Hexadecimal text of `width` bytes at `offset` in memory order, two digits
## per byte (identifiers such as NGUID and EUI-64).
export pure hex_bytes(data: Bytes, offset: Int, width: Int) -> Str {
  var out = ""
  for index in range(width) {
    out += two_digits(u8(data, offset + index))
  }
  out
}

# Decimal text of a big-endian byte list, exact for any width.
pure decimal_of(digits: List[Int]) -> Str {
  var current = digits
  var chunks: List[Int] = []
  while ! current.is_empty() {
    var quotient: List[Int] = []
    var remainder = 0
    for byte in current {
      let accumulated = remainder * 256 + byte
      quotient += [accumulated / 1000000000]
      remainder = accumulated % 1000000000
    }
    chunks += [remainder]
    var start = 0
    while start < quotient.len() and quotient[start] == 0 {
      start += 1
    }
    current = quotient[start..]
  }
  if chunks.is_empty() { return "0" }
  var out = f"{chunks[chunks.len() - 1]}"
  var index = chunks.len() - 2
  while index >= 0 {
    let text = f"{chunks[index]}"
    out += "000000000".byte_slice(0, length: 9 - text.byte_len()) + text
    index -= 1
  }
  out
}

## Exact decimal and minimal hexadecimal text of a little-endian unsigned
## value of any width.
export pure wide(data: Bytes, offset: Int, width: Int) -> Wide {
  var big_endian: List[Int] = []
  var started = false
  var index = width - 1
  while index >= 0 {
    let byte = u8(data, offset + index)
    if byte != 0 or started {
      big_endian += [byte]
      started = true
    }
    index -= 1
  }
  var digits = ""
  for position in range(big_endian.len()) {
    digits += two_digits(big_endian[position])
  }
  while digits.starts_with("0") and digits.byte_len() > 1 {
    digits = digits.byte_slice(1)
  }
  if big_endian.is_empty() { digits = "0" }
  {decimal: decimal_of(big_endian), hex: digits}
}

## The character for a byte in a hexdump or text field: itself when it is
## printable ASCII (space through tilde), otherwise `.`.
export pure printable_char(byte: Int) -> Str {
  if byte >= 32 and byte < 127 { PRINTABLE.byte_slice(byte - 32, length: 1) } else { "." }
}

## Printable ASCII of a fixed-width text field: bytes outside the printable
## range other than NUL become `.`; the trailing NUL padding is dropped and
## space padding is kept, as the specification pads these fields with spaces.
export pure ascii_text(data: Bytes, offset: Int, width: Int) -> Str {
  var out = ""
  var count = width
  while count > 0 and u8(data, offset + count - 1) == 0 {
    count -= 1
  }
  for index in range(count) {
    let byte = u8(data, offset + index)
    out += if byte >= 32 { printable_char(byte) } else { "." }
  }
  out
}

pure decode_one(data: Bytes, entry: Str) -> Field {
  let parts = entry.split(" ")
  let offset = parts[1].parse_int() ?? 0
  let width = parts[2].parse_int() ?? 0
  let style = parts[3]
  if style == "s" or style == "hexstr" or style == "ieee" {
    let text = if style == "s" { ascii_text(data, offset, width) } else { hex_bytes(data, offset, width) }
    var value = 0
    var shown = text
    if style == "ieee" {
      value = u8(data, offset) + u8(data, offset + 1) * 256 + u8(data, offset + 2) * 65536
      shown = two_digits(u8(data, offset + 2)) + two_digits(u8(data, offset + 1)) + two_digits(u8(data, offset))
    }
    return {name: parts[0], value: value, decimal: f"{value}", hex: hex_digits(value), text: shown, style: style}
  }
  let magnitude = wide(data, offset, width)
  let value = if width <= 4 or (width == 8 and magnitude.decimal.byte_len() < 19) { le(data, offset, width) } else { 0 }
  {name: parts[0], value: value, decimal: magnitude.decimal, hex: magnitude.hex, text: "", style: style}
}

## Decodes the fields of a layout table from a structure.
export pure decode(data: Bytes, layout: List[Str]) -> List[Field] {
  [decode_one(data, entry) for entry in layout]
}

## The field called `name`, or a zero field when the layout has none.
export pure field(fields: List[Field], name: Str) -> Field {
  for item in fields {
    if item.name == name { return item }
  }
  {name: name, value: 0, decimal: "0", hex: "0", text: "", style: "d"}
}

## The integer value of the field called `name`.
export pure field_value(fields: List[Field], name: Str) -> Int {
  field(fields, name).value
}

## A field as nvme-cli prints it in the normal format.
export pure display(item: Field) -> Str {
  match item.style {
    "x4" => "0x" + "0000".byte_slice(0, length: 4 - item.hex.byte_len()) + item.hex
    "x" => if item.hex == "0" { "0" } else { "0x" + item.hex }
    "d" => item.decimal
    else => item.text
  }
}

## The Power State Descriptor entries of an Identify Controller structure:
## `npss` plus one descriptors from byte 2048.
export pure power_states(data: Bytes, count: Int) -> List[PowerState] {
  var out: List[PowerState] = []
  for index in range(count) {
    let base = 2048 + index * 32
    let flags = u8(data, base + 3)
    out += [{
      max_power: le(data, base, 2),
      max_scale: flags % 2,
      non_operational: flags / 2 % 2 == 1,
      entry_latency: le(data, base + 4, 4),
      exit_latency: le(data, base + 8, 4),
      read_throughput: u8(data, base + 12) % 32,
      read_latency: u8(data, base + 13) % 32,
      write_throughput: u8(data, base + 14) % 32,
      write_latency: u8(data, base + 15) % 32,
      idle_power: le(data, base + 16, 2),
      idle_scale: u8(data, base + 18) / 64,
      active_power: le(data, base + 20, 2),
      active_scale: u8(data, base + 22) / 64,
      workload: u8(data, base + 22) % 8,
    }]
  }
  out
}

## The LBA format descriptors of an Identify Namespace structure
## (`nlbaf` and `nulbaf` are zero-based counts).
export pure lba_formats(data: Bytes, count: Int) -> List[LbaFormat] {
  var out: List[LbaFormat] = []
  for index in range(count) {
    let base = 128 + index * 4
    out += [{
      metadata_size: le(data, base, 2),
      data_size_shift: u8(data, base + 2),
      relative_performance: u8(data, base + 3) % 4,
    }]
  }
  out
}

## The index of the LBA format in use: FLBAS bits 3:0 and, with more than
## sixteen formats, bits 6:5 as the high bits.
export pure lba_format_index(flbas: Int) -> Int {
  flbas % 16 + flbas / 32 % 4 * 16
}

## One Error Information log entry (64 bytes at `index`).
export pure error_entry(log: Bytes, index: Int) -> List[Field] {
  decode(log.slice(index * ERROR_ENTRY_BYTES, length: ERROR_ENTRY_BYTES), ERROR_ENTRY_LAYOUT)
}

## The result entries of a Device Self-test log (used entries only: an entry
## whose result is 0xf was never written).
export pure self_test_results(log: Bytes, limit: Int) -> List[SelfTestResult] {
  var out: List[SelfTestResult] = []
  for index in range(limit) {
    let base = SELF_TEST_HEADER_BYTES + index * SELF_TEST_ENTRY_BYTES
    let status = u8(log, base)
    if status % 16 == 15 { continue }
    out += [{
      code: status / 16,
      result: status % 16,
      segment: u8(log, base + 1),
      valid: u8(log, base + 2) % 16,
      power_on_hours: wide(log, base + 4, 8),
      nsid: le(log, base + 12, 4),
      failing_lba: wide(log, base + 16, 8),
      status_code_type: u8(log, base + 24) % 8,
      status_code: u8(log, base + 25),
      vendor_specific: le(log, base + 26, 2),
    }]
  }
  out
}

## The composite temperature in degrees Celsius from a Kelvin value.
export pure celsius(kelvin: Int) -> Int {
  kelvin - 273
}

## `value` with comma thousands separators, as nvme-cli groups the 128-bit
## counters of the SMART log.
export pure group_digits(digits: Str) -> Str {
  var out = ""
  let length = digits.byte_len()
  for index in range(length) {
    if index > 0 and (length - index) % 3 == 0 { out += "," }
    out += digits.byte_slice(index, length: 1)
  }
  out
}

## A count of `unit` byte units in SI prefixes (`1.18 TB`), the figure nvme-cli
## prints after the SMART data unit counters.
export pure si_bytes(decimal: Str, unit: Int) -> Str {
  var amount = (decimal.parse_float() ?? 0.0) * unit.float()
  let prefixes = ["", "k", "M", "G", "T", "P", "E", "Z", "Y"]
  var index = 0
  while amount >= 1000.0 and index < prefixes.len() - 1 {
    amount = amount / 1000.0
    index += 1
  }
  f"{amount.format_number("f", 2) ?? "0.00"} {prefixes[index]}B"
}

## The firmware revision of an eight-byte Firmware Slot Information entry.
export pure firmware_revision(log: Bytes, slot: Int) -> Str {
  ascii_text(log, 8 + (slot - 1) * 8, 8).trim()
}

## The `NVMe status` text for a completion status (the SCT/SC bits; the more
## and do-not-retry bits are ignored).
export pure status_text(status: Int) -> Str {
  let code = status % 256
  let kind = status / 256 % 8
  if kind == 0 {
    return match code {
      0 => "Successful Completion: The command completed without error"
      1 => "Invalid Command Opcode: A reserved coded value or an unsupported value in the command opcode field"
      2 => "Invalid Field in Command: A reserved coded value or an unsupported value in a defined field"
      3 => "Command ID Conflict: The command identifier is already in use"
      4 => "Data Transfer Error: Transferring the data or metadata associated with a command had an error"
      5 => "Commands Aborted due to Power Loss Notification: Indicates that the command was aborted due to a power loss notification"
      6 => "Internal Error: The command was not completed successfully due to an internal error"
      7 => "Command Abort Requested: The command was aborted due to an Abort command being received"
      8 => "Command Aborted due to SQ Deletion: The command was aborted due to a Delete I/O Submission Queue request"
      9 => "Command Aborted due to Failed Fused Command: The command was aborted due to the other command in a fused operation failing"
      10 => "Command Aborted due to Missing Fused Command: The fused command was aborted due to the adjacent command not being submitted"
      11 => "Invalid Namespace or Format: The namespace or the format of that namespace is invalid"
      12 => "Command Sequence Error: The command was aborted due to a protocol violation in a multi-command sequence"
      13 => "Invalid SGL Segment Descriptor: The command includes an invalid SGL Last Segment or SGL Segment descriptor"
      14 => "Invalid Number of SGL Descriptors: There is an SGL Last Segment descriptor or an SGL Segment descriptor in a location other than the last descriptor of a segment"
      15 => "Data SGL Length Invalid: One or more of the SGL Data Block, Keyed SGL Data Block, or Transport SGL Data Block descriptors contain an invalid length"
      16 => "Metadata SGL Length Invalid: One or more of the SGL descriptors for metadata contain an invalid length"
      17 => "SGL Descriptor Type Invalid: The type of an SGL descriptor is a type that is not supported by the controller"
      18 => "Invalid Use of Controller Memory Buffer: The attempt to use the Controller Memory Buffer is not supported"
      19 => "PRP Offset Invalid: The Offset field for a PRP entry is invalid"
      20 => "Atomic Write Unit Exceeded: The length specified exceeds the atomic write unit size"
      21 => "Operation Denied: The command was denied due to lack of access rights"
      22 => "SGL Offset Invalid: The offset specified in a descriptor is invalid"
      24 => "Host Identifier Inconsistent Format: The NVM subsystem detected the simultaneous use of 64-bit and 128-bit Host Identifier values"
      25 => "Keep Alive Timer Expired: The Keep Alive Timer expired"
      26 => "Keep Alive Timeout Invalid: The Keep Alive Timeout value specified is invalid"
      27 => "Command Aborted due to Preempt and Abort: The command was aborted due to a Reservation Acquire command"
      28 => "Sanitize Failed: The most recent sanitize operation failed"
      29 => "Sanitize In Progress: The requested function is prohibited while a sanitize operation is in progress"
      30 => "SGL Data Block Granularity Invalid: The Address alignment or Length granularity for an SGL Data Block descriptor is invalid"
      31 => "Command Not Supported for Queue in CMB: The implementation does not support submission of the command to a Submission Queue in the Controller Memory Buffer"
      32 => "Namespace is Write Protected: The command is prohibited while the namespace is write protected"
      33 => "Command Interrupted: Command processing was interrupted and the controller is unable to successfully complete the command"
      34 => "Transient Transport Error: A transient transport error was detected"
      128 => "LBA Out of Range: The command references an LBA that exceeds the size of the namespace"
      129 => "Capacity Exceeded: Execution of the command has caused the capacity of the namespace to be exceeded"
      130 => "Namespace Not Ready: The namespace is not ready to be accessed"
      131 => "Reservation Conflict: The command was aborted due to a conflict with a reservation held on the accessed namespace"
      132 => "Format In Progress: A Format NVM command is in progress on the namespace"
      else => "Unknown"
    }
  }
  if kind == 1 {
    return match code {
      0 => "Completion Queue Invalid: The Completion Queue identifier specified in the command does not exist"
      1 => "Invalid Queue Identifier: The creation of the I/O Completion Queue failed due to an invalid queue identifier"
      2 => "Invalid Queue Size: The host attempted to create an I/O Completion Queue with an invalid number of entries"
      3 => "Abort Command Limit Exceeded: The number of concurrently outstanding Abort commands has exceeded the limit"
      5 => "Async Event Request Limit Exceeded: The number of concurrently outstanding Asynchronous Event Request commands has been exceeded"
      6 => "Invalid Firmware Slot: The firmware slot indicated is invalid or read only"
      7 => "Invalid Firmware Image: The firmware image specified for activation is invalid and not loaded by the controller"
      8 => "Invalid Interrupt Vector: The creation of the I/O Completion Queue failed due to an invalid interrupt vector"
      9 => "Invalid Log Page: The log page indicated is invalid"
      10 => "Invalid Format: The LBA Format specified is not supported"
      11 => "FW Activation Requires Conventional Reset: The firmware commit was successful, however, activation of the firmware image requires a conventional reset"
      12 => "Invalid Queue Deletion: Invalid I/O Completion Queue specified to delete"
      13 => "Feature Identifier Not Saveable: The Feature Identifier specified does not support a saveable value"
      14 => "Feature Not Changeable: The Feature Identifier is not able to be changed"
      15 => "Feature Not Namespace Specific: The Feature Identifier specified is not namespace specific"
      16 => "FW Activation Requires NVM Subsystem Reset: The firmware commit was successful, however, activation of the firmware image requires an NVM Subsystem Reset"
      17 => "FW Activation Requires Controller Level Reset: The firmware commit was successful; however, the image specified does not support being activated without a reset"
      18 => "FW Activation Requires Maximum Time Violation: The image specified if activated immediately would exceed the Maximum Time for Firmware Activation"
      19 => "FW Activation Prohibited: The image specified is being prohibited from activation by the controller"
      20 => "Overlapping Range: The firmware image has overlapping ranges"
      21 => "Namespace Insufficient Capacity: Creating the namespace requires more free space than is currently available"
      22 => "Namespace Identifier Unavailable: The number of namespaces supported has been exceeded"
      24 => "Namespace Already Attached: The controller is already attached to the namespace specified"
      25 => "Namespace Is Private: The namespace is private and is already attached to one controller"
      26 => "Namespace Not Attached: The request to detach the controller could not be completed because the controller is not attached to the namespace"
      27 => "Thin Provisioning Not Supported: Thin provisioning is not supported by the controller"
      28 => "Controller List Invalid: The controller list provided is invalid"
      29 => "Device Self-test In Progress: The controller or NVM subsystem already has a device self-test operation in process"
      30 => "Boot Partition Write Prohibited: The command is trying to modify a Boot Partition while it is locked"
      31 => "Invalid Controller Identifier: An invalid Controller Identifier was specified"
      32 => "Invalid Secondary Controller State: The action requested for the secondary controller is invalid based on the current state of the secondary controller"
      33 => "Invalid Number of Controller Resources: The specified number of Flexible Resources is invalid"
      34 => "Invalid Resource Identifier: At least one of the specified resource identifiers was invalid"
      else => "Unknown"
    }
  }
  if kind == 2 {
    return match code {
      128 => "Write Fault: The write data could not be committed to the media"
      129 => "Unrecovered Read Error: The read data could not be recovered from the media"
      130 => "End-to-end Guard Check Error: The command was aborted due to an end-to-end guard check failure"
      131 => "End-to-end Application Tag Check Error: The command was aborted due to an end-to-end application tag check failure"
      132 => "End-to-end Reference Tag Check Error: The command was aborted due to an end-to-end reference tag check failure"
      133 => "Compare Failure: The command failed due to a miscompare during a Compare command"
      134 => "Access Denied: Access to the namespace and/or LBA range is denied due to lack of access rights"
      135 => "Deallocated or Unwritten Logical Block: The command failed due to an attempt to read from or verify an LBA range containing a deallocated or unwritten logical block"
      else => "Unknown"
    }
  }
  if kind == 3 {
    return match code {
      0 => "Internal Path Error: The command was not completed as the result of a controller internal error"
      1 => "Asymmetric Access Persistent Loss: The requested function (e.g., command) is not able to be performed as a result of the relationship between the controller and the namespace being in the ANA Persistent Loss state"
      2 => "Asymmetric Access Inaccessible: The requested function (e.g., command) is not able to be performed as a result of the relationship between the controller and the namespace being in the ANA Inaccessible state"
      3 => "Asymmetric Access Transition: The requested function (e.g., command) is not able to be performed as a result of the relationship between the controller and the namespace transitioning between ANA states"
      else => "Unknown"
    }
  }
  "Unknown"
}

## The name of a feature identifier.
export pure feature_name(feature_id: Int) -> Str {
  match feature_id {
    1 => "Arbitration"
    2 => "Power Management"
    3 => "LBA Range Type"
    4 => "Temperature Threshold"
    5 => "Error Recovery"
    6 => "Volatile Write Cache"
    7 => "Number of Queues"
    8 => "Interrupt Coalescing"
    9 => "Interrupt Vector Configuration"
    10 => "Write Atomicity Normal"
    11 => "Async Event Configuration"
    12 => "Autonomous Power State Transition"
    13 => "Host Memory Buffer"
    14 => "Timestamp"
    15 => "Keep Alive Timer"
    16 => "Host Controlled Thermal Management"
    17 => "Non-Operational Power State Config"
    18 => "Read Recovery Level Config"
    19 => "Predictable Latency Mode Config"
    20 => "Predictable Latency Mode Window"
    21 => "LBA Status Information Report Interval"
    22 => "Host Behavior Support"
    23 => "Sanitize Config"
    24 => "Endurance Group Event Configuration"
    25 => "I/O Command Set Profile"
    26 => "Spinup Control"
    27 => "Power Loss Signaling Config"
    28 => "Performance Characteristics"
    29 => "Flexible Data Placement"
    30 => "Flexible Data Placement Events"
    31 => "Namespace Admin Label"
    128 => "Software Progress Marker"
    129 => "Host Identifier"
    130 => "Reservation Notification Mask"
    131 => "Reservation Persistence"
    132 => "Namespace Write Protection Config"
    else => if feature_id >= 192 { "Vendor Specific" } else { "Unknown" }
  }
}

## The transport type text of an Error Information log entry.
export pure transport_type_text(trtype: Int) -> Str {
  match trtype {
    0 => "The transport type is not indicated or the error is not transport related."
    1 => "RDMA Transport error."
    2 => "Fibre Channel Transport error."
    3 => "TCP Transport error."
    254 => "Intra-host Transport error."
    else => "Reserved"
  }
}
