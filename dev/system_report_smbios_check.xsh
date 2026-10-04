##! Independent bounded reference for the kernel-exported SMBIOS structure table.
use context

error SmbiosCheckError = Invalid(message: Str)

pure smbios_check_failure(message: Str) -> SmbiosCheckError {
  SmbiosCheckError.Invalid(message:)
}

type SmbiosSectionStatus = {state: Str, enumeration_succeeded: Bool}

## Preserves an SMBIOS string's decoded text or exact non-UTF-8 bytes.
export type SmbiosStringReference = {state: Str, value: Str?, raw_bytes_base64: Str?}

## Identifies a supported formatted field by its raw value and source unit.
export type SmbiosFieldReference = {name: Str, value: Int, unit: Str}

## Keeps unknown structure types and handles as distinct raw table records.
export type SmbiosRecordReference = {
  record_type: Int,
  handle: Int,
  formatted_length: Int,
  fields: List[SmbiosFieldReference],
  strings: List[SmbiosStringReference],
}

## Reports whether the bounded raw table contained a complete end marker.
export type SmbiosReference = {records: List[SmbiosRecordReference], complete: Bool, invalid_indices: List[Str]}

## Retains a bounded DMI table read without treating absence as an empty table.
export type SmbiosSourceReference = {data: Bytes?, complete: Bool, absent: Bool}

type CandidateSmbiosRecord = {
  record_type: Int,
  handle: Int,
  formatted_length: Int,
  fields: List[SmbiosFieldReference],
  strings: List[SmbiosStringReference],
}

type CandidateSmbiosSection = {status: SmbiosSectionStatus, source: Str, records: List[CandidateSmbiosRecord]}

## Counts raw SMBIOS record and field disagreements separately from incomplete tables.
export type SmbiosComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  unstable_fields: List[Str],
  eligible: Bool,
  exact: Bool,
}

type SmbiosFieldSpec = {name: Str, offset: Int, width: Int, unit: Str}

## Reads only the kernel-exported DMI structure table within the collector's bound.
export proc read_smbios_reference(root: FsRoot) [fs, error] -> Result[SmbiosSourceReference] {
  let source = root.read_result(p"sys/firmware/dmi/tables/DMI", max_bytes: 1048576)?
  return Ok({data: null, complete: true, absent: true}) when source.state == "absent"

  if source.state != "observed" or source.truncated or source.data == null {
    return Ok({data: null, complete: false, absent: false})
  }

  Ok({data: source.data, complete: true, absent: false})
}

pure smbios_reference_fields(
  data: Bytes,
  offset: Int,
  record_type: Int,
  formatted_length: Int,
) -> Result[List[SmbiosFieldReference]] {
  var fields: List[SmbiosFieldReference] = []
  var specs: List[SmbiosFieldSpec] = []
  if record_type == 0 {
    specs = [
      {
        name: "vendor_index",
        offset: 4,
        width: 1,
        unit: "string_index",
      },
      {
        name: "version_index",
        offset: 5,
        width: 1,
        unit: "string_index",
      },
      {
        name: "release_date_index",
        offset: 8,
        width: 1,
        unit: "string_index",
      },
      {
        name: "rom_size_raw",
        offset: 9,
        width: 1,
        unit: "smbios_raw",
      },
    ]
  } else if record_type == 1 {
    specs = [
      {
        name: "manufacturer_index",
        offset: 4,
        width: 1,
        unit: "string_index",
      },
      {
        name: "product_index",
        offset: 5,
        width: 1,
        unit: "string_index",
      },
      {
        name: "version_index",
        offset: 6,
        width: 1,
        unit: "string_index",
      },
      {
        name: "serial_index",
        offset: 7,
        width: 1,
        unit: "string_index",
      },
      {
        name: "wake_up_type_raw",
        offset: 24,
        width: 1,
        unit: "smbios_raw",
      },
      {
        name: "sku_index",
        offset: 25,
        width: 1,
        unit: "string_index",
      },
      {
        name: "family_index",
        offset: 26,
        width: 1,
        unit: "string_index",
      },
    ]
  } else if record_type == 2 {
    specs = [
      {
        name: "manufacturer_index",
        offset: 4,
        width: 1,
        unit: "string_index",
      },
      {
        name: "product_index",
        offset: 5,
        width: 1,
        unit: "string_index",
      },
      {
        name: "version_index",
        offset: 6,
        width: 1,
        unit: "string_index",
      },
      {
        name: "serial_index",
        offset: 7,
        width: 1,
        unit: "string_index",
      },
      {
        name: "asset_tag_index",
        offset: 8,
        width: 1,
        unit: "string_index",
      },
      {
        name: "board_type_raw",
        offset: 13,
        width: 1,
        unit: "smbios_raw",
      },
    ]
  } else if record_type == 4 {
    specs = [
      {
        name: "socket_designation_index",
        offset: 4,
        width: 1,
        unit: "string_index",
      },
      {
        name: "manufacturer_index",
        offset: 7,
        width: 1,
        unit: "string_index",
      },
      {
        name: "version_index",
        offset: 16,
        width: 1,
        unit: "string_index",
      },
      {
        name: "core_count_raw",
        offset: 23,
        width: 1,
        unit: "smbios_raw",
      },
      {
        name: "core_enabled_raw",
        offset: 24,
        width: 1,
        unit: "smbios_raw",
      },
      {
        name: "thread_count_raw",
        offset: 25,
        width: 1,
        unit: "smbios_raw",
      },
    ]
  } else if record_type == 16 {
    specs = [
      {
        name: "maximum_capacity_raw",
        offset: 7,
        width: 4,
        unit: "smbios_raw",
      },
      {
        name: "number_of_devices",
        offset: 13,
        width: 2,
        unit: "count",
      },
    ]
  } else if record_type == 17 {
    specs = [
      {
        name: "total_width_raw",
        offset: 8,
        width: 2,
        unit: "smbios_raw",
      },
      {
        name: "data_width_raw",
        offset: 10,
        width: 2,
        unit: "smbios_raw",
      },
      {
        name: "size_raw",
        offset: 12,
        width: 2,
        unit: "smbios_raw",
      },
      {
        name: "device_locator_index",
        offset: 16,
        width: 1,
        unit: "string_index",
      },
      {
        name: "bank_locator_index",
        offset: 17,
        width: 1,
        unit: "string_index",
      },
      {
        name: "part_number_index",
        offset: 26,
        width: 1,
        unit: "string_index",
      },
      {
        name: "extended_size_raw",
        offset: 28,
        width: 4,
        unit: "smbios_raw",
      },
    ]
  }

  for spec in specs {
    continue when spec.offset + spec.width > formatted_length
    continue when spec.name == "extended_size_raw" and bytes.unpack_le(data, 2, offset + 12)? != 32767
    fields = fields.push(
      {name: spec.name, value: bytes.unpack_le(data, spec.width, offset + spec.offset)?, unit: spec.unit},
    )
  }

  fields
}

