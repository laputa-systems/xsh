##! smartctl's ATA presentation: the text sections in smartmontools' layout and
##! the matching JSON records, built from the decoders in `smart`.
use smart
use smart_json as j

# smartctl's tab-aligned blocks put continuation lines at column 40.
const CONT = "\n\t\t\t\t\t"

pure line(label: Str, value: Str) -> Str {
  smart.pad_end(label + ":", 18) + value + "\n"
}

## The `=== START OF INFORMATION SECTION ===` block of an ATA device. `serials`
## is false under `-q noserial`, which omits the serial number and World Wide
## Name.
export pure info_text(page: Bytes, local_time: Str, serials: Bool) -> Str {
  let w = smart.words(page)
  var out = "=== START OF INFORMATION SECTION ===\n"
  out += line("Device Model", smart.ata_string(page, 27, 20))
  if serials { out += line("Serial Number", smart.ata_string(page, 10, 10)) }
  if let name = smart.wwn(w) {
    if serials { out += line("LU WWN Device Id", f"{smart.hex(name.naa, 1)} {smart.hex(name.oui, 6)} {smart.hex(name.id, 9)}") }
  }
  out += line("Firmware Version", smart.ata_string(page, 23, 4))
  let sectors = smart.sector_count(w)
  let logical = smart.logical_sector_bytes(w)
  let physical = smart.physical_sector_bytes(w)
  if sectors > 0 {
    let size = sectors * logical
    out += line("User Capacity", f"{smart.thousands(size)} bytes [{smart.capacity_text(size)}]")
  }
  if logical == physical {
    out += line("Sector Size", f"{logical} bytes logical/physical")
  } else {
    out += line("Sector Sizes", f"{logical} bytes logical, {physical} bytes physical")
  }
  let rotation = smart.rotation_rate(w)
  if rotation > 1 { out += line("Rotation Rate", f"{rotation} rpm") }
  if rotation == 1 { out += line("Rotation Rate", "Solid State Device") }
  if let factor = smart.form_factor(w) { out += line("Form Factor", factor.name) }
  if let trim = smart.trim_text(w) { out += line("TRIM Command", trim) }
  out += line("Device is", "Not in smartctl database")
  out += line("ATA Version is", smart.ata_version(w).text)
  if let sata = smart.sata_version(w) { out += line("SATA Version is", smart.sata_version_text(sata)) }
  out += line("Local Time is", local_time)
  let supported = smart.smart_supported(w)
  let enabled = smart.smart_enabled(w)
  if supported < 0 {
    out += line("SMART support is", "Ambiguous - ATA IDENTIFY DEVICE words 82-83 don't show if SMART supported.")
  } else if supported == 0 {
    out += line("SMART support is", "Unavailable - device lacks SMART capability.")
  } else {
    out += line("SMART support is", "Available - device has SMART capability.")
    if enabled < 0 {
      out += line("SMART support is", "Ambiguous - ATA IDENTIFY DEVICE words 85-87 don't show if SMART is enabled.")
    } else {
      out += line("SMART support is", if enabled == 1 { "Enabled" } else { "Disabled" })
    }
  }
  if supported == 1 {
    out += smart.aam_text(w) + "\n"
    out += smart.apm_text(w) + "\n"
    out += smart.lookahead_text(w) + "\n"
    out += smart.write_cache_text(w) + "\n"
    out += smart.dsn_text(w) + "\n"
    out += smart.security_text(w) + "\n"
  }
  out + "\n"
}

## The header of the block that reads SMART data from the device.
export const READ_HEADER = "=== START OF READ SMART DATA SECTION ===\n"

