##! NVMe identify, SMART/health, error-information and self-test log decoding,
##! with smartctl's text layout and JSON records.
use smart
use smart_json as j

## Controller identity fields smartctl reports.
export type Controller = {
  vendor: Int,
  subsystem_vendor: Int,
  serial: Str,
  model: Str,
  firmware: Str,
  ieee: Int,
  controller_id: Int,
  version: Int,
  oacs: Int,
  frmw: Int,
  lpa: Int,
  mdts: Int,
  wctemp: Int,
  cctemp: Int,
  total_capacity: Int,
  unallocated_capacity: Int,
  namespaces: Int,
  oncs: Int,
  power_state_count: Int,
}

## One LBA format of a namespace.
export type LbaFormat = {metadata_size: Int, data_bytes: Int, relative_performance: Int}

## Namespace identity fields smartctl reports.
export type Namespace = {
  features: Int,
  size_blocks: Int,
  capacity_blocks: Int,
  utilization_blocks: Int,
  formatted: Int,
  lba_size: Int,
  eui64: Bytes,
  formats: List[LbaFormat],
}

## The SMART/health information log page.
export type Health = {
  critical_warning: Int,
  temperature: Int,
  available_spare: Int,
  spare_threshold: Int,
  percentage_used: Int,
  data_units_read: Int,
  data_units_written: Int,
  host_reads: Int,
  host_writes: Int,
  busy_minutes: Int,
  power_cycles: Int,
  power_on_hours: Int,
  unsafe_shutdowns: Int,
  media_errors: Int,
  error_entries: Int,
  warning_temp_minutes: Int,
  critical_temp_minutes: Int,
  sensors: List[Int],
  thermal_transitions: List[Int],
  thermal_minutes: List[Int],
}

## One error information log entry.
export type ErrorRecord = {count: Int, queue_id: Int, command_id: Int, status: Int, location: Int, lba: Int, nsid: Int, vendor: Int}

## One self-test log entry.
export type SelfTestRecord = {code: Int, result: Int, segment: Int, hours: Int, nsid: Int, lba: Int, sct: Int, sc: Int, flags: Int}

## The device self-test log.
export type SelfTestLog = {operation: Int, completion: Int, entries: List[SelfTestRecord]}

# Size of one error information entry and of the self-test log header and entry.
const ERROR_ENTRY_BYTES = 64
const SELF_TEST_ENTRY_BYTES = 28

## Decodes the identify controller page (4096 bytes).
export pure controller(page: Bytes) -> Controller {
  {
    vendor: smart.le(page, 0, 2),
    subsystem_vendor: smart.le(page, 2, 2),
    serial: text(page, 4, 20),
    model: text(page, 24, 40),
    firmware: text(page, 64, 8),
    ieee: smart.at(page, 75) * 65536 + smart.at(page, 74) * 256 + smart.at(page, 73),
    controller_id: smart.le(page, 78, 2),
    version: smart.le(page, 80, 4),
    oacs: smart.le(page, 256, 2),
    frmw: smart.at(page, 260),
    lpa: smart.at(page, 261),
    mdts: smart.at(page, 77),
    wctemp: smart.le(page, 266, 2),
    cctemp: smart.le(page, 268, 2),
    total_capacity: smart.le(page, 280, 8),
    unallocated_capacity: smart.le(page, 296, 8),
    namespaces: smart.le(page, 516, 4),
    oncs: smart.le(page, 520, 2),
    power_state_count: smart.at(page, 263) + 1,
  }
}

pure text(page: Bytes, offset: Int, length: Int) -> Str {
  let raw = page.slice(offset, length: length)
  let decoded = raw.utf8() ?? ""
  decoded.trim()
}