## Independently walks bounded SMBIOS structures by their length and double-null terminator.
export pure parse_smbios_reference(data: Bytes) -> Result[SmbiosReference] {
  var records: List[SmbiosRecordReference] = []
  var invalid_indices: List[Str] = []
  if data.len() > 1048576 {
    return Ok({records: records, complete: false, invalid_indices: invalid_indices})
  }

  var cursor = 0
  var saw_end = false
  while cursor < data.len() {
    if records.len() >= 4096 or cursor + 4 > data.len() {
      return Ok({records: records, complete: false, invalid_indices: invalid_indices})
    }

    let record_type = data.byte_at(cursor) ?? -1
    let length = data.byte_at(cursor + 1) ?? -1
    if length < 4 or cursor + length > data.len() {
      return Ok({records: records, complete: false, invalid_indices: invalid_indices})
    }

    let handle = bytes.unpack_le(data, 2, cursor + 2)?
    let fields = smbios_reference_fields(data, cursor, record_type, length)?
    let strings_start = cursor + length
    var terminator = strings_start
    while terminator + 1 < data.len() and ((data.byte_at(terminator) ?? -1) != 0 or (data.byte_at(terminator + 1) ?? -1) != 0) {
      terminator += 1
    }

    if terminator + 1 >= data.len() {
      return Ok({records: records, complete: false, invalid_indices: invalid_indices})
    }

    var strings: List[SmbiosStringReference] = []
    var string_start = strings_start
    while string_start < terminator {
      var string_end = string_start
      while string_end < terminator and (data.byte_at(string_end) ?? -1) != 0 {
        string_end += 1
      }

      if string_end > string_start {
        let raw = data[string_start..string_end]
        if let Ok(value) = raw.utf8() {
          strings = strings.push({state: "observed", value: value, raw_bytes_base64: null})
        } else {
          strings = strings.push({state: "malformed", value: null, raw_bytes_base64: raw.base64()})
        }
      }

      string_start = string_end + 1
    }

    for field in fields {
      if field.unit == "string_index" and field.value > strings.len() {
        invalid_indices = invalid_indices.push(f"{record_type}:{handle}.{field.name}")
      }
    }

    records = records.push({
      record_type: record_type,
      handle: handle,
      formatted_length: length,
      fields: fields,
      strings: strings,
    })
    cursor = terminator + 2
    if record_type == 127 {
      saw_end = true
      break
    }
  }

  Ok({records: records, complete: saw_end, invalid_indices: invalid_indices})
}

## Compares every stable raw SMBIOS record, including unknown types and string bytes.
export pure compare_smbios(candidate_json: Str, before: Bytes, after: Bytes) -> Result[SmbiosComparison] {
  let data = json.decode(candidate_json)?
  let section = json.get(data, ["firmware"])?.require(CandidateSmbiosSection)?
  let reference = parse_smbios_reference(before)?
  var missing_names: List[Str] = []
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  var unstable_fields: List[Str] = []
  if before == after and reference.complete and section.source != "smbios" {
    field_mismatches += ["source"]
  }

  if before != after {
    unstable_fields += ["table.changed"]
  }

  if ! reference.complete {
    unstable_fields += ["table.incomplete"]
  }

  for invalid in reference.invalid_indices {
    unstable_fields = unstable_fields.push(f"{invalid}.string_index")
  }

  var reference_by_key: Map[Int] = {}
  var candidate_by_key: Map[Int] = {}
  for index in range(reference.records.len()) {
    let item = reference.records[index]
    let key = f"{item.record_type}:{item.handle}"
    if key in reference_by_key {
      return Err(smbios_check_failure("SMBIOS reference repeats a type and handle"))
    }

    reference_by_key = reference_by_key.set(key, index)
  }

  for index in range(section.records.len()) {
    let item = section.records[index]
    let key = f"{item.record_type}:{item.handle}"
    if key in candidate_by_key {
      return Err(smbios_check_failure("candidate SMBIOS report repeats a type and handle"))
    }

    candidate_by_key = candidate_by_key.set(key, index)
  }

  var matched_count = 0
  if before == after and reference.complete {
    for item in reference.records {
      let key = f"{item.record_type}:{item.handle}"
      if key not in candidate_by_key {
        missing_names += [key]
        continue
      }

      matched_count += 1
      let actual = section.records[candidate_by_key.get(key)?]
      if actual.formatted_length != item.formatted_length {
        field_mismatches = field_mismatches.push(f"{key}.formatted_length")
      }

      var actual_fields: Map[Int] = {}
      for field_index in range(actual.fields.len()) {
        let field = actual.fields[field_index]
        if field.name in actual_fields {
          return Err(smbios_check_failure("candidate SMBIOS record repeats a field"))
        }

        actual_fields = actual_fields.set(field.name, field_index)
      }

      for field in item.fields {
        if field.name not in actual_fields {
          field_mismatches = field_mismatches.push(f"{key}.field.{field.name}")
        } else if actual.fields[actual_fields.get(field.name)?] != field {
          field_mismatches = field_mismatches.push(f"{key}.field.{field.name}")
        }
      }

      for field in actual.fields {
        if ! (item.fields |> any .name == field.name) {
          field_mismatches = field_mismatches.push(f"{key}.field.{field.name}")
        }
      }

      if actual.strings.len() != item.strings.len() {
        field_mismatches = field_mismatches.push(f"{key}.strings")
      } else {
        for string_index in range(item.strings.len()) {
          if actual.strings[string_index] != item.strings[string_index] {
            field_mismatches = field_mismatches.push(f"{key}.string.{string_index + 1}")
          }
        }
      }
    }

    for item in section.records {
      let key = f"{item.record_type}:{item.handle}"
      if key not in reference_by_key {
        unexpected_names += [key]
      }
    }
  }

  let eligible = before.len() > 0 or after.len() > 0
  Ok(
    {
      reference_count: reference.records.len(),
      candidate_count: section.records.len(),
      matched_count: matched_count,
      missing_names: missing_names |> sort-by .,
      unexpected_names: unexpected_names |> sort-by .,
      field_mismatches: field_mismatches |> sort-by .,
      unstable_fields: unstable_fields |> sort-by .,
      eligible: eligible,
      exact: eligible and before == after and reference.complete and reference.invalid_indices.len() == 0 and section.source == "smbios" and section.status.enumeration_succeeded and missing_names.len() == 0 and unexpected_names.len() == 0 and field_mismatches.len() == 0,
    },
  )
}