## The overall-health verdict block. When the attribute table could be read,
## a failure names the attributes at or below their thresholds and a pass
## names the marginal ones (below a threshold in the past, or a usage
## attribute below it now).
export pure health_text(passed: Bool, values: smart.SmartValues?) -> Str {
  if passed {
    var out = "SMART overall-health self-assessment test result: PASSED\n"
    if let table = values {
      let marginal = [item for item in table.attributes if item.state == "failed_past" or (item.state == "failed_now" and !item.prefailure)]
      if !marginal.is_empty() {
        out += "Please note the following marginal Attributes:\n" + attribute_header(false)
        for item in marginal { out += attribute_row(item, false) }
      }
    }
    return out + "\n"
  }
  var out = "SMART overall-health self-assessment test result: FAILED!\nDrive failure expected in less than 24 hours. SAVE ALL DATA.\n"
  if let table = values {
    let failing = [item for item in table.attributes if item.state == "failed_now"]
    if failing.is_empty() {
      out += "No failed Attributes found.\n"
    } else {
      out += "Failed Attributes:\n" + attribute_header(false)
      for item in failing { out += attribute_row(item, false) }
    }
  } else {
    out += "See vendor-specific Attribute list for failed Attributes.\n"
  }
  out + "\n"
}

pure capability_lines(values: smart.SmartValues, w: smart.Words) -> Str {
  var out = "General SMART Values:\n"
  out += f"Offline data collection status:  (0x{smart.hex(values.offline_status, 2)})\t{smart.offline_status_text(values.offline_status)}\n"
  out += f"Self-test execution status:      ({smart.pad_start(f"{values.self_test_status}", 4)})\t{smart.self_test_exec_text(values.self_test_status)}\n"
  out += f"Total time to complete Offline \ndata collection: \t\t({smart.pad_start(f"{values.offline_seconds}", 5)}) seconds.\n"
  let cap = values.offline_capability
  out += f"Offline data collection\ncapabilities: \t\t\t (0x{smart.hex(cap, 2)}) "
  if cap == 0 {
    out += "\tOff-line data collection not supported.\n"
  } else {
    out += (if smart.bit_set(cap, 0) { "SMART execute Offline immediate." } else { "No SMART execute Offline immediate." }) + CONT
    out += (if smart.bit_set(cap, 1) { "Auto Offline data collection on/off support." } else { "No Auto Offline data collection support." }) + CONT
    out += (if smart.bit_set(cap, 2) { f"Abort Offline collection upon new{CONT}command." } else { f"Suspend Offline collection upon new{CONT}command." }) + CONT
    out += (if smart.bit_set(cap, 3) { "Offline surface scan supported." } else { "No Offline surface scan supported." }) + CONT
    out += (if smart.bit_set(cap, 4) { "Self-test supported." } else { "No Self-test supported." }) + CONT
    out += (if smart.bit_set(cap, 5) { "Conveyance Self-test supported." } else { "No Conveyance Self-test supported." }) + CONT
    out += (if smart.bit_set(cap, 6) { "Selective Self-test supported." } else { "No Selective Self-test supported." }) + "\n"
  }
  let save = values.smart_capability
  let tabs = CONT.byte_slice(1)
  out += f"SMART capabilities:            (0x{smart.hex(save, 4)})\t"
  if save == 0 {
    # smartctl's own text has no line break between these two sentences.
    out += f"Automatic saving of SMART data{tabs}is not implemented.\n"
  } else {
    if smart.bit_set(save, 0) {
      out += f"Saves SMART data before entering{CONT}power-saving mode.\n"
    } else {
      out += f"Does not save SMART data before{CONT}entering power-saving mode.\n"
    }
    if smart.bit_set(save, 1) { out += tabs + "Supports SMART auto save timer.\n" }
  }
  let logging = values.error_log_capability
  let not_supported = if smart.bit_set(logging, 0) { "" } else { "NOT " }
  out += f"Error logging capability:        (0x{smart.hex(logging, 2)})\tError logging {not_supported}supported.\n"
  out += tabs + (if smart.gp_logging(w) { "General Purpose Logging supported.\n" } else { "No General Purpose Logging support.\n" })
  let self_test = smart.bit_set(cap, 4)
  let short_time = if self_test { f" ({smart.pad_start(f"{values.short_minutes}", 4)}) minutes." } else { "        Not Supported." }
  let extended_time = if self_test { f" ({smart.pad_start(f"{values.extended_minutes}", 4)}) minutes." } else { "        Not Supported." }
  let conveyance_time = if smart.bit_set(cap, 5) { f" ({smart.pad_start(f"{values.conveyance_minutes}", 4)}) minutes." } else { "        Not Supported." }
  out += f"Short self-test routine \nrecommended polling time: \t{short_time}\n"
  out += f"Extended self-test routine\nrecommended polling time: \t{extended_time}\n"
  out += f"Conveyance self-test routine\nrecommended polling time: \t{conveyance_time}\n"
  let sct = w[206]
  if smart.bit_set(sct, 0) and w[206] != 65535 {
    out += f"SCT capabilities: \t       (0x{smart.hex(sct, 4)})\tSCT Status supported.\n"
    if smart.bit_set(sct, 3) { out += CONT.byte_slice(1) + "SCT Error Recovery Control supported.\n" }
    if smart.bit_set(sct, 4) { out += CONT.byte_slice(1) + "SCT Feature Control supported.\n" }
    if smart.bit_set(sct, 5) { out += CONT.byte_slice(1) + "SCT Data Table supported.\n" }
  }
  out + "\n"
}