## Decodes the identify namespace page (4096 bytes).
export pure namespace(page: Bytes) -> Namespace {
  let count = smart.at(page, 25) + 1
  var formats: List[LbaFormat] = []
  for index in range(count) {
    let base = 128 + index * 4
    formats += [{
      metadata_size: smart.le(page, base, 2),
      data_bytes: smart.pow2(smart.at(page, base + 2)),
      relative_performance: smart.at(page, base + 3) % 4,
    }]
  }
  let formatted = smart.at(page, 26) % 16
  let current = if formatted < formats.len() { formats[formatted].data_bytes } else { 512 }
  {
    features: smart.at(page, 24),
    size_blocks: smart.le(page, 0, 8),
    capacity_blocks: smart.le(page, 8, 8),
    utilization_blocks: smart.le(page, 16, 8),
    formatted: formatted,
    lba_size: current,
    eui64: page.slice(120, length: 8),
    formats: formats,
  }
}

## Decodes the SMART/health information log page (512 bytes).
export pure health(page: Bytes) -> Health {
  var sensors: List[Int] = []
  for index in range(8) {
    let kelvin = smart.le(page, 200 + index * 2, 2)
    sensors += [if kelvin == 0 { 0 } else { kelvin - 273 }]
  }
  let kelvin = smart.le(page, 1, 2)
  {
    critical_warning: smart.at(page, 0),
    temperature: if kelvin == 0 { 0 } else { kelvin - 273 },
    available_spare: smart.at(page, 3),
    spare_threshold: smart.at(page, 4),
    percentage_used: smart.at(page, 5),
    data_units_read: smart.le(page, 32, 8),
    data_units_written: smart.le(page, 48, 8),
    host_reads: smart.le(page, 64, 8),
    host_writes: smart.le(page, 80, 8),
    busy_minutes: smart.le(page, 96, 8),
    power_cycles: smart.le(page, 112, 8),
    power_on_hours: smart.le(page, 128, 8),
    unsafe_shutdowns: smart.le(page, 144, 8),
    media_errors: smart.le(page, 160, 8),
    error_entries: smart.le(page, 176, 8),
    warning_temp_minutes: smart.le(page, 192, 4),
    critical_temp_minutes: smart.le(page, 196, 4),
    sensors: sensors,
    thermal_transitions: [smart.le(page, 216, 4), smart.le(page, 220, 4)],
    thermal_minutes: [smart.le(page, 224, 4), smart.le(page, 228, 4)],
  }
}

## Decodes the error information log, skipping unused entries.
export pure error_records(page: Bytes) -> List[ErrorRecord] {
  var found: List[ErrorRecord] = []
  for index in range(page.len() / ERROR_ENTRY_BYTES) {
    let base = index * ERROR_ENTRY_BYTES
    let count = smart.le(page, base, 8)
    continue when count == 0
    found += [{
      count: count,
      queue_id: smart.le(page, base + 8, 2),
      command_id: smart.le(page, base + 10, 2),
      status: smart.le(page, base + 12, 2) / 2,
      location: smart.le(page, base + 14, 2),
      lba: smart.le(page, base + 16, 8),
      nsid: smart.le(page, base + 24, 4),
      vendor: smart.at(page, base + 28),
    }]
  }
  found
}

## Decodes the device self-test log page (564 bytes).
export pure self_test_log(page: Bytes) -> SelfTestLog {
  var entries: List[SelfTestRecord] = []
  for index in range(20) {
    let base = 4 + index * SELF_TEST_ENTRY_BYTES
    let status = smart.at(page, base)
    continue when status / 16 == 15
    entries += [{
      code: status % 16,
      result: status / 16,
      segment: smart.at(page, base + 1),
      flags: smart.at(page, base + 2),
      hours: smart.le(page, base + 4, 8),
      nsid: smart.le(page, base + 12, 4),
      lba: smart.le(page, base + 16, 8),
      sct: smart.at(page, base + 24) % 8,
      sc: smart.at(page, base + 25),
    }]
  }
  {operation: smart.at(page, 0) % 16, completion: smart.at(page, 1) % 128, entries: entries}
}

## The `major.minor[.tertiary]` version text.
export pure version_text(version: Int) -> Str {
  let major = version / 65536
  let minor = version / 256 % 256
  let tertiary = version % 256
  if tertiary != 0 { return f"{major}.{minor}.{tertiary}" }
  f"{major}.{minor}"
}

pure row(label: Str, value: Str) -> Str {
  smart.pad_end(label + ":", 36) + value + "\n"
}