type SmbiosCaptureSource = {
  path: Str,
  state: Str,
  truncated: Bool,
  errno: Int?,
  error_kind: Str?,
  byte_count: Int,
  sha256_hex: Str?,
}

type SmbiosCapture = {
  schema_version: Int,
  origin: Str,
  captured_unix_ms: Int,
  reference_adapter: Str,
  stable: Bool,
  source: SmbiosCaptureSource,
  entry_point: SmbiosCaptureSource,
  reference: SmbiosReference?,
}

type ValidatedSmbiosBundle = {
  reference: SmbiosReference,
  data: Bytes,
  entry_point: Bytes?,
  origin: Str,
  metadata_bytes: Bytes,
}

## Reports whether a bounded raw-table capture can be scored after replay.
export type SmbiosCaptureSummary = {origin: Str, captured_unix_ms: Int, stable: Bool, scoreable: Bool}

type SmbiosCollector = module {
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> Result[Record]
}

type SmbiosReportEncoder = module {
  export pure encode_report_json(report: Record, sensitive: Bool, pretty: Bool) -> Result[Str]
}

## Saves one bounded kernel-exported DMI table and an independently parsed reference.
export proc capture_smbios_bundle(
  source: FsRoot,
  bundle: FsRoot,
  origin: Str,
) [fs, time, error] -> Result[SmbiosCaptureSummary] {
  if origin not in ["synthetic_fixture", "live_capture"] {
    return Err(smbios_check_failure("SMBIOS capture origin must identify a fixture or live capture"))
  }

  let relative = p"sys/firmware/dmi/tables/DMI"
  let entry_relative = p"sys/firmware/dmi/tables/smbios_entry_point"
  if bundle.exists(p"capture.json")? or bundle.exists(relative)? or bundle.exists(
    entry_relative,
  )? {
    return Err(smbios_check_failure("SMBIOS capture destination already contains source data"))
  }

  bundle.mkdir(p"sys/firmware/dmi/tables", mode: 0o700, parents: true)?
  let first = source.read_result(relative, max_bytes: 1048576)?
  let entry_first = source.read_result(entry_relative, max_bytes: 64)?
  if first.data != null {
    bundle.write(relative, first.data)?
  }

  if entry_first.data != null {
    bundle.write(entry_relative, entry_first.data)?
  }

  let second = source.read_result(relative, max_bytes: 1048576)?
  let entry_second = source.read_result(entry_relative, max_bytes: 64)?
  let stable = first.state == second.state and first.truncated == second.truncated and first.errno == second.errno and first.error_kind == second.error_kind and first.data == second.data and entry_first.state == entry_second.state and entry_first.truncated == entry_second.truncated and entry_first.errno == entry_second.errno and entry_first.error_kind == entry_second.error_kind and entry_first.data == entry_second.data
  var reference: SmbiosReference? = null
  if stable and first.state == "observed" and ! first.truncated and first.data != null {
    let parsed = parse_smbios_reference(first.data)?
    if parsed.complete and parsed.invalid_indices.len() == 0 {
      reference = parsed
    }
  }

  let source_observation = SmbiosCaptureSource(
    path: "sys/firmware/dmi/tables/DMI",
    state: first.state,
    truncated: first.truncated,
    errno: first.errno,
    error_kind: first.error_kind,
    byte_count: first.data?.len() ?? 0,
    sha256_hex: if first.data == null {
      null
    } else {
      hash.sha256(first.data).hex()
    },
  )
  let entry_observation = SmbiosCaptureSource(
    path: "sys/firmware/dmi/tables/smbios_entry_point",
    state: entry_first.state,
    truncated: entry_first.truncated,
    errno: entry_first.errno,
    error_kind: entry_first.error_kind,
    byte_count: entry_first.data?.len() ?? 0,
    sha256_hex: if entry_first.data == null {
      null
    } else {
      hash.sha256(entry_first.data).hex()
    },
  )
  let captured_unix_ms = time.now()
  let capture = SmbiosCapture(
    schema_version: 1,
    origin:,
    captured_unix_ms:,
    reference_adapter: "smbios-raw-rooted-v1",
    stable:,
    source: source_observation,
    entry_point: entry_observation,
    reference:,
  )
  let wire: Any = capture
  bundle.write_atomic(p"capture.json", json.encode(wire, pretty: true)?)?
  {origin: origin, captured_unix_ms: captured_unix_ms, stable: stable, scoreable: reference != null}
}