## The `General SMART Values` block (smartctl -c).
export pure capabilities_text(values: smart.SmartValues, page: Bytes) -> Str {
  capability_lines(values, smart.words(page))
}

pure flag_letters(flags: Int) -> Str {
  let letters = "POSRCK"
  var out = ""
  for bit in range(6) {
    out += if smart.bit_set(flags, bit) { letters.byte_slice(bit, 1) } else { "-" }
  }
  out
}

pure when_failed(state: Str) -> Str {
  if state == "failed_now" { return "FAILING_NOW" }
  if state == "failed_past" { return "In_the_past" }
  "-"
}

pure attribute_header(brief: Bool) -> Str {
  if brief { return "ID# ATTRIBUTE_NAME          FLAGS    VALUE WORST THRESH FAIL RAW_VALUE\n" }
  "ID# ATTRIBUTE_NAME          FLAG     VALUE WORST THRESH TYPE      UPDATED  WHEN_FAILED RAW_VALUE\n"
}

pure attribute_row(attribute: smart.Attribute, brief: Bool) -> Str {
  let threshold = if attribute.threshold == null { "---" } else { smart.zero_pad(attribute.threshold ?? 0, 3) }
  let numbers = f"{smart.zero_pad(attribute.value, 3)}   {smart.zero_pad(attribute.worst, 3)}   {threshold}"
  let head = f"{smart.pad_start(f"{attribute.id}", 3)} {smart.pad_end(attribute.name, 24)}"
  if brief {
    let fail = if attribute.state == "failed_now" { "NOW" } else if attribute.state == "failed_past" { "Past" } else { "-" }
    return f"{head}{smart.pad_end(flag_letters(attribute.flags), 9)}{numbers}    {smart.pad_end(fail, 5)}{attribute.raw_text}\n"
  }
  let kind = if attribute.prefailure { "Pre-fail" } else { "Old_age" }
  let updated = if smart.bit_set(attribute.flags, 1) { "Always" } else { "Offline" }
  let failed = when_failed(attribute.state)
  let failed_column = if failed == "-" { "    -       " } else { smart.pad_end(failed, 12) }
  f"{head}0x{smart.hex(attribute.flags, 4)}   {numbers}    {smart.pad_end(kind, 10)}{smart.pad_end(updated, 9)}{failed_column}{attribute.raw_text}\n"
}

## The attribute table. `brief` selects the compact `-f brief` columns.
export pure attributes_text(values: smart.SmartValues, brief: Bool) -> Str {
  var out = f"SMART Attributes Data Structure revision number: {values.revision}\n"
  out += "Vendor Specific SMART Attributes with Thresholds:\n"
  out += attribute_header(brief)
  for attribute in values.attributes { out += attribute_row(attribute, brief) }
  if brief {
    out += "                            ||||||_ K auto-keep\n"
    out += "                            |||||__ C event count\n"
    out += "                            ||||___ R error rate\n"
    out += "                            |||____ S speed/performance\n"
    out += "                            ||_____ O updated online\n"
    out += "                            |______ P prefailure warning\n"
  }
  out + "\n"
}

