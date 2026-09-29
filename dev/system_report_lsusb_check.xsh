##! Optional usbutils corroboration of visible USB identity and descriptors.
error LsusbCheckError = Invalid(message: Str)

pure lsusb_failure(message: Str) -> LsusbCheckError {
  return LsusbCheckError.Invalid(message:)
}

## Retains numeric identity without depending on the installed USB ID database.
export type LsusbDevice = {bus: Int, device: Int, vendor_id: Int, product_id: Int}

## Retains one displayed port and optional interface driver from `lsusb -t`.
export type LsusbTreeRow = {bus: Int, device: Int, port: Int, interface_number: Int?, driver: Str?}

## Retains selected descriptor fields from one bounded verbose capture.
export type LsusbDescriptor = {
  bus: Int,
  device: Int,
  vendor_id: Int,
  product_id: Int,
  class_code: Int,
  configuration_count: Int,
}

type CandidateInterface = {number: Int, driver: Str?}

type CandidateDevice = {
  bus_number: Int?,
  device_number: Int?,
  vendor_id: Int?,
  product_id: Int?,
  class_code: Int?,
  configuration_count: Int?,
  port_path: Str?,
  interfaces: List[CandidateInterface],
}

type CandidateUsb = {devices: List[CandidateDevice]}

type CandidateReport = {usb: CandidateUsb}

## Separates exact identity matches from unscoreable device and interface fields.
export type LsusbComparison = {
  matched_devices: Int,
  matched_tree_rows: Int,
  matched_descriptor_fields: Int,
  mismatches: List[Str],
  partial: List[Str],
}

## Records selected commands and raw-output fingerprints for an opt-in live run.
export type LsusbRun = {
  version: Str,
  executable: Str,
  comparison: LsusbComparison,
  reference_started_unix_ms: Int,
  candidate_started_unix_ms: Int,
  list_sha256_hex: Str,
  tree_sha256_hex: Str,
  verbose_sha256_hex: Str,
}

pure decimal(value: Str) -> Result[Int] {
  if value == "" {
    return Err(lsusb_failure("USB reference has an empty decimal identity"))
  }

  for digit in value.split("") {
    if digit not in [
      "0",
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
    ] {
      return Err(lsusb_failure("USB reference has a nondecimal identity"))
    }
  }

  return value.parse_int()
}

pure hex4(value: Str) -> Result[Int] {
  if value.byte_len() != 4 {
    return Err(lsusb_failure("USB reference has a noncanonical hex ID"))
  }

  for digit in value.split("") {
    if digit not in [
      "0",
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
      "a",
      "b",
      "c",
      "d",
      "e",
      "f",
      "A",
      "B",
      "C",
      "D",
      "E",
      "F",
    ] {
      return Err(lsusb_failure("USB reference has a nonhex ID"))
    }
  }

  return f"0x${value}".parse_int()
}

pure words(value: Str) -> List[Str] {
  return value.split(" ") |> where . != ""
}

## Parses stable `Bus NNN Device NNN: ID VVVV:PPPP` prefixes only.
export pure parse_lsusb_list(output: Str) -> Result[List[LsusbDevice]] {
  if output.byte_len() > 1048576 {
    return Err(lsusb_failure("lsusb list output exceeds its bound"))
  }

  var devices: List[LsusbDevice] = []
  var seen = set.empty()
  for line in output.lines() {
    continue when line.trim() == ""
    let fields = words(line.trim())
    if fields.len() < 6 or fields[0] != "Bus" or fields[2] != "Device" or ! fields[3].ends_with(":") or fields[4] != "ID" {
      return Err(lsusb_failure("lsusb list row has an unsupported shape"))
    }

    let bus = decimal(fields[1])?
    let device = decimal(fields[3].byte_slice(0, length: fields[3].byte_len() - 1))?
    let ids = fields[5].split(":")
    if ids.len() != 2 {
      return Err(lsusb_failure("lsusb list row has invalid IDs"))
    }

    let key = f"${bus}:${device}"
    if bus <= 0 or device <= 0 or set.has(seen, key) {
      return Err(lsusb_failure("lsusb list has duplicate or invalid device identity"))
    }

    seen = set.add(seen, key)
    devices = devices.push({bus: bus, device: device, vendor_id: hex4(ids[0])?, product_id: hex4(ids[1])?})
  }

  return devices |> sort-by f"${.bus}:${.device}"
}