proc validate_smbios_bundle_data(bundle: FsRoot) [fs, error] -> Result[ValidatedSmbiosBundle] {
  let metadata = bundle.read_result(p"capture.json", max_bytes: 2097152)?
  if metadata.state != "observed" or metadata.truncated or metadata.data == null {
    return Err(smbios_check_failure("SMBIOS capture metadata is missing or incomplete"))
  }

  let capture = json.decode(metadata.data.utf8()?)?.require(SmbiosCapture)?
  let relative = p"sys/firmware/dmi/tables/DMI"
  if capture.schema_version != 1 or capture.origin not in ["synthetic_fixture", "live_capture"] or capture.reference_adapter != "smbios-raw-rooted-v1" or ! capture.stable or capture.source.path != "sys/firmware/dmi/tables/DMI" or capture.entry_point.path != "sys/firmware/dmi/tables/smbios_entry_point" or capture.source.state != "observed" or capture.source.truncated or capture.source.errno != null or capture.source.error_kind != null or capture.source.sha256_hex == null or (capture.entry_point.state == "observed" and (capture.entry_point.truncated or capture.entry_point.errno != null or capture.entry_point.error_kind != null or capture.entry_point.sha256_hex == null)) or (capture.entry_point.state == "absent" and capture.entry_point.sha256_hex != null) {
    return Err(smbios_check_failure("SMBIOS capture metadata cannot support exact replay"))
  }

  let raw = bundle.read_result(relative, max_bytes: 1048576)?
  if raw.state != "observed" or raw.truncated or raw.data == null or raw.data.len() != capture.source.byte_count or hash.sha256(
    raw.data,
  )
    .hex() != capture.source.sha256_hex {
    return Err(smbios_check_failure("SMBIOS capture bytes differ from metadata"))
  }

  let entry_relative = p"sys/firmware/dmi/tables/smbios_entry_point"
  var entry_data: Bytes? = null
  if capture.entry_point.sha256_hex == null {
    if capture.entry_point.byte_count != 0 or bundle.exists(entry_relative)? {
      return Err(smbios_check_failure("SMBIOS entry point absence differs from metadata"))
    }
  } else {
    let entry = bundle.read_result(entry_relative, max_bytes: 64)?
    if entry.state != "observed" or entry.truncated or entry.data == null or entry.data.len() != capture.entry_point.byte_count or hash.sha256(
      entry.data,
    )
      .hex() != capture.entry_point.sha256_hex {
      return Err(smbios_check_failure("SMBIOS entry point bytes differ from metadata"))
    }

    if capture.entry_point.state == "observed" and ! capture.entry_point.truncated {
      entry_data = entry.data
    }
  }

  if capture.reference == null {
    return Err(smbios_check_failure("SMBIOS capture has no complete reference table"))
  }

  let reference = parse_smbios_reference(raw.data)?
  if ! reference.complete or reference.invalid_indices.len() > 0 or reference != (capture.reference ?? reference) {
    return Err(smbios_check_failure("SMBIOS capture reference differs from raw table"))
  }

  {
    reference: reference,
    data: raw.data,
    entry_point: entry_data,
    origin: capture.origin,
    metadata_bytes: metadata.data,
  }
}

## Checks saved raw bytes, capture metadata, and the independent SMBIOS oracle.
export proc validate_smbios_bundle(bundle: FsRoot) [fs, error] -> Result[SmbiosReference] {
  validate_smbios_bundle_data(bundle)?.reference
}

pure smbios_checksum_is_zero(data: Bytes, offset: Int, length: Int) -> Bool {
  return false when offset < 0 or length < 0 or offset + length > data.len()

  var sum = 0
  for index in range(offset, offset + length) {
    sum += data.byte_at(index) ?? -1
  }

  sum % 256 == 0
}

## Places a captured DMI table at the offset expected by dmidecode's saved-dump reader.
export pure craft_dmidecode_dump(entry_point: Bytes, table: Bytes) -> Result[Bytes] {
  if entry_point.len() > 32 or table.len() == 0 or table.len() > 1048576 {
    return Err(smbios_check_failure("SMBIOS dump inputs exceed their bounds or lack a table"))
  }

  let is_v3 = entry_point.len() >= 24 and entry_point[..5] == b"_SM3_"
  let is_v2 = entry_point.len() >= 30 and entry_point[..4] == b"_SM_"
  if ! is_v3 and ! is_v2 {
    return Err(smbios_check_failure("SMBIOS entry point has an unsupported signature or length"))
  }

  let entry_length = entry_point.byte_at(if is_v3 { 6 } else { 5 }) ?? -1
  let minimum_length = if is_v3 { 24 } else { 30 }
  let table_capacity = if is_v3 { bytes.unpack_le(entry_point, 4, 12)? } else { bytes.unpack_le(entry_point, 2, 22)? }
  if entry_length < minimum_length or entry_length > entry_point.len() or table_capacity < table.len() or ! smbios_checksum_is_zero(
    entry_point,
    0,
    entry_length,
  ) or (is_v2 and (entry_point[16..21] != b"_DMI_" or ! smbios_checksum_is_zero(entry_point, 16, 15))) {
    return Err(smbios_check_failure("SMBIOS entry point checksum, structure, or table bound is invalid"))
  }

  let address_start = if is_v3 { 16 } else { 24 }
  let address_width = if is_v3 { 8 } else { 4 }
  let checksum_offset = if is_v3 { 5 } else { 21 }
  var prior_address_sum = 0
  for index in range(address_start, address_start + address_width) {
    prior_address_sum += entry_point.byte_at(index) ?? -1
  }

  let relocated_checksum = ((entry_point.byte_at(checksum_offset) ?? -1) + prior_address_sum - 32 + 256) % 256
  var header: List[Int] = []
  for index in range(32) {
    var value = if index < entry_point.len() { entry_point.byte_at(index) ?? -1 } else { 0 }
    if index == checksum_offset {
      value = relocated_checksum
    } else if index == address_start {
      value = 32
    } else if index > address_start and index < address_start + address_width {
      value = 0
    }

    header += [value]
  }

  let patched = bytes.from_ints(header)?
  if ! smbios_checksum_is_zero(patched, 0, entry_length) or (is_v2 and ! smbios_checksum_is_zero(patched, 16, 15)) {
    return Err(smbios_check_failure("SMBIOS entry point relocation produced an invalid checksum"))
  }

  bytes.concat([patched, table])
}

