##! Reads aggregate and sparse per-core CPU counters, uptime, and load from one rooted procfs view.
##!
##! Counter rows retain all kernel fields so consumers can apply distinct aggregate
##! and per-process denominator policies without consulting a live-only sampler.
use sys_source as src
use system_report as report

## Retains every aggregate or per-core counter field from one procfs row.
export type CpuCounters = {fields: List[Int]}
## Associates sparse logical CPU identifiers with their raw counters.
export type CpuCore = {id: Int, counters: CpuCounters}
## Keeps aggregate usability separate from malformed or unreadable core rows.
export type CpuStat = {
  state: report.ObservationState,
  aggregate: CpuCounters?,
  cores: List[CpuCore],
  issues: List[src.Issue],
}
## Holds an uptime value and the observation state that supports it.
export type Uptime = {state: report.ObservationState, seconds: Float?}
## Holds the three load averages and their source state.
export type LoadAverage = {state: report.ObservationState, values: List[Float]}
## Groups the three independent procfs observations used by the CPU panel.
export type CpuSource = {stat: CpuStat, uptime: Uptime, load: LoadAverage}

type CounterParse = {state: report.ObservationState, fields: List[Int]}

pure empty_stat(state: report.ObservationState, issues: List[src.Issue] = []) -> CpuStat {
  {state: state, aggregate: null, cores: [], issues: issues}
}

pure parse_counter_fields(words: List[Str]) -> CounterParse {
  return {state: report.Malformed, fields: []} when words.len() < 4

  var fields = []
  for word in words {
    let parsed = src.parse_integer(word)
    if parsed == null or (parsed ?? -1) < 0 {
      let state = if src.decimal_digits(word) { report.RangeFailure } else { report.Malformed }
      return {state: state, fields: []}
    }
    fields += [parsed ?? 0]
  }

  {state: report.Observed, fields: fields}
}

## Parses a complete /proc/stat observation without exposing truncated counters.
export pure parse_stat(source: src.SourceRead) -> CpuStat {
  if source.observation.state != .Observed or source.observation.value == null {
    return empty_stat(source.observation.state, [
      src.issue("cpu.stat", source.observation.state, source.error_kind, source.errno),
    ])
  }

  var aggregate: CpuCounters? = null
  var aggregate_malformed = false
  var cores: List[CpuCore] = []
  var seen_ids: Set[Int] = set.empty()
  var issues: List[src.Issue] = []
  for line_item in source.observation.value.lines() |> enumerate() {
    let words = src.parse_words(line_item.value)
    continue when words.is_empty()
    let label = words[0]
    if label == "cpu" {
      if aggregate != null or aggregate_malformed {
        aggregate_malformed = true
        aggregate = null
        issues += [src.issue("cpu.stat.aggregate", report.Malformed, "duplicate_aggregate_cpu_row", null)]
        continue
      }

      let parsed = parse_counter_fields(words |> drop(1))
      if parsed.state == .Observed {
        aggregate = {fields: parsed.fields}
      } else {
        aggregate_malformed = true
        issues += [src.issue("cpu.stat.aggregate", parsed.state, "invalid_aggregate_cpu_row", null)]
      }
      continue
    }

    continue when ! label.starts_with("cpu")
    let id_text = label.split("") |> drop(3).join("")
    if ! src.decimal_digits(id_text) {
      issues += [src.issue(f"cpu.stat.{label}", report.Malformed, "invalid_cpu_row_name", null)]
      continue
    }

    let id = src.parse_integer(id_text)
    if id == null or (id ?? -1) < 0 {
      issues += [src.issue(f"cpu.stat.{label}", report.RangeFailure, "cpu_id_out_of_range", null)]
      continue
    }

    let cpu_id = id ?? 0
    if cpu_id in seen_ids {
      issues += [src.issue(f"cpu.stat.{label}", report.Malformed, "duplicate_cpu_row", null)]
      continue
    }

    seen_ids = seen_ids.add(cpu_id)
    let parsed = parse_counter_fields(words |> drop(1))
    if parsed.state == .Observed {
      cores += [{id: cpu_id, counters: {fields: parsed.fields}}]
    } else {
      issues += [src.issue(f"cpu.stat.{label}", parsed.state, "invalid_cpu_row", null)]
    }
  }

  if aggregate == null {
    let state = if aggregate_malformed or source.observation.value != "" { report.Malformed } else { report.Absent }
    if issues.is_empty() {
      issues += [src.issue("cpu.stat.aggregate", state, "missing_aggregate_cpu_row", null)]
    }
    return {state: state, aggregate: null, cores: [], issues: issues}
  }

  {
    state: report.Observed,
    aggregate: aggregate,
    cores: cores |> sort-by .id,
    issues: issues,
  }
}

pure decimal_seconds(value: Str) -> Float? {
  return null when value == ""
  var dot_seen = false
  var digits = 0
  for character in value {
    if character == "." and ! dot_seen {
      dot_seen = true
      continue
    }
    return null when character < "0" or character > "9"
    digits += 1
  }
  return null when digits == 0
  if let Ok(parsed) = value.parse_float() {
    return null when parsed < 0.0
    parsed
  } else {
    null
  }
}

## Parses the first complete decimal value from /proc/uptime.
export pure parse_uptime(source: src.SourceRead) -> Uptime {
  if source.observation.state != .Observed or source.observation.value == null {
    return {state: source.observation.state, seconds: null}
  }
  let words = src.parse_words(source.observation.value)
  return {state: report.Malformed, seconds: null} when words.is_empty()
  let seconds = decimal_seconds(words[0])
  return {state: report.Malformed, seconds: null} when seconds == null
  {state: report.Observed, seconds: seconds}
}

## Parses the first three complete values from /proc/loadavg.
export pure parse_loadavg(source: src.SourceRead) -> LoadAverage {
  if source.observation.state != .Observed or source.observation.value == null {
    return {state: source.observation.state, values: []}
  }
  let words = src.parse_words(source.observation.value)
  return {state: report.Malformed, values: []} when words.len() < 3
  var values = []
  for word in words |> take(3) {
    let value = decimal_seconds(word)
    return {state: report.Malformed, values: []} when value == null
    values += [value ?? 0.0]
  }
  {state: report.Observed, values: values}
}

## Reads only the aggregate CPU, uptime, and load inputs for the monitor.
export proc collect(root: FsRoot) [fs, error] -> CpuSource {
  let stat = src.read_source_text(root, p"proc/stat", max_bytes: 1048576)
  let uptime = src.read_source_text(root, p"proc/uptime", max_bytes: 4096)
  let load = src.read_source_text(root, p"proc/loadavg", max_bytes: 4096)
  {
    stat: parse_stat(stat),
    uptime: parse_uptime(uptime),
    load: parse_loadavg(load),
  }
}