pure field_after(fields: List[Str], name: Str) -> Str? {
  for index in range(fields.len()) {
    if fields[index] == name and index + 1 < fields.len() {
      return fields[index + 1]
    }
  }

  return null
}

## Keeps the current root bus while parsing child interface rows.
export pure parse_lsusb_tree(output: Str) -> Result[List[LsusbTreeRow]] {
  if output.byte_len() > 1048576 {
    return Err(lsusb_failure("lsusb tree output exceeds its bound"))
  }

  var bus = 0
  var rows: List[LsusbTreeRow] = []
  for line in output.lines() {
    continue when line.trim() == ""
    let fields = words(line.trim().replace(",", "").replace(":", " "))
    let bus_token = field_after(fields, "Bus")
    var root_port: Str? = null
    if bus_token != null {
      let parts = bus_token.split(".Port")
      if parts.len() != 2 {
        return Err(lsusb_failure("lsusb tree root bus is malformed"))
      }

      bus = decimal(parts[0])?
      root_port = field_after(fields, bus_token)
    }

    let dev_token = field_after(fields, "Dev")
    let port_token = if root_port == null { field_after(fields, "Port") } else { root_port }
    if bus <= 0 or dev_token == null or port_token == null {
      return Err(lsusb_failure("lsusb tree row lacks bus, port, or device"))
    }

    let interface_token = field_after(fields, "If")
    var interface_number: Int? = null
    if interface_token != null {
      interface_number = decimal(interface_token)?
    }

    var driver: Str? = null
    for token in fields {
      if token.starts_with("Driver=") {
        driver = token.byte_slice(7).split("/")[0]
      }
    }

    rows = rows.push({
      bus: bus,
      device: decimal(dev_token ?? "")?,
      port: decimal(port_token ?? "")?,
      interface_number: interface_number,
      driver: driver,
    })
  }

  return rows
}

pure verbose_number(output: Str, key: Str, hex: Bool) -> Result[Int] {
  var values: List[Int] = []
  for line in output.lines() {
    let fields = words(line.trim())
    continue when fields.len() < 2 or fields[0] != key
    var value = 0
    if hex {
      if ! fields[1].starts_with("0x") {
        return Err(lsusb_failure(f"lsusb verbose ${key} lacks hex value"))
      }

      value = hex4(fields[1].byte_slice(2))?
    } else {
      value = decimal(fields[1])?
    }

    values = values.push(value)
  }

  if values.len() != 1 {
    return Err(lsusb_failure(f"lsusb verbose ${key} is absent or ambiguous"))
  }

  return values[0]
}

## Reads one device descriptor without interpreting configuration or interface text tables.
export pure parse_lsusb_verbose(output: Str, bus: Int, device: Int) -> Result[LsusbDescriptor] {
  if output.byte_len() > 1048576 {
    return Err(lsusb_failure("lsusb verbose output exceeds its bound"))
  }

  var headers: List[LsusbDevice] = []
  for line in output.lines() {
    if line.starts_with("Bus ") {
      headers = headers.extend(parse_lsusb_list(line)?)
    }
  }

  if headers.len() != 1 or headers[0].bus != bus or headers[0].device != device {
    return Err(lsusb_failure("lsusb verbose output belongs to another device"))
  }

  let vendor_id = verbose_number(output, "idVendor", true)?
  let product_id = verbose_number(output, "idProduct", true)?
  if vendor_id != headers[0].vendor_id or product_id != headers[0].product_id {
    return Err(lsusb_failure("lsusb verbose descriptor IDs disagree with its header"))
  }

  return {
    bus: bus,
    device: device,
    vendor_id: vendor_id,
    product_id: product_id,
    class_code: verbose_number(output, "bDeviceClass", false)?,
    configuration_count: verbose_number(output, "bNumConfigurations", false)?,
  }
}