## The summary SMART error log block.
export pure error_log_text(log: smart.ErrorLog) -> Str {
  var out = f"SMART Error Log Version: {log.revision}\n"
  if log.pointer == 0 { return out + "No Errors Logged\n\n" }
  if log.pointer > 5 {
    return out + f"Invalid Error Log index = 0x{smart.hex(log.pointer, 2)} (valid range is from 1 to 5)\n\n"
  }
  if log.count <= 5 {
    out += f"ATA Error Count: {log.count}\n"
  } else {
    out += f"ATA Error Count: {log.count} (device log contains only the most recent five errors)\n"
  }
  out += "\tCR = Command Register [HEX]\n"
  out += "\tFR = Features Register [HEX]\n"
  out += "\tSC = Sector Count Register [HEX]\n"
  out += "\tSN = Sector Number Register [HEX]\n"
  out += "\tCL = Cylinder Low Register [HEX]\n"
  out += "\tCH = Cylinder High Register [HEX]\n"
  out += "\tDH = Device/Head Register [HEX]\n"
  out += "\tDC = Device Command Register [HEX]\n"
  out += "\tER = Error register [HEX]\n"
  out += "\tST = Status register [HEX]\n"
  out += "Powered_Up_Time is measured from power on, and printed as\n"
  out += "DDd+hh:mm:SS.sss where DD=days, hh=hours, mm=minutes,\n"
  out += "SS=sec, and sss=millisec. It \"wraps\" after 49.710 days.\n\n"
  for entry in log.entries {
    out += f"Error {entry.number} occurred at disk power-on lifetime: {entry.hours} hours ({entry.hours / 24} days + {entry.hours % 24} hours)\n"
    out += f"  When the command that caused the error occurred, the device was {smart.error_state_text(entry.state)}.\n\n"
    out += "  After command completion occurred, registers were:\n  ER ST SC SN CL CH DH\n  -- -- -- -- -- -- --\n"
    out += f"  {smart.hex(entry.error, 2)} {smart.hex(entry.status, 2)} {smart.hex(entry.count, 2)} {smart.hex(entry.lba_low, 2)} {smart.hex(entry.lba_mid, 2)} {smart.hex(entry.lba_high, 2)} {smart.hex(entry.device, 2)}"
    let description = smart.error_description(entry)
    if description != "" { out += "  " + description }
    out += "\n\n"
    out += "  Commands leading to the command that caused the error were:\n"
    out += "  CR FR SC SN CL CH DH DC   Powered_Up_Time  Command/Feature_Name\n"
    out += "  -- -- -- -- -- -- -- --  ----------------  --------------------\n"
    for command in entry.commands {
      let regs = f"{smart.hex(command.command, 2)} {smart.hex(command.features, 2)} {smart.hex(command.count, 2)} {smart.hex(command.lba_low, 2)} {smart.hex(command.lba_mid, 2)} {smart.hex(command.lba_high, 2)} {smart.hex(command.device, 2)} {smart.hex(command.device_control, 2)}"
      out += f"  {regs}  {smart.pad_start(smart.power_up_time(command.timestamp_ms), 16)}  {smart.command_name(command.command, command.features)}\n"
    }
    out += "\n"
  }
  out
}

## The SMART self-test log block.
export pure self_test_log_text(log: smart.SelfTestLog) -> Str {
  var out = f"SMART Self-test log structure revision number {log.revision}\n"
  if log.revision != 1 {
    out += "Warning: ATA Specification requires self-test log structure revision number = 1\n"
  }
  if log.entries.is_empty() {
    return out + "No self-tests have been logged.  [To run self-tests, use: smartctl -t]\n\n"
  }
  out += "Num  Test_Description    Status                  Remaining  LifeTime(hours)  LBA_of_first_error\n"
  for entry in log.entries {
    let failed = entry.status >= 3 and entry.status <= 8
    let lba = if failed and entry.lba != 4294967295 { f"{entry.lba}" } else { "-" }
    out += f"#{smart.pad_start(f"{entry.number}", 2)}  {smart.pad_end(smart.self_test_name(entry.kind), 19)} {smart.pad_end(smart.self_test_status_text(entry.status), 29)} {entry.remaining_percent / 10}0%  {smart.pad_start(f"{entry.hours}", 8)}         {lba}\n"
  }
  if log.outdated_count > 0 {
    out += f"{log.outdated_count} of {log.error_count} failed self-tests are outdated by newer successful extended offline self-test #{smart.pad_start(f"{log.outdated_by}", 2)}\n"
  }
  out + "\n"
}

