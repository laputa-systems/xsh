##! Bounded source reads and observation helpers shared by the `sys_*` collectors.
##!
##! Every collector reads through a caller-owned `FsRoot`, so the same code serves
##! the live host, fixtures, and captured replay trees. A read never publishes a
##! value from a truncated, unreadable, or non-UTF-8 source: the observation state
##! says why a value is missing. The state vocabulary is `system_report`'s
##! `ObservationState`, which keeps collector output and the report model aligned.
use system_report as report

## Retains a source observation and stable read-error details.
export type SourceRead = {
  observation: report.TextObservation,
  errno: Int?,
  error_kind: Str?,
}

## Retains a bounded integer or the source, syntax, or range state that withheld it.
export type BoundedNumber = {value: Int?, state: report.ObservationState?, error_kind: Str?, errno: Int?}

## Describes one field-addressed collection problem without choosing a report section.
export type Issue = {
  field: Str,
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
  detail: report.TextObservation,
}

## Keeps a class device's parent target and source failure state together.
export type ClassParentObservation = {
  target: Path?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

## Maps a rooted read state onto the observation vocabulary.
export pure source_state(state: Str, truncated: Bool) -> report.ObservationState {
  return report.Truncated when truncated

  match state {
    "observed" => report.Observed
    "absent" => report.Absent
    "permission_denied" => report.PermissionDenied
    else => report.ReadFailure
  }
}

## Builds a text observation that carries only a state.
export pure empty_text(state: report.ObservationState) -> report.TextObservation {
  {state: state, value: null, raw_bytes_base64: null}
}

## Builds an issue whose detail carries only its state.
export pure issue(field: Str, state: report.ObservationState, error_kind: Str?, errno: Int?) -> Issue {
  {
    field: field,
    state: state,
    error_kind: error_kind,
    errno: errno,
    detail: empty_text(state),
  }
}

## Builds an issue with an explanatory detail line.
export pure issue_with_detail(
  field: Str,
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
  detail: Str,
) -> Issue {
  {
    ...issue(field, state, error_kind, errno),
    detail: {
      state: report.Observed,
      value: detail,
      raw_bytes_base64: null,
    },
  }
}

## Appends an issue for a source that was neither observed nor absent.
export pure append_text_issue(issues: List[Issue], field: Str, source: SourceRead) -> List[Issue] {
  let state = source.observation.state
  return issues when state == .Observed or state == .Absent

  issues.push(issue(field, state, source.error_kind, source.errno))
}

## Attaches a report section name to collector issues.
export pure with_section(section: Str, issues: List[Issue]) -> List[report.CollectionIssue] {
  [{
    section: section,
    field: item.field,
    state: item.state,
    error_kind: item.error_kind,
    errno: item.errno,
    detail: item.detail,
  } for item in issues]
}

## Reads bounded text without treating absence or invalid UTF-8 as an empty value.
## Preserving whitespace keeps command-line token boundaries faithful to the source.
## A partial read retains its state but cannot expose its prefix as a complete value.
export proc read_source_text(
  root: FsRoot,
  source_path: Path,
  max_bytes: Int = 65536,
  preserve_whitespace: Bool = false,
) [fs, error] -> SourceRead {
  let raw = root.read_result(source_path, max_bytes:)?
  var state = source_state(raw.state, raw.truncated)
  var value: Str? = null
  var raw_bytes_base64: Str? = null

  if raw.data != null {
    let data = raw.data
    if let Ok(text) = data.utf8() {
      value = if preserve_whitespace { text } else { text.trim() }
    } else {
      if state == .Observed {
        state = report.Malformed
      }

      raw_bytes_base64 = data.base64()
    }
  } else if state == .Observed {
    state = report.Malformed
  }

  if state != .Observed {
    value = null
  }

  {
    observation: {
      state: state,
      value: value,
      raw_bytes_base64: raw_bytes_base64,
    },
    errno: raw.errno,
    error_kind: raw.error_kind,
  }
}

## Returns the observed text of a read, or null when it was not fully observed.
export pure observed_text(source: SourceRead) -> Str? {
  return source.observation.value when source.observation.state == .Observed

  null
}

## Accepts only nonempty ASCII decimal digits.
export pure decimal_digits(value: Str) -> Bool {
  return false when value == ""

  for digit in value {
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
      return false
    }
  }

  true
}

## Parses an optionally negative decimal integer, rejecting radix prefixes, plus signs, and overflow.
export pure parse_integer(value: Str?) -> Int? {
  guard value != null else {
    return null
  }

  let source_text = value.trim()
  let signed_digits = source_text.starts_with("-") and decimal_digits(source_text.split("") |> drop(1).join(""))
  return null when ! decimal_digits(source_text) and ! signed_digits

  if let Ok(parsed) = source_text.parse_int() {
    parsed
  } else {
    null
  }
}