## Compares utility identities, displayed interfaces, and one selected descriptor.
export pure compare_lsusb(
  candidate_json: Str,
  devices: List[LsusbDevice],
  tree: List[LsusbTreeRow],
  descriptor: LsusbDescriptor,
) -> Result[LsusbComparison] {
  let report = json.decode(candidate_json)?.require(CandidateReport)?
  var candidate_by_key: Map[Int] = {}
  var mismatches: List[Str] = []
  var partial: List[Str] = []
  var matched_devices = 0
  var matched_tree_rows = 0
  var matched_descriptor_fields = 0
  for index in range(report.usb.devices.len()) {
    let item = report.usb.devices[index]
    if item.bus_number == null or item.device_number == null {
      partial = partial.push("candidate_bus_device")
      continue
    }

    let key = f"${item.bus_number ?? 0}:${item.device_number ?? 0}"
    if candidate_by_key.has(key) {
      mismatches = mismatches.push(f"${key}.duplicate")
    }

    candidate_by_key = candidate_by_key.set(key, index)
  }

  var basic_seen = set.empty()
  for reference in devices {
    let key = f"${reference.bus}:${reference.device}"
    basic_seen = set.add(basic_seen, key)
    if ! candidate_by_key.has(key) {
      mismatches = mismatches.push(f"${key}.missing")
      continue
    }

    let candidate = report.usb.devices[candidate_by_key.get(key)?]
    if candidate.vendor_id != reference.vendor_id or candidate.product_id != reference.product_id {
      mismatches = mismatches.push(f"${key}.ids")
    } else {
      matched_devices += 1
    }
  }

  for key in candidate_by_key.keys() {
    if ! set.has(basic_seen, key) {
      mismatches = mismatches.push(f"${key}.unexpected")
    }
  }

  var tree_seen = set.empty()
  for row in tree {
    let key = f"${row.bus}:${row.device}"
    tree_seen = set.add(tree_seen, key)
    if ! candidate_by_key.has(key) {
      mismatches = mismatches.push(f"${key}.tree_missing")
      continue
    }

    let candidate = report.usb.devices[candidate_by_key.get(key)?]
    if row.interface_number == null {
      matched_tree_rows += 1
      continue
    }

    let port_parts = (candidate.port_path ?? "").split(".")
    if (decimal(port_parts.get(port_parts.len() - 1, "")) ?? -1) != row.port {
      mismatches = mismatches.push(f"${key}.port")
      continue
    }

    let interfaces = candidate.interfaces |> where .number == (row.interface_number ?? -1)
    if interfaces.len() != 1 {
      mismatches = mismatches.push(f"${key}.interface")
      continue
    }

    if row.driver != null and interfaces[0].driver != row.driver {
      mismatches = mismatches.push(f"${key}.driver")
    } else {
      matched_tree_rows += 1
    }
  }

  for reference in devices {
    if ! set.has(tree_seen, f"${reference.bus}:${reference.device}") {
      mismatches = mismatches.push(f"${reference.bus}:${reference.device}.tree_absent")
    }
  }

  let selected_key = f"${descriptor.bus}:${descriptor.device}"
  if ! candidate_by_key.has(selected_key) {
    mismatches = mismatches.push(f"${selected_key}.descriptor_absent")
  } else {
    let selected = report.usb.devices[candidate_by_key.get(selected_key)?]
    for pair in [
      {
        name: "vendor",
        actual: selected.vendor_id,
        expected: descriptor.vendor_id,
      },
      {
        name: "product",
        actual: selected.product_id,
        expected: descriptor.product_id,
      },
      {
        name: "class",
        actual: selected.class_code,
        expected: descriptor.class_code,
      },
      {
        name: "configurations",
        actual: selected.configuration_count,
        expected: descriptor.configuration_count,
      },
    ] {
      if pair.actual == null {
        partial = partial.push(f"${selected_key}.${pair.name}")
      } else if pair.actual != pair.expected {
        mismatches = mismatches.push(f"${selected_key}.${pair.name}")
      } else {
        matched_descriptor_fields += 1
      }
    }
  }

  return {
    matched_devices: matched_devices,
    matched_tree_rows: matched_tree_rows,
    matched_descriptor_fields: matched_descriptor_fields,
    mismatches: mismatches |> sort-by .,
    partial: partial |> sort-by .,
  }
}