# The span under test takes its status from the self-test execution status of
# the SMART data page; every other span is not being tested.
pure span_status(log: smart.SelectiveLog, exec_status: Int, index: Int) -> Str {
  let span = log.spans[index]
  if log.current_span == index + 1 {
    let nibble = exec_status / 16
    let names = ["", "Aborted_by_host", "Interrupted", "Fatal_error", "Completed_unknown_failure", "Completed_electrical_failure", "Completed_servo/seek_failure", "Completed_read_failure", "Completed_handling_damage??"]
    if nibble == 15 {
      return f"Self_test_in_progress [{exec_status % 16}0% left] ({span.start}-{span.end})"
    }
    if nibble >= 1 and nibble <= 8 { return names[nibble] }
  }
  "Not_testing"
}

## The selective self-test log block.
export pure selective_log_text(log: smart.SelectiveLog, exec_status: Int) -> Str {
  var out = f"SMART Selective self-test log data structure revision number {log.revision}\n"
  if log.revision != 1 {
    out += "Note: revision number not 1 implies that no selective self-test has ever been run\n"
  }
  # The LBA columns widen to fit the largest span end, never below the
  # header names.
  var width = 7
  for span in log.spans {
    let digits = f"{span.end}".byte_len()
    if digits > width { width = digits }
  }
  out += f" SPAN  {smart.pad_start("MIN_LBA", width)}  {smart.pad_start("MAX_LBA", width)}  CURRENT_TEST_STATUS\n"
  for index in range(5) {
    let span = log.spans[index]
    out += f"    {index + 1}  {smart.pad_start(f"{span.start}", width)}  {smart.pad_start(f"{span.end}", width)}  {span_status(log, exec_status, index)}\n"
  }
  out += f"Selective self-test flags (0x{smart.hex(log.flags, 1)}):\n"
  let scan = smart.bit_set(log.flags, 1)
  if scan {
    if smart.bit_set(log.flags, 4) {
      out += "  Currently read-scanning the remainder of the disk.\n"
    } else if smart.bit_set(log.flags, 3) {
      out += f"  Read-scan of remainder of disk interrupted; will resume {log.pending_minutes} min after power-up.\n"
    } else {
      out += "  After scanning selected spans, read-scan remainder of disk.\n"
    }
  } else {
    out += "  After scanning selected spans, do NOT read-scan remainder of disk.\n"
  }
  out += f"If Selective self-test is pending on power-up, resume after {log.pending_minutes} minute delay.\n"
  out + "\n"
}

## The SMART log directory block.
export pure directory_text(dir: smart.LogDirectory) -> Str {
  var out = f"SMART Log Directory Version {dir.revision}"
  out += if dir.revision == 1 { " [multi-sector log support]\n" } else { "\n" }
  out += "Address    Access  R/W   Size  Description\n"
  # Consecutive addresses that share a description and size print as a range.
  var index = 0
  while index < dir.entries.len() {
    let entry = dir.entries[index]
    let described = smart.log_name(entry.address)
    var last = entry.address
    var next = index + 1
    while next < dir.entries.len() {
      let candidate = dir.entries[next]
      let other = smart.log_name(candidate.address)
      if candidate.address != last + 1 or candidate.sectors != entry.sectors or other.name != described.name or other.access != described.access { break }
      last = candidate.address
      next += 1
    }
    let first_text = f"0x{smart.hex(entry.address, 2)}"
    let label = if last == entry.address { f"{first_text}       " } else { f"{first_text}-0x{smart.hex(last, 2)}  " }
    out += f"{label}{smart.pad_start("SL", 6)}  {smart.pad_end(described.access, 3)}  {smart.pad_start(f"{entry.sectors}", 5)}  {described.name}\n"
    index = next
  }
  out + "\n"
}