pure size_text(size: Int) -> Str {
  f"{smart.thousands(size)} [{smart.capacity_text(size)}]"
}

pure eui64_text(raw: Bytes) -> Str? {
  guard !smart.all_zero(raw, 0, 8) else { return null }
  let oui = smart.at(raw, 0) * 65536 + smart.at(raw, 1) * 256 + smart.at(raw, 2)
  var extension = 0
  for index in range(3, 8) { extension = extension * 256 + smart.at(raw, index) }
  f"{smart.hex(oui, 6)} {smart.hex(extension, 10)}"
}

## The `=== START OF INFORMATION SECTION ===` block of an NVMe device. The
## namespace is optional: a controller node has none to describe. `serials` is
## false under `-q noserial`, which omits the serial number and EUI-64.
export pure info_text(ctrl: Controller, ns: Namespace?, nsid: Int, local_time: Str, serials: Bool) -> Str {
  var out = "=== START OF INFORMATION SECTION ===\n"
  out += row("Model Number", ctrl.model)
  if serials { out += row("Serial Number", ctrl.serial) }
  out += row("Firmware Version", ctrl.firmware)
  if ctrl.vendor == ctrl.subsystem_vendor {
    out += row("PCI Vendor/Subsystem ID", f"0x{smart.hex(ctrl.vendor, 4)}")
  } else {
    out += row("PCI Vendor ID", f"0x{smart.hex(ctrl.vendor, 4)}")
    out += row("PCI Vendor Subsystem ID", f"0x{smart.hex(ctrl.subsystem_vendor, 4)}")
  }
  out += row("IEEE OUI Identifier", f"0x{smart.hex(ctrl.ieee, 6)}")
  if ctrl.total_capacity > 0 {
    out += row("Total NVM Capacity", size_text(ctrl.total_capacity))
    out += row("Unallocated NVM Capacity", if ctrl.unallocated_capacity == 0 { "0" } else { size_text(ctrl.unallocated_capacity) })
  }
  if ctrl.version >= 66048 { out += row("Controller ID", f"{ctrl.controller_id}") }
  # Version fields below 1.2 are not reliable, so smartctl shows a bound.
  out += row("NVMe Version", if ctrl.version >= 66048 { version_text(ctrl.version) } else { "<1.2" })
  out += row("Number of Namespaces", f"{ctrl.namespaces}")
  if let space = ns {
    let size = space.size_blocks * space.lba_size
    let capacity = space.capacity_blocks * space.lba_size
    if size == capacity {
      out += row(f"Namespace {nsid} Size/Capacity", size_text(size))
    } else {
      out += row(f"Namespace {nsid} Size", size_text(size))
      out += row(f"Namespace {nsid} Capacity", size_text(capacity))
    }
    if space.utilization_blocks > 0 {
      out += row(f"Namespace {nsid} Utilization", size_text(space.utilization_blocks * space.lba_size))
    }
    out += row(f"Namespace {nsid} Formatted LBA Size", f"{space.lba_size}")
    if let eui = eui64_text(space.eui64) {
      if serials { out += row(f"Namespace {nsid} IEEE EUI-64", eui) }
    }
  }
  out += row("Local Time is", local_time)
  out
}

pure flag_names(value: Int, names: List[Str]) -> Str {
  var out: List[Str] = []
  for index in range(names.len()) {
    if smart.bit_set(value, index) { out += [names[index]] }
  }
  out.join(" ")
}

pure power_text(units: Int, scale: Int) -> Str {
  # `scale` is 0.0001 W per unit (1) or 0.01 W per unit (2); other values do
  # not describe a power figure.
  if scale == 1 {
    return f"{units / 10000}.{smart.zero_pad(units % 10000, 4)}W"
  }
  f"{units / 100}.{smart.zero_pad(units % 100, 2)}W"
}