proc lsusb_output(root: FsRoot, executable: Str, name: Str, argv: List[Str]) [fs, process, error] -> Result[Str] {
  let scratch_path = root.host_path()?
  let status = process.run(
    process.command_argv(
      executable,
      argv,
      cwd: /,
      env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/${name}",
      stderr: fp"${scratch_path}/${name}-error",
    ),
  )?
  if ! status.exited_with(0) {
    return Err(lsusb_failure(f"lsusb ${name} command failed"))
  }

  let raw = root.read_result(fp"${name}", max_bytes: 1048576)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(lsusb_failure(f"lsusb ${name} output is incomplete"))
  }

  return (raw.data ?? b"").utf8()?
}

## Selects one verbose device from the independent list and brackets identity and tree shape.
export proc compare_live_lsusb(
  xsh_bin: Str,
  script: Str,
  executable: Str,
) [fs, process, time, error] -> Result[LsusbRun] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") or ! executable.starts_with("/") {
    return Err(lsusb_failure("lsusb comparison requires absolute paths"))
  }

  let scratch = fs.tempdir()?
  defer scratch.close()?
  let version = lsusb_output(scratch, executable, "version", [executable, "--version"])?.lines().get(0, "").trim()
  if ! version.starts_with("lsusb ") {
    return Err(lsusb_failure("lsusb version is unsupported"))
  }

  let started = time.now()
  let before_output = lsusb_output(scratch, executable, "before", [executable])?
  let before = parse_lsusb_list(before_output)?
  if before.len() == 0 {
    return Err(lsusb_failure("lsusb has no selectable device"))
  }

  let tree_output = lsusb_output(scratch, executable, "tree", [executable, "-t"])?
  let tree = parse_lsusb_tree(tree_output)?
  let selected = before[0]
  let selector = f"${selected.bus}:${selected.device}"
  let verbose_output = lsusb_output(scratch, executable, "verbose", [executable, "-v", "-s", selector])?
  let descriptor = parse_lsusb_verbose(verbose_output, selected.bus, selected.device)?
  let candidate_started = time.now()
  let scratch_path = scratch.host_path()?
  let status = process.run(
    process.command_argv(
      xsh_bin,
      [xsh_bin, script, "--", "--section", "usb", "--sensitive", "--json"],
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
      stdout: fp"${scratch_path}/candidate",
      stderr: fp"${scratch_path}/candidate-error",
    ),
  )?
  if ! status.exited_with(0) {
    return Err(lsusb_failure("candidate USB collection failed"))
  }

  let candidate_raw = scratch.read_result(p"candidate", max_bytes: 8388608)?
  if candidate_raw.state != "observed" or candidate_raw.truncated or candidate_raw.data == null {
    return Err(lsusb_failure("candidate USB output is incomplete"))
  }

  let candidate = (candidate_raw.data ?? b"").utf8()?
  if json.get(json.decode(candidate)?, ["source_mode"])?.require(Str)? != "live_linux" {
    return Err(lsusb_failure("candidate is not a live Linux report"))
  }

  let after = parse_lsusb_list(lsusb_output(scratch, executable, "after", [executable])?)?
  let after_tree = parse_lsusb_tree(lsusb_output(scratch, executable, "after-tree", [executable, "-t"])?)?
  if before != after or tree != after_tree {
    return Err(lsusb_failure("lsusb inventory changed around candidate collection"))
  }

  return {
    version: version,
    executable: executable,
    comparison: compare_lsusb(candidate, before, tree, descriptor)?,
    reference_started_unix_ms: started,
    candidate_started_unix_ms: candidate_started,
    list_sha256_hex: hash.sha256(fp"${scratch_path}/before")?.hex(),
    tree_sha256_hex: hash.sha256(fp"${scratch_path}/tree")?.hex(),
    verbose_sha256_hex: hash.sha256(fp"${scratch_path}/verbose")?.hex(),
  }
}