pure speed_name(gen: Int) -> Str {
  ["", "1.5 Gb/s", "3.0 Gb/s", "6.0 Gb/s"][gen]
}

pure speed_object(gen: Int) -> j.Object {
  j.object([
    j.m("sata_value", gen),
    j.m("string", speed_name(gen)),
    j.m("units_per_second", [0, 15, 30, 60][gen]),
    j.m("bits_per_unit", 100000000),
  ])
}

## JSON members for the identity block; `serials` is false under `-q noserial`.
export pure info_json(page: Bytes, serials: Bool) -> List[j.Member] {
  let w = smart.words(page)
  let supported = smart.smart_supported(w)
  var out: List[j.Member] = [j.m("model_name", smart.ata_string(page, 27, 20))]
  if serials { out += [j.m("serial_number", smart.ata_string(page, 10, 10))] }
  if let name = smart.wwn(w) {
    if serials { out += [j.m("wwn", j.object([j.m("naa", name.naa), j.m("oui", name.oui), j.m("id", name.id)]))] }
  }
  out += [j.m("firmware_version", smart.ata_string(page, 23, 4))]
  let sectors = smart.sector_count(w)
  let logical = smart.logical_sector_bytes(w)
  if sectors > 0 {
    out += [j.m("user_capacity", j.object([j.m("blocks", sectors), j.m("bytes", sectors * logical)]))]
  }
  out += [j.m("logical_block_size", logical), j.m("physical_block_size", smart.physical_sector_bytes(w))]
  let rotation = smart.rotation_rate(w)
  if rotation == 1 { out += [j.m("rotation_rate", 0)] }
  if rotation > 1 { out += [j.m("rotation_rate", rotation)] }
  if let factor = smart.form_factor(w) {
    out += [j.m("form_factor", j.object([j.m("ata_value", factor.value), j.m("name", factor.name)]))]
  }
  if smart.bit_set(w[169], 0) {
    out += [j.m("trim", j.object([
      j.m("supported", true),
      j.m("deterministic", smart.bit_set(w[69], 14)),
      j.m("zeroed", smart.bit_set(w[69], 5)),
    ]))]
  }
  let version = smart.ata_version(w)
  out += [j.m("in_smartctl_database", false)]
  out += [j.m("ata_version", j.object([
    j.m("string", version.text),
    j.m("major_value", w[80]),
    j.m("minor_value", version.minor),
  ]))]
  if let sata = smart.sata_version(w) {
    out += [j.m("sata_version", j.object([j.m("string", sata.text), j.m("value", w[222])]))]
    if sata.max_speed >= 1 and sata.max_speed <= 3 {
      var speeds: List[j.Member] = [j.m("max", speed_object(sata.max_speed))]
      if sata.current_speed >= 1 and sata.current_speed <= 3 {
        speeds += [j.m("current", speed_object(sata.current_speed))]
      }
      out += [j.m("interface_speed", j.object(speeds))]
    }
  }
  let enabled = smart.smart_enabled(w)
  if supported == 1 and enabled >= 0 {
    out += [j.m("smart_support", j.object([j.m("available", true), j.m("enabled", enabled == 1)]))]
  } else if supported >= 0 {
    out += [j.m("smart_support", j.object([j.m("available", supported == 1)]))]
  }
  out
}

