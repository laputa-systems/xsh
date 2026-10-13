##! Reads Linux interface byte counters from one bounded /proc/net/dev observation.
##!
##! Interface selection is a separate policy so an empty listing and an unreadable
##! source remain distinct from an observed zero-valued counter.
use sys_source as src
use system_report as report

## Keeps one interface's name and byte counters from a complete kernel row.
export type NetworkInterface = {name: Str, ifindex: Int?, rx_bytes: Int, tx_bytes: Int}
## Preserves valid rows alongside source state and field-level issues.
export type NetworkSource = {
  state: report.ObservationState,
  interfaces: List[NetworkInterface],
  issues: List[src.Issue],
}

## Parses the byte counters from complete /proc/net/dev rows and keeps valid neighbors.
export pure parse(source: src.SourceRead) -> NetworkSource {
  if source.observation.state != .Observed or source.observation.value == null {
    return {
      state: source.observation.state,
      interfaces: [],
      issues: [src.issue("network.dev", source.observation.state, source.error_kind, source.errno)],
    }
  }

  var interfaces: List[NetworkInterface] = []
  var seen: Set[Str] = set.empty()
  var issues: List[src.Issue] = []
  for line_item in source.observation.value.lines() |> enumerate() {
    continue when line_item.index < 2
    continue when line_item.value.trim() == ""
    let pair = line_item.value.split(":", maxsplit: 1)
    if pair.len() != 2 {
      issues += [src.issue(f"network.line.{line_item.index}", report.Malformed, "invalid_netdev_row", null)]
      continue
    }
    let name = pair[0].trim()
    if name == "" or name in seen {
      issues += [src.issue(f"network.line.{line_item.index}", report.Malformed, "invalid_or_duplicate_interface", null)]
      continue
    }
    seen = seen.add(name)
    let fields = src.parse_words(pair[1].trim().replace("\t", with: " "))
    if fields.len() < 16 {
      issues += [src.issue(f"network.{name}", report.Malformed, "incomplete_netdev_row", null)]
      continue
    }
    var counters = []
    var state: report.ObservationState? = null
    for value in fields |> take(16) {
      let parsed = src.parse_integer(value)
      if parsed == null or (parsed ?? -1) < 0 {
        state = if src.decimal_digits(value) { report.RangeFailure } else { report.Malformed }
        break
      }
      counters += [parsed ?? 0]
    }
    if state != null {
      issues += [src.issue(f"network.{name}", state ?? report.Malformed, "invalid_netdev_counter", null)]
      continue
    }
    interfaces += [{name: name, ifindex: null, rx_bytes: counters[0], tx_bytes: counters[8]}]
  }

  {state: report.Observed, interfaces: interfaces |> sort-by .name, issues: issues}
}

## Reads a complete network-interface table from the caller-owned root.
export proc collect(root: FsRoot) [fs, error] -> NetworkSource {
  let parsed = parse(src.read_source_text(root, p"proc/net/dev", max_bytes: 1048576))
  return parsed when parsed.state != report.Observed

  var interfaces: List[NetworkInterface] = []
  var issues = parsed.issues
  for item in parsed.interfaces {
    let source = src.read_source_text(root, fp"sys/class/net/{item.name}/ifindex", max_bytes: 128)
    if source.observation.state != report.Observed or source.observation.value == null {
      issues += [src.issue(f"network.{item.name}.ifindex", source.observation.state, source.error_kind, source.errno)]
      interfaces += [{...item, ifindex: null}]
      continue
    }
    let raw = (source.observation.value ?? "").trim()
    let parsed_index = src.parse_integer(raw)
    if parsed_index == null or (parsed_index ?? 0) <= 0 {
      let state = if src.decimal_digits(raw) { report.RangeFailure } else { report.Malformed }
      issues += [src.issue(f"network.{item.name}.ifindex", state, "invalid_interface_index", null)]
      interfaces += [{...item, ifindex: null}]
      continue
    }
    interfaces += [{...item, ifindex: parsed_index}]
  }
  {...parsed, interfaces: interfaces, issues: issues}
}

## Selects the first name-sorted non-loopback interface, falling back to loopback only.
export pure selected_interface(interfaces: List[NetworkInterface]) -> NetworkInterface? {
  var candidates = interfaces |> where .name != "lo"
  if candidates.is_empty() {
    candidates = interfaces
  }
  return null when candidates.is_empty()

  (candidates |> sort-by .name)[0]
}

## Requires an observed interface when a caller needs selection to succeed.
export proc select_interface(interfaces: List[NetworkInterface]) [error] -> Result[NetworkInterface, Error] {
  if let selected = selected_interface(interfaces) {
    return Ok(selected)
  }

  Err(error.failure("no network interface is observed"))
}
