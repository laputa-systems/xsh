##! SCSI disk identity and health: the standard INQUIRY page, the unit serial
##! number page, READ CAPACITY data and the informational exceptions and
##! temperature log pages, in smartctl's text layout.
use smart
use smart_json as j

## Standard INQUIRY fields.
export type Inquiry = {device_type: Int, version: Int, vendor: Str, product: Str, revision: Str}

## The last logical block and logical block size of a unit.
export type Capacity = {last: Int, block_bytes: Int}

## The current and reference temperatures, in Celsius, when reported.
export type Temperatures = {current: Int?, trip: Int?}

## The informational exceptions log page's first parameter.
export type Exceptions = {asc: Int, ascq: Int, temperature: Int?}

## Decodes standard INQUIRY data.
export pure inquiry(page: Bytes) -> Inquiry {
  {
    device_type: smart.at(page, 0) % 32,
    version: smart.at(page, 2),
    vendor: field(page, 8, 8),
    product: field(page, 16, 16),
    revision: field(page, 32, 4),
  }
}

pure field(page: Bytes, offset: Int, length: Int) -> Str {
  let raw = page.slice(offset, length: length)
  let decoded = raw.utf8() ?? ""
  decoded.trim()
}

## Whether a device identifies itself as an ATA disk behind a SAT translator.
export pure is_ata(id: Inquiry) -> Bool {
  id.vendor == "ATA"
}

## The serial number from the unit serial number VPD page, or null when the
## page is empty.
export pure serial_number(page: Bytes) -> Str? {
  let length = smart.at(page, 3)
  guard smart.at(page, 1) == 128 and length > 0 else { return null }
  let text = field(page, 4, length)
  if text == "" { return null }
  text
}

## The device type name smartctl prints.
export pure device_type_name(code: Int) -> Str {
  match code {
    0 => "disk"
    1 => "tape"
    5 => "cd/dvd"
    7 => "optical"
    14 => "simplified disk"
    _ => f"unknown (0x{smart.hex(code, 2)})"
  }
}

## The SPC compliance name from the INQUIRY version byte.
export pure compliance(version: Int) -> Str {
  match version {
    3 => "SPC"
    4 => "SPC-2"
    5 => "SPC-3"
    6 => "SPC-4"
    7 => "SPC-5"
    _ => "SCSI"
  }
}

## Capacity from READ CAPACITY(10): last block and block size.
export pure capacity10(page: Bytes) -> Capacity {
  {last: be(page, 0, 4), block_bytes: be(page, 4, 4)}
}

## Capacity from READ CAPACITY(16): last block and block size.
export pure capacity16(page: Bytes) -> Capacity {
  {last: be(page, 0, 8), block_bytes: be(page, 8, 4)}
}

## A big-endian unsigned field; 0 past the end of the page.
export pure be(data: Bytes, offset: Int, width: Int) -> Int {
  bytes.unpack_be(data, width, offset) ?? 0
}

## Decodes the first parameter of the informational exceptions page (2Fh).
export pure exceptions(page: Bytes) -> Exceptions {
  # A four-byte page header and a four-byte parameter header precede the data.
  let temperature = smart.at(page, 10)
  {asc: smart.at(page, 8), ascq: smart.at(page, 9), temperature: if temperature == 255 { null } else { temperature }}
}

## The temperature log page's current and reference temperatures; null when a
## parameter is absent or reports 0xff.
export pure temperatures(page: Bytes) -> Temperatures {
  var current: Int? = null
  var trip: Int? = null
  var offset = 4
  let end = 4 + be(page, 2, 2)
  while offset + 4 <= end {
    let code = be(page, offset, 2)
    let length = smart.at(page, offset + 3)
    let value = smart.at(page, offset + 5)
    if code == 0 and value != 255 { current = value }
    if code == 1 and value != 255 { trip = value }
    offset += 4 + length
  }
  {current: current, trip: trip}
}

pure row(label: Str, value: Str) -> Str {
  smart.pad_end(label + ":", 22) + value + "\n"
}

## The `=== START OF INFORMATION SECTION ===` block of a SCSI device.
export pure info_text(id: Inquiry, serial: Str?, capacity: Capacity?, local_time: Str) -> Str {
  var out = "=== START OF INFORMATION SECTION ===\n"
  out += row("Vendor", id.vendor)
  out += row("Product", id.product)
  out += row("Revision", id.revision)
  out += row("Compliance", compliance(id.version))
  if let size = capacity {
    let total = (size.last + 1) * size.block_bytes
    out += row("User Capacity", f"{smart.thousands(total)} bytes [{smart.capacity_text(total)}]")
    out += row("Logical block size", f"{size.block_bytes} bytes")
  }
  if let number = serial { out += row("Serial number", number) }
  out += row("Device type", device_type_name(id.device_type))
  out += row("Local Time is", local_time)
  out + "\n"
}

## The header of the block that reads health data from the device.
export const READ_HEADER = "=== START OF READ SMART DATA SECTION ===\n"

## The `SMART Health Status` line.
export pure health_text(report: Exceptions) -> Str {
  if report.asc == 0 { return "SMART Health Status: OK\n" }
  f"SMART Health Status: FAILURE PREDICTION THRESHOLD EXCEEDED: ascq=0x{smart.hex(report.ascq, 2)}\n"
}

## Temperature lines for the attribute block.
export pure temperature_text(temps: Temperatures) -> Str {
  var out = ""
  if let current = temps.current { out += f"Current Drive Temperature:     {current} C\n" }
  if let trip = temps.trip { out += f"Drive Trip Temperature:        {trip} C\n" }
  out + "\n"
}

## JSON members for identity.
export pure info_json(id: Inquiry, serial: Str?, capacity: Capacity?) -> List[j.Member] {
  var out: List[j.Member] = [
    j.m("vendor", id.vendor),
    j.m("product", id.product),
    j.m("model_name", f"{id.vendor} {id.product}"),
    j.m("revision", id.revision),
    j.m("scsi_version", compliance(id.version)),
  ]
  if let size = capacity {
    let total = (size.last + 1) * size.block_bytes
    out += [j.m("user_capacity", j.object([j.m("blocks", size.last + 1), j.m("bytes", total)]))]
    out += [j.m("logical_block_size", size.block_bytes)]
  }
  if let number = serial { out += [j.m("serial_number", number)] }
  out += [j.m("device_type", j.object([j.m("scsi_value", id.device_type), j.m("name", device_type_name(id.device_type))]))]
  out
}

## JSON members for the health status.
export pure health_json(report: Exceptions) -> List[j.Member] {
  [j.m("smart_status", j.object([j.m("passed", report.asc == 0)]))]
}

## JSON members for the temperatures.
export pure temperature_json(temps: Temperatures) -> List[j.Member] {
  var out: List[j.Member] = []
  if let current = temps.current { out += [j.m("temperature", j.object([j.m("current", current)]))] }
  if let trip = temps.trip { out += [j.m("scsi_temperature", j.object([j.m("drive_trip", trip)]))] }
  out
}