## JSON members for the capabilities block.
export pure capabilities_json(values: smart.SmartValues, page: Bytes) -> List[j.Member] {
  let w = smart.words(page)
  let cap = values.offline_capability
  let sct = w[206]
  let exec = values.self_test_status
  let nibble = exec / 16
  var out: List[j.Member] = [j.m("ata_smart_data", j.object([
    j.m("offline_data_collection", j.object([
      j.m("status", j.object([
        j.m("value", values.offline_status),
        j.m("string", smart.offline_activity(values.offline_status)),
        j.m("passed", values.offline_status % 128 != 6),
      ])),
      j.m("completion_seconds", values.offline_seconds),
    ])),
    j.m("self_test", j.object([
      j.m("status", j.object([
        j.m("value", exec),
        j.m("string", smart.self_test_exec_name(exec)),
        j.m("passed", nibble < 3 or nibble == 15),
      ])),
      j.m("polling_minutes", j.object([
        j.m("short", values.short_minutes),
        j.m("extended", values.extended_minutes),
        j.m("conveyance", values.conveyance_minutes),
      ])),
    ])),
    j.m("capabilities", j.object([
      j.m("values", [cap, values.smart_capability]),
      j.m("exec_offline_immediate_supported", smart.bit_set(cap, 0)),
      j.m("offline_is_aborted_upon_new_cmd", smart.bit_set(cap, 2)),
      j.m("offline_surface_scan_supported", smart.bit_set(cap, 3)),
      j.m("self_tests_supported", smart.bit_set(cap, 4)),
      j.m("conveyance_self_test_supported", smart.bit_set(cap, 5)),
      j.m("selective_self_test_supported", smart.bit_set(cap, 6)),
      j.m("attribute_autosave_enabled", smart.bit_set(values.smart_capability, 1)),
      j.m("error_logging_supported", smart.bit_set(values.error_log_capability, 0)),
      j.m("gp_logging_supported", smart.gp_logging(w)),
    ])),
  ]))]
  if smart.bit_set(sct, 0) and sct != 65535 {
    out += [j.m("ata_sct_capabilities", j.object([
      j.m("value", sct),
      j.m("error_recovery_control_supported", smart.bit_set(sct, 3)),
      j.m("feature_control_supported", smart.bit_set(sct, 4)),
      j.m("data_table_supported", smart.bit_set(sct, 5)),
    ]))]
  }
  out
}

## JSON members for the attribute table, with the power-on, cycle-count and
## temperature summaries smartctl reports beside it.
export pure attributes_json(values: smart.SmartValues) -> List[j.Member] {
  var table: List[j.Object] = []
  for attribute in values.attributes {
    let flags = attribute.flags
    table += [j.object([
      j.m("id", attribute.id),
      j.m("name", attribute.name),
      j.m("value", attribute.value),
      j.m("worst", attribute.worst),
      j.m("thresh", attribute.threshold ?? 0),
      j.m("when_failed", if attribute.state == "failed_now" { "now" } else if attribute.state == "failed_past" { "past" } else { "" }),
      j.m("flags", j.object([
        j.m("value", flags),
        j.m("string", flag_letters(flags)),
        j.m("prefailure", smart.bit_set(flags, 0)),
        j.m("updated_online", smart.bit_set(flags, 1)),
        j.m("performance", smart.bit_set(flags, 2)),
        j.m("error_rate", smart.bit_set(flags, 3)),
        j.m("event_count", smart.bit_set(flags, 4)),
        j.m("auto_keep", smart.bit_set(flags, 5)),
      ])),
      j.m("raw", j.object([j.m("value", attribute.raw_value), j.m("string", attribute.raw_text)])),
    ])]
  }
  var out: List[j.Member] = [j.m("ata_smart_attributes", j.object([j.m("revision", values.revision), j.m("table", table)]))]
  var temperature: Int? = null
  for attribute in values.attributes {
    if attribute.id == 9 { out += [j.m("power_on_time", j.object([j.m("hours", attribute.raw_value % 4294967296)]))] }
    if attribute.id == 12 { out += [j.m("power_cycle_count", attribute.raw_value)] }
    if attribute.id == 194 or (attribute.id == 190 and temperature == null) { temperature = attribute.raw_bytes[0] }
  }
  if let current = temperature { out += [j.m("temperature", j.object([j.m("current", current)]))] }
  out
}