## The firmware, optional-command and temperature-threshold lines (`-c`), with
## the power state table and namespace formats.
export pure capabilities_text(ctrl: Controller, ns: Namespace?, nsid: Int, identify: Bytes) -> Str {
  var out = ""
  let slots = ctrl.frmw / 2 % 8
  var firmware = f"{slots} Slot{if slots == 1 { "" } else { "s" }}"
  if smart.bit_set(ctrl.frmw, 0) { firmware += ", Slot 1 R/O" }
  if smart.bit_set(ctrl.frmw, 4) { firmware += ", no Reset required" }
  if smart.bit_set(ctrl.frmw, 5) { firmware += ", multiple detected" }
  if ctrl.frmw / 64 != 0 { firmware += ", *Other*" }
  out += row(f"Firmware Updates (0x{smart.hex(ctrl.frmw, 2)})", firmware)
  out += row(f"Optional Admin Commands (0x{smart.hex(ctrl.oacs, 4)})", flag_names(ctrl.oacs, ["Security", "Format", "Frmw_DL", "NS_Mngmt", "Self_Test", "Directvs", "MI_Snd/Rec", "Vrt_Mngmt", "Drbl_Bf_Cfg", "Get_LBA_Sts"]))
  out += row(f"Optional NVM Commands (0x{smart.hex(ctrl.oncs, 4)})", flag_names(ctrl.oncs, ["Comp", "Wr_Unc", "DS_Mngmt", "Wr_Zero", "Sav/Sel_Feat", "Resv", "Timestmp", "Verify", "Copy"]))
  out += row(f"Log Page Attributes (0x{smart.hex(ctrl.lpa, 2)})", flag_names(ctrl.lpa, ["S/H_per_NS", "Cmd_Eff_Lg", "Ext_Get_Lg", "Telmtry_Lg", "Pers_Ev_Lg", "Log0_FISE_MI", "Telmtry_Ar_4"]))
  out += row("Maximum Data Transfer Size", if ctrl.mdts == 0 { "-" } else { f"{smart.pow2(ctrl.mdts)} Pages" })
  out += row("Warning  Comp. Temp. Threshold", if ctrl.wctemp == 0 { "-" } else { f"{ctrl.wctemp - 273} Celsius" })
  out += row("Critical Comp. Temp. Threshold", if ctrl.cctemp == 0 { "-" } else { f"{ctrl.cctemp - 273} Celsius" })
  if let space = ns {
    if space.features != 0 {
      out += row(f"Namespace {nsid} Features (0x{smart.hex(space.features, 2)})", flag_names(space.features, ["Thin_Prov", "NA_Fields", "Dea/Unw_Error", "No_ID_Reuse", "NP_Fields"]))
    }
  }
  out += "\nSupported Power States\n"
  out += "St Op     Max   Active     Idle   RL RT WL WT  Ent_Lat  Ex_Lat\n"
  for index in range(ctrl.power_state_count) {
    let base = 2048 + index * 32
    continue when base + 32 > identify.len()
    let flags = smart.at(identify, base + 3)
    let max_scale = if smart.bit_set(flags, 0) { 1 } else { 2 }
    let max_power = power_text(smart.le(identify, base, 2), max_scale)
    let idle_scale = smart.at(identify, base + 18) / 64
    let active_scale = smart.at(identify, base + 22) / 64
    let idle_units = smart.le(identify, base + 16, 2)
    let active_units = smart.le(identify, base + 20, 2)
    let idle = if idle_scale == 0 or idle_units == 0 { "-" } else { power_text(idle_units, idle_scale) }
    let active = if active_scale == 0 or active_units == 0 { "-" } else { power_text(active_units, active_scale) }
    let operational = if smart.bit_set(flags, 1) { "-" } else { "+" }
    let latency = f"{smart.pad_start(f"{smart.at(identify, base + 13) % 32}", 3)} {smart.pad_start(f"{smart.at(identify, base + 12) % 32}", 2)} {smart.pad_start(f"{smart.at(identify, base + 15) % 32}", 2)} {smart.pad_start(f"{smart.at(identify, base + 14) % 32}", 2)}"
    out += f"{smart.pad_start(f"{index}", 2)} {operational} {smart.pad_start(max_power, 9)} {smart.pad_start(active, 8)} {smart.pad_start(idle, 8)} {latency} {smart.pad_start(f"{smart.le(identify, base + 4, 4)}", 8)} {smart.pad_start(f"{smart.le(identify, base + 8, 4)}", 7)}\n"
  }
  if let space = ns {
    out += f"\nSupported LBA Sizes (NSID 0x{smart.hex(nsid, 1)})\n"
    out += "Id Fmt  Data  Metadt  Rel_Perf\n"
    for index in range(space.formats.len()) {
      let format = space.formats[index]
      let mark = if index == space.formatted { "+" } else { "-" }
      out += f"{smart.pad_start(f"{index}", 2)} {mark} {smart.pad_start(f"{format.data_bytes}", 7)} {smart.pad_start(f"{format.metadata_size}", 7)} {smart.pad_start(f"{format.relative_performance}", 9)}\n"
    }
  }
  out + "\n"
}

