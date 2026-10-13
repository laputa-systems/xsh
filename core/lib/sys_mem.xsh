##! Reads the memory counters used by the monitor from a bounded procfs source.
##!
##! The shared meminfo parse retains every valid counter for system-report and
##! projects the byte counters needed by btop from that same observation.
use sys_source as src
use system_report as report

## Separates the byte value from absent, malformed, or truncated source state.
export type MemoryField = {
  state: report.ObservationState,
  bytes: Int?,
  error_kind: Str?,
}

## Retains one validated meminfo value before projecting it into a report field.
export type MemInfoField = {
  name: Str,
  state: report.ObservationState,
  value: Int?,
  unit: Str?,
  error_kind: Str?,
}

type FieldParse = {field: MemInfoField, issue: src.Issue?}
type ProjectedFields = {
  total: MemoryField,
  free: MemoryField,
  available: MemoryField,
  buffers: MemoryField,
  cached: MemoryField,
  active: MemoryField,
  inactive: MemoryField,
  dirty: MemoryField,
  writeback: MemoryField,
  swap_total: MemoryField,
  swap_free: MemoryField,
}

## Holds the canonical meminfo result for both the report and monitor readers.
export type MemorySource = {
  state: report.ObservationState,
  errno: Int?,
  source_error_kind: Str?,
  total: MemoryField,
  free: MemoryField,
  available: MemoryField,
  buffers: MemoryField,
  cached: MemoryField,
  active: MemoryField,
  inactive: MemoryField,
  dirty: MemoryField,
  writeback: MemoryField,
  swap_total: MemoryField,
  swap_free: MemoryField,
  fields: List[MemInfoField],
  counters: List[report.MemoryCounter],
  issues: List[src.Issue],
}

pure empty_field(state: report.ObservationState, error_kind: Str? = null) -> MemoryField {
  {state: state, bytes: null, error_kind: error_kind}
}

pure meminfo_field(
  name: Str,
  state: report.ObservationState,
  value: Int? = null,
  unit: Str? = null,
  error_kind: Str? = null,
) -> MemInfoField {
  {name: name, state: state, value: value, unit: unit, error_kind: error_kind}
}

pure meminfo_issue(field: Str, state: report.ObservationState, error_kind: Str) -> src.Issue {
  src.issue(f"meminfo.{field}", state, error_kind, null)
}

pure error_field(name: Str, error_kind: Str, state: report.ObservationState = report.Malformed) -> MemInfoField {
  meminfo_field(name, state, error_kind: error_kind)
}

pure parse_value(name: Str, values: List[Str]) -> FieldParse {
  if values.is_empty() {
    return {
      field: error_field(name, "missing_integer"),
      issue: meminfo_issue(name, report.Malformed, "missing_integer"),
    }
  }

  if values.len() > 2 {
    return {
      field: error_field(name, "unexpected_meminfo_columns"),
      issue: meminfo_issue(name, report.Malformed, "unexpected_meminfo_columns"),
    }
  }

  let parsed = src.parse_integer(values[0])
  if parsed == null {
    let state = if src.decimal_digits(values[0]) { report.RangeFailure } else { report.Malformed }
    let error_kind = if state == report.RangeFailure { "integer_out_of_range" } else { "invalid_integer" }
    return {
      field: error_field(name, error_kind, state),
      issue: meminfo_issue(name, state, error_kind),
    }
  }

  let parsed_value = parsed ?? -1
  if parsed_value < 0 {
    return {
      field: error_field(name, "negative_integer"),
      issue: meminfo_issue(name, report.Malformed, "negative_integer"),
    }
  }

  let has_kib_unit = values.len() > 1 and values[1] == "kB"
  let maximum_source_value = if has_kib_unit { 8796093022207 } else { 9007199254740991 }
  if parsed_value > maximum_source_value {
    let error_kind = if has_kib_unit { "byte_count_out_of_range" } else { "json_integer_out_of_range" }
    return {
      field: error_field(name, error_kind, report.RangeFailure),
      issue: meminfo_issue(name, report.RangeFailure, error_kind),
    }
  }

  let byte_counter = name in [
    "MemTotal",
    "MemFree",
    "MemAvailable",
    "Buffers",
    "Cached",
    "Active",
    "Inactive",
    "Dirty",
    "Writeback",
    "SwapTotal",
    "SwapFree",
  ]
  if byte_counter and ! has_kib_unit {
    return {
      field: error_field(name, "invalid_byte_counter_unit"),
      issue: meminfo_issue(name, report.Malformed, "invalid_byte_counter_unit"),
    }
  }

  let multiplier = if has_kib_unit { 1024 } else { 1 }
  let value = parsed_value * multiplier
  let unit = if has_kib_unit { "bytes" } else { values.get(1) ?? "count" }
  {field: meminfo_field(name, report.Observed, value, unit), issue: null}
}

