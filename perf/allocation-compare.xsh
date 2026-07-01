error CompareError = Failed(message: Str)

type AllocationMetrics = {
  allocation_calls: Int,
  allocation_bytes: Int,
  deallocation_calls: Int,
  deallocation_bytes: Int,
  reallocation_calls: Int,
  reallocation_bytes: Int,
  alloc_calls_le16: Int,
  alloc_calls_le64: Int,
  alloc_calls_le256: Int,
  alloc_calls_le4096: Int,
  alloc_calls_gt4096: Int,
  peak_rss: Int,
}

type AllocationScenario = {name: Str, metrics: AllocationMetrics}

type AllocationReport = {version: Int, scale: Int, scenarios: List[AllocationScenario]}

let script_args = args

if script_args.len() < 2 {
  print "usage: allocation-compare.xsh -- <baseline.json> <current.json>"
  abort(2)
}

let baseline = json.read(fp"${script_args[0]}")?.require(AllocationReport)?
let current = json.read(fp"${script_args[1]}")?.require(AllocationReport)?

if baseline.scale != current.scale {
  print f"scale changed: ${baseline.scale} -> ${current.scale}"
}

proc compare_field(scenario: Str, name: Str, before: Int, after: Int, fail_on_increase: Bool) [io] -> Int {
  let delta = after - before

  if delta == 0 {
    return 0
  }

  let sign = if delta > 0 { "+" } else { "" }
  print f"${scenario}  ${name}:${before}->${after}(${sign}${delta})"

  if fail_on_increase and delta > 0 {
    return 1
  }

  return 0
}

pure find_scenario(scenarios: List[AllocationScenario], name: Str) -> Result[AllocationMetrics] {
  for scenario in scenarios {
    if scenario.name == name {
      return scenario.metrics
    }
  }

  return Err(CompareError.Failed(f"missing scenario: ${name}"))
}

var regressions = 0

for b_scenario in baseline.scenarios |> sort-by .name {
  let id = b_scenario.name
  let b_entry = b_scenario.metrics

  match find_scenario(current.scenarios, id) {
    Ok(c_entry) => {
      regressions += compare_field(id, "allocation_calls", b_entry.allocation_calls, c_entry.allocation_calls, true)
      regressions += compare_field(id, "allocation_bytes", b_entry.allocation_bytes, c_entry.allocation_bytes, true)

      regressions += compare_field(
        id,
        "deallocation_calls",
        b_entry.deallocation_calls,
        c_entry.deallocation_calls,
        true,
      )

      regressions += compare_field(
        id,
        "deallocation_bytes",
        b_entry.deallocation_bytes,
        c_entry.deallocation_bytes,
        true,
      )

      regressions += compare_field(
        id,
        "reallocation_calls",
        b_entry.reallocation_calls,
        c_entry.reallocation_calls,
        true,
      )

      regressions += compare_field(
        id,
        "reallocation_bytes",
        b_entry.reallocation_bytes,
        c_entry.reallocation_bytes,
        true,
      )

      regressions += compare_field(id, "alloc_calls_le16", b_entry.alloc_calls_le16, c_entry.alloc_calls_le16, true)
      regressions += compare_field(id, "alloc_calls_le64", b_entry.alloc_calls_le64, c_entry.alloc_calls_le64, true)
      regressions += compare_field(id, "alloc_calls_le256", b_entry.alloc_calls_le256, c_entry.alloc_calls_le256, true)

      regressions += compare_field(
        id,
        "alloc_calls_le4096",
        b_entry.alloc_calls_le4096,
        c_entry.alloc_calls_le4096,
        true,
      )

      regressions += compare_field(
        id,
        "alloc_calls_gt4096",
        b_entry.alloc_calls_gt4096,
        c_entry.alloc_calls_gt4096,
        true,
      )

      regressions += compare_field(id, "peak_rss", b_entry.peak_rss, c_entry.peak_rss, false)
    }
    Err(_) => {
      print f"${id}  missing from current report"
      regressions += 1
    }
  }
}

for c_scenario in current.scenarios |> sort-by .name {
  match find_scenario(baseline.scenarios, c_scenario.name) {
    Ok(_) => {}
    Err(_) => print f"${c_scenario.name}  new scenario"
  }
}

if regressions > 0 {
  print f"""
${regressions} allocation metric(s) regressed"""

  abort(1)
}