## Holds one record independently decoded from dmidecode's hexadecimal text output.
export type DmidecodeHexRecord = {record_type: Int, handle: Int, formatted: Bytes, strings: List[Bytes]}

## Reports disagreement between a saved raw table and dmidecode's parsed record output.
export type DmidecodeComparison = {
  reference_count: Int,
  decoded_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  exact: Bool,
}

pure dmidecode_hex_row(line: Str) -> Result[List[Int]] {
  let tokens = line.trim().split(" ") |> where . != ""
  return Err(smbios_check_failure("dmidecode hex row is empty")) when tokens.len() == 0

  var octets: List[Int] = []
  for token in tokens {
    guard token.count_chars() == 2 else {
      return Err(smbios_check_failure("dmidecode hex row contains an invalid byte"))
    }

    if let Ok(value) = f"0x{token}".parse_int() {
      if value < 0 or value > 255 {
        return Err(smbios_check_failure("dmidecode hex byte is out of range"))
      }

      octets += [value]
    } else {
      return Err(smbios_check_failure("dmidecode hex row contains a nonhexadecimal byte"))
    }
  }

  octets
}

pure dmidecode_record_from_output(
  record_type: Int,
  handle: Int,
  length: Int,
  formatted_octets: List[Int],
  strings: List[Bytes],
  saw_header: Bool,
) -> Result[DmidecodeHexRecord] {
  if ! saw_header or formatted_octets.len() != length {
    return Err(smbios_check_failure("dmidecode formatted bytes do not match the record length"))
  }

  let formatted = bytes.from_ints(formatted_octets)?
  if length < 4 or (formatted.byte_at(0) ?? -1) != record_type or (formatted.byte_at(1) ?? -1) != length or bytes.unpack_le(
    formatted,
    2,
    2,
  )? != handle {
    return Err(smbios_check_failure("dmidecode formatted header disagrees with its record identity"))
  }

  {record_type: record_type, handle: handle, formatted: formatted, strings: strings}
}

## Parses bounded hexadecimal rows without trusting human-readable field labels.
export pure parse_dmidecode_hex_output(output: Str) -> Result[List[DmidecodeHexRecord]] {
  if output.count_chars() > 8388608 {
    return Err(smbios_check_failure("dmidecode output exceeds the 8 MiB bound"))
  }

  var records: List[DmidecodeHexRecord] = []
  var record_type = -1
  var handle = -1
  var length = -1
  var formatted_octets: List[Int] = []
  var strings: List[Bytes] = []
  var pending_string: List[Int] = []
  var saw_header = false
  var in_formatted = false
  var in_strings = false
  var expect_display = false
  for line in output.lines() {
    let trimmed = line.trim()
    if expect_display {
      expect_display = false
      continue
    }

    if trimmed.starts_with("Handle 0x") {
      if pending_string.len() > 0 {
        return Err(smbios_check_failure("dmidecode string has no terminator"))
      }

      if record_type >= 0 {
        records = records.push(
          dmidecode_record_from_output(record_type, handle, length, formatted_octets, strings, saw_header)?,
        )
      }

      if records.len() >= 4096 {
        return Err(smbios_check_failure("dmidecode output exceeds the record bound"))
      }

      let parts = trimmed.split(", ")
      if parts.len() != 3 or ! parts[1].starts_with("DMI type ") or ! parts[2].ends_with(" bytes") {
        return Err(smbios_check_failure("dmidecode record header is malformed"))
      }

      if let Ok(value) = (parts[0].split(" ").get(1) ?? "").parse_int() {
        handle = value
      } else {
        return Err(smbios_check_failure("dmidecode handle is malformed"))
      }

      if let Ok(value) = (parts[1].split(" ").get(2) ?? "").parse_int() {
        record_type = value
      } else {
        return Err(smbios_check_failure("dmidecode record type is malformed"))
      }

      if let Ok(value) = (parts[2].split(" ").get(0) ?? "").parse_int() {
        length = value
      } else {
        return Err(smbios_check_failure("dmidecode formatted length is malformed"))
      }

      if handle < 0 or handle > 65535 or record_type < 0 or record_type > 255 or length < 4 or length > 255 {
        return Err(smbios_check_failure("dmidecode record header is outside SMBIOS bounds"))
      }

      formatted_octets = []
      strings = []
      saw_header = false
      in_formatted = false
      in_strings = false
      continue
    }

    if trimmed == "Header and Data:" {
      if record_type < 0 or saw_header {
        return Err(smbios_check_failure("dmidecode formatted section is misplaced"))
      }

      saw_header = true
      in_formatted = true
      in_strings = false
      continue
    }

    if trimmed == "Strings:" {
      if ! saw_header or formatted_octets.len() != length {
        return Err(smbios_check_failure("dmidecode strings precede complete formatted bytes"))
      }

      in_formatted = false
      in_strings = true
      continue
    }

    if in_formatted {
      if trimmed == "" {
        in_formatted = false
        continue
      }

      formatted_octets = formatted_octets.extend(dmidecode_hex_row(trimmed)?)
      if formatted_octets.len() > length {
        return Err(smbios_check_failure("dmidecode formatted data exceeds its declared length"))
      }

      continue
    }

    if in_strings {
      if trimmed == "" {
        in_strings = false
        continue
      }

      pending_string = pending_string.extend(dmidecode_hex_row(trimmed)?)
      if pending_string.len() > 1048576 {
        return Err(smbios_check_failure("dmidecode string exceeds its bound"))
      }

      if pending_string[pending_string.len() - 1] == 0 {
        strings = strings.push(bytes.from_ints(pending_string |> take(pending_string.len() - 1))?)
        pending_string = []
        expect_display = true
      }
    }
  }

  if pending_string.len() > 0 or expect_display {
    return Err(smbios_check_failure("dmidecode output ends inside a string"))
  }

  if record_type >= 0 {
    records = records.push(
      dmidecode_record_from_output(record_type, handle, length, formatted_octets, strings, saw_header)?,
    )
  }

  if records.len() == 0 {
    return Err(smbios_check_failure("dmidecode output contains no records"))
  }

  records
}