## The reasons a nonzero critical warning byte names, in bit order.
export pure warning_reasons(warning: Int) -> List[Str] {
  var reasons: List[Str] = []
  if smart.bit_set(warning, 0) { reasons += ["- available spare has fallen below threshold"] }
  if smart.bit_set(warning, 1) { reasons += ["- temperature is above or below threshold"] }
  if smart.bit_set(warning, 2) { reasons += ["- NVM subsystem reliability has been degraded"] }
  if smart.bit_set(warning, 3) { reasons += ["- media has been placed in read only mode"] }
  if smart.bit_set(warning, 4) { reasons += ["- volatile memory backup device has failed"] }
  if smart.bit_set(warning, 5) { reasons += ["- persistent memory region has become read-only or unreliable"] }
  if warning / 64 != 0 { reasons += [f"- unknown critical warning(s) (0x{smart.hex(warning / 64 * 64, 2)})"] }
  reasons
}

## The health verdict block: a nonzero critical warning byte is a failure, and
## each bit is explained.
export pure health_text(log: Health) -> Str {
  if log.critical_warning == 0 { return "SMART overall-health self-assessment test result: PASSED\n\n" }
  var out = "SMART overall-health self-assessment test result: FAILED!\n"
  for reason in warning_reasons(log.critical_warning) { out += reason + "\n" }
  out + "\n"
}

## The `SMART/Health Information` block for the namespace id the log was read for.
export pure health_log_text(log: Health, nsid: Int) -> Str {
  var out = f"SMART/Health Information (NVMe Log 0x02, NSID 0x{smart.hex(nsid, 1)})\n"
  out += row("Critical Warning", f"0x{smart.hex(log.critical_warning, 2)}")
  out += row("Temperature", f"{log.temperature} Celsius")
  out += row("Available Spare", f"{log.available_spare}%")
  out += row("Available Spare Threshold", f"{log.spare_threshold}%")
  out += row("Percentage Used", f"{log.percentage_used}%")
  out += row("Data Units Read", f"{smart.thousands(log.data_units_read)} [{smart.capacity_text(log.data_units_read * 512000)}]")
  out += row("Data Units Written", f"{smart.thousands(log.data_units_written)} [{smart.capacity_text(log.data_units_written * 512000)}]")
  out += row("Host Read Commands", smart.thousands(log.host_reads))
  out += row("Host Write Commands", smart.thousands(log.host_writes))
  out += row("Controller Busy Time", smart.thousands(log.busy_minutes))
  out += row("Power Cycles", smart.thousands(log.power_cycles))
  out += row("Power On Hours", smart.thousands(log.power_on_hours))
  out += row("Unsafe Shutdowns", smart.thousands(log.unsafe_shutdowns))
  out += row("Media and Data Integrity Errors", smart.thousands(log.media_errors))
  out += row("Error Information Log Entries", smart.thousands(log.error_entries))
  out += row("Warning  Comp. Temperature Time", f"{log.warning_temp_minutes}")
  out += row("Critical Comp. Temperature Time", f"{log.critical_temp_minutes}")
  for index in range(8) {
    if log.sensors[index] != 0 { out += row(f"Temperature Sensor {index + 1}", f"{log.sensors[index]} Celsius") }
  }
  for index in range(2) {
    if log.thermal_transitions[index] != 0 { out += row(f"Thermal Temp. {index + 1} Transition Count", f"{log.thermal_transitions[index]}") }
  }
  for index in range(2) {
    if log.thermal_minutes[index] != 0 { out += row(f"Thermal Temp. {index + 1} Total Time", f"{log.thermal_minutes[index]}") }
  }
  out + "\n"
}