## JSON members for the summary error log.
export pure error_log_json(log: smart.ErrorLog) -> List[j.Member] {
  var summary: List[j.Member] = [j.m("revision", log.revision), j.m("count", log.count)]
  if !log.entries.is_empty() {
    summary += [j.m("logged_count", log.entries.len())]
    var table: List[j.Object] = []
    for entry in log.entries {
      var previous: List[j.Object] = []
      for command in entry.commands {
        previous += [j.object([
          j.m("registers", j.object([
            j.m("command", command.command),
            j.m("features", command.features),
            j.m("count", command.count),
            j.m("lba", command.lba_high * 65536 + command.lba_mid * 256 + command.lba_low),
            j.m("device", command.device),
            j.m("device_control", command.device_control),
          ])),
          j.m("powerup_milliseconds", command.timestamp_ms),
          j.m("command_name", smart.command_name(command.command, command.features)),
        ])]
      }
      table += [j.object([
        j.m("error_number", entry.number),
        j.m("lifetime_hours", entry.hours),
        j.m("completion_registers", j.object([
          j.m("error", entry.error),
          j.m("status", entry.status),
          j.m("count", entry.count),
          j.m("lba", entry.lba_high * 65536 + entry.lba_mid * 256 + entry.lba_low),
          j.m("device", entry.device),
        ])),
        j.m("error_description", smart.error_description(entry)),
        j.m("previous_commands", previous),
      ])]
    }
    summary += [j.m("table", table)]
  }
  [j.m("ata_smart_error_log", j.object([j.m("summary", j.object(summary))]))]
}

## JSON members for the self-test log.
export pure self_test_log_json(log: smart.SelfTestLog) -> List[j.Member] {
  var table: List[j.Object] = []
  for entry in log.entries {
    let failed = entry.status >= 3 and entry.status <= 8
    var status: List[j.Member] = [
      j.m("value", entry.status * 16 + entry.remaining_percent / 10),
      j.m("string", smart.self_test_status_text(entry.status)),
      j.m("passed", !failed),
    ]
    if entry.status == 15 { status += [j.m("remaining_percent", entry.remaining_percent)] }
    var item: List[j.Member] = [
      j.m("type", j.object([j.m("value", entry.kind), j.m("string", smart.self_test_name(entry.kind))])),
      j.m("status", j.object(status)),
      j.m("lifetime_hours", entry.hours),
    ]
    if failed and entry.lba != 4294967295 { item += [j.m("lba", entry.lba)] }
    table += [j.object(item)]
  }
  [j.m("ata_smart_self_test_log", j.object([j.m("standard", j.object([
    j.m("revision", log.revision),
    j.m("table", table),
    j.m("count", log.entries.len()),
    j.m("error_count_total", log.error_count),
    j.m("error_count_outdated", 0),
  ]))]))]
}

## JSON members for the selective self-test log.
export pure selective_log_json(log: smart.SelectiveLog, exec_status: Int) -> List[j.Member] {
  var table: List[j.Object] = []
  for index in range(5) {
    let span = log.spans[index]
    let text = span_status(log, exec_status, index)
    table += [j.object([
      j.m("lba_min", span.start),
      j.m("lba_max", span.end),
      j.m("status", j.object([j.m("value", if text == "Not_testing" { 0 } else { exec_status / 16 }), j.m("string", text)])),
    ])]
  }
  [j.m("ata_smart_selective_self_test_log", j.object([
    j.m("revision", log.revision),
    j.m("table", table),
    j.m("flags", j.object([j.m("value", log.flags), j.m("remainder_scan_enabled", smart.bit_set(log.flags, 1))])),
    j.m("power_up_scan_resume_minutes", log.pending_minutes),
  ]))]
}

## JSON members for the SMART log directory.
export pure directory_json(dir: smart.LogDirectory) -> List[j.Member] {
  var table: List[j.Object] = []
  for entry in dir.entries {
    let described = smart.log_name(entry.address)
    table += [j.object([
      j.m("address", entry.address),
      j.m("name", described.name),
      j.m("read", true),
      j.m("write", described.access == "R/W"),
      j.m("smart_sectors", entry.sectors),
    ])]
  }
  [j.m("ata_log_directory", j.object([
    j.m("smart_dir_version", dir.revision),
    j.m("smart_dir_multi_sector", dir.revision == 1),
    j.m("table", table),
  ]))]
}