## Corroborates exact raw fields and string bytes with a separate SMBIOS decoder.
export pure compare_dmidecode_hex_output(reference: SmbiosReference, output: Str) -> Result[DmidecodeComparison] {
  if ! reference.complete or reference.invalid_indices.len() > 0 {
    return Err(smbios_check_failure("incomplete raw SMBIOS tables cannot be corroborated"))
  }

  let decoded = parse_dmidecode_hex_output(output)?
  var reference_by_key: Map[Int] = {}
  var decoded_by_key: Map[Int] = {}
  for index in range(reference.records.len()) {
    let item = reference.records[index]
    let key = f"{item.record_type}:{item.handle}"
    if key in reference_by_key {
      return Err(smbios_check_failure("raw SMBIOS table repeats a record identity"))
    }

    reference_by_key = reference_by_key.set(key, index)
  }

  for index in range(decoded.len()) {
    let item = decoded[index]
    let key = f"{item.record_type}:{item.handle}"
    if key in decoded_by_key {
      return Err(smbios_check_failure("dmidecode output repeats a record identity"))
    }

    decoded_by_key = decoded_by_key.set(key, index)
  }

  var missing_names: List[Str] = []
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  var matched_count = 0
  for item in reference.records {
    let key = f"{item.record_type}:{item.handle}"
    if key not in decoded_by_key {
      missing_names += [key]
      continue
    }

    matched_count += 1
    let actual = decoded[decoded_by_key.get(key)?]
    if actual.formatted.len() != item.formatted_length {
      field_mismatches = field_mismatches.push(f"{key}.formatted_length")
      continue
    }

    let actual_fields = smbios_reference_fields(actual.formatted, 0, actual.record_type, actual.formatted.len())?
    for field in item.fields {
      let matches = actual_fields |> where .name == field.name
      if matches.len() != 1 or matches[0] != field {
        field_mismatches = field_mismatches.push(f"{key}.field.{field.name}")
      }
    }

    for field in actual_fields {
      if ! (item.fields |> any .name == field.name) {
        field_mismatches = field_mismatches.push(f"{key}.field.{field.name}")
      }
    }

    if actual.strings.len() != item.strings.len() {
      field_mismatches = field_mismatches.push(f"{key}.strings")
    } else {
      for string_index in range(item.strings.len()) {
        let raw = actual.strings[string_index]
        let observed: SmbiosStringReference = if let Ok(value) = raw.utf8() {
          {state: "observed", value: value, raw_bytes_base64: null}
        } else {
          {state: "malformed", value: null, raw_bytes_base64: raw.base64()}
        }
        if observed != item.strings[string_index] {
          field_mismatches = field_mismatches.push(f"{key}.string.{string_index + 1}")
        }
      }
    }
  }

  for item in decoded {
    let key = f"{item.record_type}:{item.handle}"
    if key not in reference_by_key {
      unexpected_names += [key]
    }
  }

  {
    reference_count: reference.records.len(),
    decoded_count: decoded.len(),
    matched_count: matched_count,
    missing_names: missing_names |> sort-by .,
    unexpected_names: unexpected_names |> sort-by .,
    field_mismatches: field_mismatches |> sort-by .,
    exact: missing_names.len() == 0 and unexpected_names.len() == 0 and field_mismatches.len() == 0,
  }
}

type DmidecodeRunMetadata = {
  schema_version: Int,
  reference_adapter: Str,
  executable: Str,
  version_argv: List[Str],
  argv: List[Str],
  locale: Str,
  euid: Int,
  origin: Str,
  source_mode: Str,
  table_sha256_hex: Str,
  entry_point_sha256_hex: Str,
  version: Str,
  version_started_unix_ms: Int,
  version_ended_unix_ms: Int,
  version_exit_status: Int,
  started_unix_ms: Int,
  ended_unix_ms: Int,
  exit_status: Int,
  output_bytes: Int,
  output_sha256_hex: Str,
  output_truncated: Bool,
  stderr_bytes: Int,
  stderr_sha256_hex: Str,
  stderr_truncated: Bool,
}