## Parses a kernel `0`/`1` flag; any other number is not a flag.
export pure parse_bool01(value: Str?) -> Bool? {
  let parsed = parse_integer(value)
  return null when parsed == null

  return false when parsed == 0

  return true when parsed == 1

  null
}

## Splits whitespace-separated kernel fields.
export pure parse_words(value: Str?) -> List[Str] {
  return [] when value == null or value == ""

  value.split(" ") |> where .trim() != ""
}

## Splits a comma-separated kernel list, keeping an empty source empty.
export pure split_csv(value: Str) -> List[Str] {
  return [] when value == ""

  value.split(",")
}

## Parses only complete source observations and keeps integers exact in JSON.
export pure bounded_number(source: SourceRead, nonnegative: Bool) -> BoundedNumber {
  let observed = source.observation
  if observed.state == .Absent {
    return {value: null, state: null, error_kind: null, errno: null}
  }

  if observed.state != .Observed {
    return {value: null, state: observed.state, error_kind: source.error_kind, errno: source.errno}
  }

  let raw = observed.value ?? ""
  let signed_digits = raw.starts_with("-") and decimal_digits(raw.split("") |> drop(1).join(""))
  if ! decimal_digits(raw) and ! signed_digits {
    return {value: null, state: report.Malformed, error_kind: "invalid_integer", errno: null}
  }

  if nonnegative and raw.starts_with("-") {
    return {value: null, state: report.Malformed, error_kind: "negative_integer", errno: null}
  }

  if let Ok(number) = raw.parse_int() {
    if number < -9007199254740991 or number > 9007199254740991 {
      return {value: null, state: report.RangeFailure, error_kind: "json_integer_out_of_range", errno: null}
    }

    {value: number, state: null, error_kind: null, errno: null}
  } else {
    {value: null, state: report.RangeFailure, error_kind: "integer_out_of_range", errno: null}
  }
}

## Reports whether a path is a directory, treating any metadata failure as no.
export proc is_directory(root: FsRoot, entry: Path) [fs, error] -> Bool {
  if let Ok(metadata) = root.metadata(entry) {
    metadata.kind == "dir"
  } else {
    false
  }
}

## Reads a symlink target, accepting a plain directory where rooted fixtures stand in for links.
export proc class_entry_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation {
  if let Ok(observed) = root.readlink_result(entry) {
    if observed.state == "observed" {
      return {target: observed.target, state: report.Observed, errno: observed.errno, error_kind: observed.error_kind}
    }

    if observed.state == "absent" {
      return {target: null, state: report.Disappeared, errno: observed.errno, error_kind: observed.error_kind}
    }

    if observed.error_kind == "invalid_input" and is_directory(root, entry) {
      return {target: null, state: report.Observed, errno: null, error_kind: null}
    }

    {
      target: null,
      state: source_state(observed.state, false),
      errno: observed.errno,
      error_kind: observed.error_kind,
    }
  } else {
    {target: null, state: report.Malformed, errno: null, error_kind: "invalid_class_entry_path"}
  }
}

## Prefers a device link and keeps a class-entry link as independent parent evidence.
export proc class_parent_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation {
  let fallback = class_entry_target(root, entry)
  if let Ok(observed) = root.readlink_result(fp"{entry}/device") {
    if observed.state == "observed" {
      guard observed.target == null else {
        return {target: observed.target, state: report.Observed, errno: null, error_kind: null}
      }

      return {target: fallback.target, state: report.Malformed, errno: null, error_kind: "missing_device_target"}
    }

    return fallback when observed.state == "absent"

    {
      target: fallback.target,
      state: source_state(observed.state, false),
      errno: observed.errno,
      error_kind: observed.error_kind,
    }
  } else {
    {
      target: fallback.target,
      state: report.Malformed,
      errno: null,
      error_kind: "invalid_device_link_path",
    }
  }
}

## Reads an optional driver binding while retaining failures separate from an unbound device.
export proc driver_name(root: FsRoot, source_path: Path) [fs, error] -> SourceRead {
  if let Ok(observed) = root.readlink_result(source_path) {
    let state = source_state(observed.state, false)
    var value: Str? = null
    if observed.target != null {
      value = observed.target.name()
    }

    if state == .Observed and value == null {
      return {
        observation: empty_text(report.Malformed),
        errno: null,
        error_kind: "missing_driver_target",
      }
    }

    {
      observation: {
        state: state,
        value: value,
        raw_bytes_base64: null,
      },
      errno: observed.errno,
      error_kind: observed.error_kind,
    }
  } else {
    {
      observation: empty_text(report.Malformed),
      errno: null,
      error_kind: "invalid_driver_link_path",
    }
  }
}