## The message for a status code type and code of an error log entry.
export pure status_message(status: Int) -> Str {
  let code = status % 256
  let kind = status / 256 % 8
  if kind == 0 {
    match code {
      0 => return "Successful Completion"
      1 => return "Invalid Command Opcode"
      2 => return "Invalid Field in Command"
      3 => return "Command ID Conflict"
      4 => return "Data Transfer Error"
      5 => return "Commands Aborted due to Power Loss Notification"
      6 => return "Internal Error"
      7 => return "Command Abort Requested"
      8 => return "Command Aborted due to SQ Deletion"
      9 => return "Command Aborted due to Failed Fused Command"
      10 => return "Command Aborted due to Missing Fused Command"
      11 => return "Invalid Namespace or Format"
      12 => return "Command Sequence Error"
      128 => return "LBA Out of Range"
      129 => return "Capacity Exceeded"
      130 => return "Namespace Not Ready"
      _ => return "Unknown"
    }
  }
  if kind == 2 {
    match code {
      128 => return "Write Fault"
      129 => return "Unrecovered Read Error"
      130 => return "End-to-end Guard Check Error"
      131 => return "End-to-end Application Tag Check Error"
      132 => return "End-to-end Reference Tag Check Error"
      133 => return "Compare Failure"
      134 => return "Access Denied"
      _ => return "Unknown"
    }
  }
  "Unknown"
}

## The `Error Information` block. `read` entries of `capacity` were fetched.
export pure error_log_text(records: List[ErrorRecord], read: Int, capacity: Int, entries: Int) -> Str {
  var out = f"Error Information (NVMe Log 0x01, {read} of {capacity} entries)\n"
  if entries == 0 and records.is_empty() { return out + "No Errors Logged\n\n" }
  out += "Num   ErrCount  SQId   CmdId  Status  PELoc          LBA  NSID    VS  Message\n"
  for index in range(records.len()) {
    let record = records[index]
    let nsid = if record.nsid == 4294967295 or record.nsid == 0 { "-" } else { f"{record.nsid}" }
    let vendor = if record.vendor == 0 { "-" } else { f"{record.vendor}" }
    let command = f"0x{smart.hex(record.command_id, 4)}"
    let status = f"0x{smart.hex(record.status, 4)}"
    let location = f"0x{smart.hex(record.location, 3)}"
    out += f"{smart.pad_start(f"{index}", 3)} {smart.pad_start(f"{record.count}", 10)} {smart.pad_start(f"{record.queue_id}", 5)} {smart.pad_start(command, 7)} {smart.pad_start(status, 7)} {smart.pad_start(location, 6)} {smart.pad_start(f"{record.lba}", 12)} {smart.pad_start(nsid, 5)} {smart.pad_start(vendor, 5)}  {status_message(record.status)}\n"
  }
  out + "\n"
}

pure self_test_name(code: Int) -> Str {
  match code {
    1 => "Short"
    2 => "Extended"
    14 => "Vendor specific"
    _ => f"Unknown (0x{smart.hex(code, 1)})"
  }
}

pure self_test_result(result: Int) -> Str {
  match result {
    0 => "Completed without error"
    1 => "Aborted: Self-test command"
    2 => "Aborted: Controller Reset"
    3 => "Aborted: Namespace removed"
    4 => "Aborted: Format NVM command"
    5 => "Fatal or unknown test error"
    6 => "Completed: unknown failed segment"
    7 => "Completed: failed segments"
    8 => "Aborted: unknown reason"
    9 => "Aborted: sanitize operation"
    _ => f"Unknown result (0x{smart.hex(result, 1)})"
  }
}