type DmidecodeProbeMetadata = {
  schema_version: Int,
  reference_adapter: Str,
  executable: Str,
  version_argv: List[Str],
  locale: Str,
  euid: Int,
  origin: Str,
  source_mode: Str,
  table_sha256_hex: Str,
  entry_point_sha256_hex: Str,
  version_started_unix_ms: Int,
  version_ended_unix_ms: Int,
  version_exit_status: Int,
  stdout_bytes: Int,
  stdout_sha256_hex: Str,
  stdout_truncated: Bool,
  stderr_bytes: Int,
  stderr_sha256_hex: Str,
  stderr_truncated: Bool,
}

## Keeps utility provenance separate from the mandatory raw-table comparison.
export type DmidecodeCorroboration = {
  comparison: DmidecodeComparison,
  version: Str,
  started_unix_ms: Int,
  ended_unix_ms: Int,
}

## Runs an explicitly selected dmidecode only on validated captured firmware bytes.
export proc corroborate_smbios_bundle(
  bundle: FsRoot,
  executable: Str,
) [fs, process, time, error] -> Result[DmidecodeCorroboration] {
  guard executable.starts_with("/") else {
    return Err(smbios_check_failure("dmidecode executable must be an absolute path"))
  }

  for name in [
    "dmidecode-version.txt",
    "dmidecode-version-error.txt",
    "dmidecode-probe.json",
    "dmidecode-output.txt",
    "dmidecode-stderr.txt",
    "dmidecode-reference.json",
    "dmidecode-comparison.json",
  ] {
    if bundle.exists(fp"{name}")? {
      return Err(smbios_check_failure("SMBIOS bundle already contains dmidecode corroboration"))
    }
  }

  let validated = validate_smbios_bundle_data(bundle)?
  if validated.entry_point == null {
    return Err(smbios_check_failure("SMBIOS bundle has no complete entry point for dmidecode"))
  }

  let entry_point = validated.entry_point
  let dump = craft_dmidecode_dump(entry_point, validated.data)?
  let scratch = fs.tempdir()?
  defer scratch.close()?
  scratch.write(p"dump.bin", dump)?
  scratch.write(p"version", "")?
  scratch.write(p"version-error", "")?
  scratch.write(p"output", "")?
  scratch.write(p"error", "")?
  let scratch_path = scratch.host_path()?
  let version_argv = [executable, "--version"]
  let version_started = time.now()
  let version_status = process.run(
    process.command_argv(
      executable,
      version_argv,
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"{scratch_path}/version",
      stderr: fp"{scratch_path}/version-error",
    ),
  )?
  let version_ended = time.now()
  let version_source = scratch.read_result(p"version", max_bytes: 4096)?
  let version_error = scratch.read_result(p"version-error", max_bytes: 65536)?
  let version_exit = version_status.exit_code() ?? -1
  let version_bytes = version_source.data ?? b""
  let version_error_bytes = version_error.data ?? b""
  bundle.write_atomic(p"dmidecode-version.txt", version_bytes)?
  bundle.write_atomic(p"dmidecode-version-error.txt", version_error_bytes)?
  let probe = DmidecodeProbeMetadata(
    schema_version: 1,
    reference_adapter: "dmidecode-hex-v1",
    executable:,
    version_argv:,
    locale: "C",
    euid: applet.current_euid(),
    origin: validated.origin,
    source_mode: "captured_replay",
    table_sha256_hex: hash.sha256(validated.data).hex(),
    entry_point_sha256_hex: hash.sha256(entry_point).hex(),
    version_started_unix_ms: version_started,
    version_ended_unix_ms: version_ended,
    version_exit_status: version_exit,
    stdout_bytes: version_bytes.len(),
    stdout_sha256_hex: hash.sha256(version_bytes).hex(),
    stdout_truncated: version_source.truncated,
    stderr_bytes: version_error_bytes.len(),
    stderr_sha256_hex: hash.sha256(version_error_bytes).hex(),
    stderr_truncated: version_error.truncated,
  )
  let probe_wire: Any = probe
  bundle.write_atomic(p"dmidecode-probe.json", json.encode(probe_wire, pretty: true)?)?
  if version_exit != 0 or version_source.state != "observed" or version_source.truncated or version_source.data == null or version_error.truncated {
    return Err(smbios_check_failure("dmidecode version probe failed"))
  }

  let version = version_source.data.utf8()?.trim()
  if version == "" {
    return Err(smbios_check_failure("dmidecode version probe returned no version"))
  }

  let dump_path = fp"{scratch_path}/dump.bin"
  let argv = [executable, "--no-quirks", "--dump", "--from-dump", dump_path.display()]
  let started = time.now()
  let status = process.run(
    process.command_argv(
      executable,
      argv,
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"{scratch_path}/output",
      stderr: fp"{scratch_path}/error",
    ),
  )?
  let ended = time.now()
  let output = scratch.read_result(p"output", max_bytes: 8388608)?
  let stderr = scratch.read_result(p"error", max_bytes: 65536)?
  let exit_status = status.exit_code() ?? -1
  let output_bytes = output.data ?? b""
  let stderr_bytes = stderr.data ?? b""
  bundle.write_atomic(p"dmidecode-output.txt", output_bytes)?
  bundle.write_atomic(p"dmidecode-stderr.txt", stderr_bytes)?
  let metadata = DmidecodeRunMetadata(
    schema_version: 1,
    reference_adapter: "dmidecode-hex-v1",
    executable:,
    version_argv:,
    argv:,
    locale: "C",
    euid: applet.current_euid(),
    origin: validated.origin,
    source_mode: "captured_replay",
    table_sha256_hex: hash.sha256(validated.data).hex(),
    entry_point_sha256_hex: hash.sha256(entry_point).hex(),
    version:,
    version_started_unix_ms: version_started,
    version_ended_unix_ms: version_ended,
    version_exit_status: version_exit,
    started_unix_ms: started,
    ended_unix_ms: ended,
    exit_status:,
    output_bytes: output_bytes.len(),
    output_sha256_hex: hash.sha256(output_bytes).hex(),
    output_truncated: output.truncated,
    stderr_bytes: stderr_bytes.len(),
    stderr_sha256_hex: hash.sha256(stderr_bytes).hex(),
    stderr_truncated: stderr.truncated,
  )
  let metadata_wire: Any = metadata
  bundle.write_atomic(p"dmidecode-reference.json", json.encode(metadata_wire, pretty: true)?)?
  if exit_status != 0 or output.state != "observed" or output.truncated or stderr.state != "observed" or stderr.truncated {
    return Err(smbios_check_failure("dmidecode reference failed or exceeded its output bound"))
  }

  let after = validate_smbios_bundle_data(bundle)?
  if after.data != validated.data or after.entry_point != validated.entry_point or after.metadata_bytes != validated.metadata_bytes {
    return Err(smbios_check_failure("SMBIOS capture changed during dmidecode corroboration"))
  }

  let compared = compare_dmidecode_hex_output(validated.reference, output_bytes.utf8()?)?
  let comparison_wire: Any = compared
  bundle.write_atomic(p"dmidecode-comparison.json", json.encode(comparison_wire, pretty: true)?)?
  {comparison: compared, version: version, started_unix_ms: started, ended_unix_ms: ended}
}