pure missing_field(name: Str, fields: List[MemInfoField]) -> MemoryField {
  let matches = fields |> where .name == name
  return empty_field(report.Absent) when matches.is_empty()

  let item = matches[0]
  return empty_field(item.state, item.error_kind) when item.state != report.Observed

  if item.unit != "bytes" {
    return empty_field(report.Malformed, "invalid_byte_counter_unit")
  }

  {state: report.Observed, bytes: item.value, error_kind: null}
}

pure project_fields(state: report.ObservationState, fields: List[MemInfoField]) -> ProjectedFields {
  let fallback = if state == report.Observed { report.Absent } else { state }
  {
    total: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("MemTotal", fields) },
    free: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("MemFree", fields) },
    available: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("MemAvailable", fields) },
    buffers: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("Buffers", fields) },
    cached: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("Cached", fields) },
    active: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("Active", fields) },
    inactive: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("Inactive", fields) },
    dirty: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("Dirty", fields) },
    writeback: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("Writeback", fields) },
    swap_total: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("SwapTotal", fields) },
    swap_free: if fields.is_empty() and fallback != report.Absent { empty_field(fallback) } else { missing_field("SwapFree", fields) },
  }
}

pure source_error(source: src.SourceRead) -> src.Issue {
  src.issue("meminfo", source.observation.state, source.error_kind, source.errno)
}

pure make_source(source: src.SourceRead, fields: List[MemInfoField], issues: List[src.Issue]) -> MemorySource {
  let projected = project_fields(source.observation.state, fields)
  let counters = [
    {name: field.name, value: field.value ?? 0, unit: field.unit ?? "count"}
    for field in fields
    if field.state == report.Observed
  ]
  {
    state: source.observation.state,
    errno: source.errno,
    source_error_kind: source.error_kind,
    total: projected.total,
    free: projected.free,
    available: projected.available,
    buffers: projected.buffers,
    cached: projected.cached,
    active: projected.active,
    inactive: projected.inactive,
    dirty: projected.dirty,
    writeback: projected.writeback,
    swap_total: projected.swap_total,
    swap_free: projected.swap_free,
    fields: fields,
    counters: counters,
    issues: issues,
  }
}

## Parses meminfo once while retaining all counters and per-field failures.
export pure parse(source: src.SourceRead) -> MemorySource {
  if source.observation.state != report.Observed or source.observation.value == null {
    return make_source(source, [], [source_error(source)])
  }

  let text = source.observation.value
  if text.trim() == "" {
    return make_source(
      source,
      [],
      [src.issue("meminfo", report.Malformed, "empty_meminfo", null)],
    )
  }

  let lines = text.lines()
  var seen_names: Set[Str] = set.empty()
  var duplicate_names: Set[Str] = set.empty()
  var issues: List[src.Issue] = []
  for line in lines {
    let pair = line.split(":", maxsplit: 1)
    continue when pair.len() != 2
    let name = pair[0].trim()
    continue when name == ""
    if name in seen_names and name not in duplicate_names {
      duplicate_names = duplicate_names.add(name)
      issues += [meminfo_issue(name, report.Malformed, "duplicate_field")]
    }
    seen_names = seen_names.add(name)
  }

  var fields: List[MemInfoField] = []
  var recorded_duplicates: Set[Str] = set.empty()
  for line_item in lines |> enumerate() {
    let line = line_item.value
    continue when line.trim() == ""
    let pair = line.split(":", maxsplit: 1)
    if pair.len() != 2 {
      issues += [
        src.issue(
          f"meminfo.line.{line_item.index}",
          report.Malformed,
          "missing_field_separator",
          null,
        ),
      ]
      continue
    }

    let name = pair[0].trim()
    if name in duplicate_names {
      if name not in recorded_duplicates {
        fields += [error_field(name, "duplicate_field")]
        recorded_duplicates = recorded_duplicates.add(name)
      }
      continue
    }

    let values = src.parse_words(pair[1].trim().replace("\t", with: " "))
    let parsed = parse_value(name, values)
    fields += [parsed.field]
    if parsed.issue != null {
      issues += [parsed.issue ?? meminfo_issue(name, report.Malformed, "invalid_integer")]
    }
  }

  make_source(source, fields, issues)
}

## Reads /proc/meminfo once and parses only fields needed by the monitor.
export proc collect(root: FsRoot) [fs, error] -> MemorySource {
  parse(src.read_source_text(root, p"proc/meminfo", max_bytes: 1048576))
}