## The `Self-test Log` block for the namespace id the log was read for.
export pure self_test_log_text(log: SelfTestLog, nsid: Int) -> Str {
  var out = f"Self-test Log (NVMe Log 0x06, NSID 0x{smart.hex(nsid, 1)})\n"
  if log.operation == 0 {
    out += "Self-test status: No self-test in progress\n"
  } else {
    out += f"Self-test status: {self_test_progress(log.operation)} ({log.completion}% completed)\n"
  }
  if log.entries.is_empty() { return out + "No Self-tests Logged\n\n" }
  out += "Num  Test_Description  Status                       Power_on_Hours  Failing_LBA  NSID Seg SCT Code\n"
  for index in range(log.entries.len()) {
    let entry = log.entries[index]
    let lba = if smart.bit_set(entry.flags, 1) { f"{entry.lba}" } else { "-" }
    let nsid_text = if smart.bit_set(entry.flags, 0) { f"{entry.nsid}" } else { "-" }
    let sct = if smart.bit_set(entry.flags, 2) { f"0x{smart.hex(entry.sct, 1)}" } else { "-" }
    let sc = if smart.bit_set(entry.flags, 3) { f"0x{smart.hex(entry.sc, 2)}" } else { "-" }
    let segment = if smart.bit_set(entry.flags, 4) or (entry.result >= 5 and entry.result <= 7) { f"{entry.segment}" } else { "-" }
    out += f"{smart.pad_start(f"{index}", 2)}   {smart.pad_end(self_test_name(entry.code), 17)} {smart.pad_end(self_test_result(entry.result), 33)} {smart.pad_start(f"{entry.hours}", 9)} {smart.pad_start(lba, 12)} {smart.pad_start(nsid_text, 5)} {smart.pad_start(segment, 3)} {smart.pad_start(sct, 3)} {smart.pad_start(sc, 4)}\n"
  }
  out + "\n"
}

pure self_test_progress(code: Int) -> Str {
  match code {
    1 => "Short self-test in progress"
    2 => "Extended self-test in progress"
    14 => "Vendor specific self-test in progress"
    _ => f"Unknown (0x{smart.hex(code, 1)}) self-test in progress"
  }
}

## JSON members for controller and namespace identity.
export pure info_json(ctrl: Controller, ns: Namespace?, nsid: Int, serials: Bool) -> List[j.Member] {
  var out: List[j.Member] = [j.m("model_name", ctrl.model)]
  if serials { out += [j.m("serial_number", ctrl.serial)] }
  out += [
    j.m("firmware_version", ctrl.firmware),
    j.m("nvme_pci_vendor", j.object([j.m("id", ctrl.vendor), j.m("subsystem_id", ctrl.subsystem_vendor)])),
    j.m("nvme_ieee_oui_identifier", ctrl.ieee),
  ]
  if ctrl.total_capacity > 0 {
    out += [j.m("nvme_total_capacity", ctrl.total_capacity), j.m("nvme_unallocated_capacity", ctrl.unallocated_capacity)]
  }
  if ctrl.version >= 66048 { out += [j.m("nvme_controller_id", ctrl.controller_id)] }
  out += [j.m("nvme_version", j.object([j.m("string", version_text(ctrl.version)), j.m("value", ctrl.version)]))]
  out += [j.m("nvme_number_of_namespaces", ctrl.namespaces)]
  if let space = ns {
    let size = space.size_blocks * space.lba_size
    var item: List[j.Member] = [
      j.m("id", nsid),
      j.m("size", j.object([j.m("blocks", space.size_blocks), j.m("bytes", size)])),
      j.m("capacity", j.object([j.m("blocks", space.capacity_blocks), j.m("bytes", space.capacity_blocks * space.lba_size)])),
      j.m("utilization", j.object([j.m("blocks", space.utilization_blocks), j.m("bytes", space.utilization_blocks * space.lba_size)])),
      j.m("formatted_lba_size", space.lba_size),
    ]
    if serials and !smart.all_zero(space.eui64, 0, 8) {
      var extension = 0
      for index in range(3, 8) { extension = extension * 256 + smart.at(space.eui64, index) }
      let oui = smart.at(space.eui64, 0) * 65536 + smart.at(space.eui64, 1) * 256 + smart.at(space.eui64, 2)
      item += [j.m("eui64", j.object([j.m("oui", oui), j.m("ext_id", extension)]))]
    }
    out += [j.m("user_capacity", j.object([j.m("blocks", space.size_blocks), j.m("bytes", size)]))]
    out += [j.m("logical_block_size", space.lba_size)]
    out += [j.m("nvme_namespaces", [j.object(item)])]
  }
  out
}