## Re-runs the production firmware collector on the saved raw table.
export proc replay_smbios_bundle(bundle: FsRoot) [fs, time, error] -> Result[SmbiosComparison] {
  let validated = validate_smbios_bundle_data(bundle)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SmbiosCollector)?
  let candidate = collector.collect_from_root(bundle, "captured-architecture", 4096, 100, "firmware", true)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SmbiosReportEncoder)?
  let candidate_json = model.encode_report_json(candidate, true, false)?
  let after = validate_smbios_bundle_data(bundle)?
  if after.data != validated.data or after.entry_point != validated.entry_point or after.metadata_bytes != validated.metadata_bytes {
    return Err(smbios_check_failure("SMBIOS capture changed during replay"))
  }

  let compared = compare_smbios(candidate_json, validated.data, after.data)?
  if compared.reference_count != validated.reference.records.len() {
    return Err(smbios_check_failure("SMBIOS replay reference count changed"))
  }

  compared
}

## Reports whether a stable SMBIOS table was scored, partial, or unavailable.
export type SmbiosLiveOutcome = {scored: Bool, partial: Bool, unavailable: Bool}

pure smbios_require_live_report(candidate_json: Str) -> Result[Unit] {
  let data = json.decode(candidate_json)?
  let source_mode = json.get(data, ["source_mode"])?.require(Str?)?
  if source_mode != "live_linux" {
    return Err(smbios_check_failure("candidate is not a live Linux report"))
  }
}

## Brackets the kernel-exported DMI table around a sensitive firmware report.
export proc compare_live_smbios(xsh_bin: Str, script: Str) [fs, process, time, error, io] -> Result[SmbiosLiveOutcome] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") {
    return Err(smbios_check_failure("--xsh-bin and --script must be absolute paths"))
  }

  let source = fs.open_root(/)?
  defer source.close()?
  let scratch = fs.tempdir()?
  defer scratch.close()?
  let before_started = time.now()
  let before = read_smbios_reference(source)?
  let before_ended = time.now()
  let scratch_path = scratch.host_path()?
  scratch.write(p"candidate", "")?
  let candidate_started = time.now()
  let status = process.run(
    process.command_argv(
      xsh_bin,
      [xsh_bin, script, "--", "--section", "firmware", "--sensitive", "--json"],
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"{scratch_path}/candidate",
    ),
  )?
  let candidate_ended = time.now()
  if ! status.exited_with(0) {
    return Err(smbios_check_failure("candidate SMBIOS collection failed"))
  }

  let after_started = time.now()
  let after = read_smbios_reference(source)?
  let after_ended = time.now()
  let candidate = scratch.read_text(p"candidate")?
  smbios_require_live_report(candidate)?
  let data = json.decode(candidate)?
  let host_claim = json.get(data, ["scope", "host_claim"])?.require(Str)?
  if ! before.complete or ! after.complete or before.absent != after.absent {
    print "firmware.smbios: raw table read incomplete or changed; comparison remains partial"
    return Ok({scored: false, partial: true, unavailable: false})
  }

  if before.absent and after.absent {
    print "firmware.smbios: kernel-exported DMI table absent; reference unavailable"
    return Ok({scored: false, partial: false, unavailable: true})
  }

  let compared = compare_smbios(candidate, before.data ?? b"", after.data ?? b"")?
  print f"firmware.smbios: reference={compared.reference_count}, candidate={compared.candidate_count}, matched={compared.matched_count}, missing={compared.missing_names.len()}, unexpected={compared.unexpected_names.len()}, mismatched={compared.field_mismatches.len()}, changed_or_incomplete={compared.unstable_fields.len()}, exposed={compared.eligible}, exact={compared.exact}"
  print f"reference: adapter=smbios-raw-rooted-v1; source=/sys/firmware/dmi/tables/DMI; bound=1 MiB, 4096 records; locale=C; euid={applet.current_euid()}; source_mode=live_linux; host_claim={json.encode(host_claim)?}; before={before_started}..{before_ended} ms; candidate={candidate_started}..{candidate_ended} ms; after={after_started}..{after_ended} ms"
  if compared.missing_names.len() > 0 or compared.unexpected_names.len() > 0 or compared.field_mismatches.len() > 0 {
    return Err(smbios_check_failure("SMBIOS records differ from the stable kernel-exported table"))
  }

  Ok({scored: compared.exact, partial: compared.eligible and ! compared.exact, unavailable: ! compared.eligible})
}