## JSON members for the health log and the summaries derived from it.
export pure health_json(log: Health) -> List[j.Member] {
  var sensors: List[Int] = []
  for temperature in log.sensors { if temperature != 0 { sensors += [temperature] } }
  var members: List[j.Member] = [
    j.m("critical_warning", log.critical_warning),
    j.m("temperature", log.temperature),
    j.m("available_spare", log.available_spare),
    j.m("available_spare_threshold", log.spare_threshold),
    j.m("percentage_used", log.percentage_used),
    j.m("data_units_read", log.data_units_read),
    j.m("data_units_written", log.data_units_written),
    j.m("host_reads", log.host_reads),
    j.m("host_writes", log.host_writes),
    j.m("controller_busy_time", log.busy_minutes),
    j.m("power_cycles", log.power_cycles),
    j.m("power_on_hours", log.power_on_hours),
    j.m("unsafe_shutdowns", log.unsafe_shutdowns),
    j.m("media_errors", log.media_errors),
    j.m("num_err_log_entries", log.error_entries),
    j.m("warning_temp_time", log.warning_temp_minutes),
    j.m("critical_comp_time", log.critical_temp_minutes),
  ]
  if !sensors.is_empty() { members += [j.m("temperature_sensors", sensors)] }
  [
    j.m("nvme_smart_health_information_log", j.object(members)),
    j.m("temperature", j.object([j.m("current", log.temperature)])),
    j.m("power_cycle_count", log.power_cycles),
    j.m("power_on_time", j.object([j.m("hours", log.power_on_hours)])),
  ]
}

## JSON members for the error information log.
export pure error_log_json(records: List[ErrorRecord], read: Int, capacity: Int, entries: Int) -> List[j.Member] {
  var table: List[j.Object] = []
  for record in records {
    table += [j.object([
      j.m("error_count", record.count),
      j.m("submission_queue_id", record.queue_id),
      j.m("command_id", record.command_id),
      j.m("status_field", j.object([j.m("value", record.status), j.m("string", status_message(record.status))])),
      j.m("parm_error_location", record.location),
      j.m("lba", j.object([j.m("value", record.lba)])),
      j.m("nsid", record.nsid),
    ])]
  }
  [j.m("nvme_error_information_log", j.object([
    j.m("size", capacity),
    j.m("read", read),
    j.m("unread", if entries > records.len() { entries - records.len() } else { 0 }),
    j.m("table", table),
  ]))]
}

## JSON members for the self-test log.
export pure self_test_log_json(log: SelfTestLog) -> List[j.Member] {
  var table: List[j.Object] = []
  for entry in log.entries {
    var members: List[j.Member] = [
      j.m("self_test_code", j.object([j.m("value", entry.code), j.m("string", self_test_name(entry.code))])),
      j.m("self_test_result", j.object([j.m("value", entry.result), j.m("string", self_test_result(entry.result))])),
      j.m("power_on_hours", entry.hours),
    ]
    if smart.bit_set(entry.flags, 1) { members += [j.m("lba", entry.lba)] }
    table += [j.object(members)]
  }
  [j.m("nvme_self_test_log", j.object([
    j.m("current_self_test_operation", j.object([
      j.m("value", log.operation),
      j.m("string", if log.operation == 0 { "No self-test in progress" } else { self_test_progress(log.operation) }),
    ])),
    j.m("table", table),
  ]))]
}

## The critical-warning object of the JSON health verdict.
export pure warning_json(warning: Int) -> j.Object {
  j.object([
    j.m("value", warning),
    j.m("spare_below_threshold", smart.bit_set(warning, 0)),
    j.m("temperature_above_or_below_threshold", smart.bit_set(warning, 1)),
    j.m("reliability_degraded", smart.bit_set(warning, 2)),
    j.m("media_read_only", smart.bit_set(warning, 3)),
    j.m("volatile_memory_backup_failed", smart.bit_set(warning, 4)),
    j.m("persistent_memory_region_unreliable", smart.bit_set(warning, 5)),
  ])
}
