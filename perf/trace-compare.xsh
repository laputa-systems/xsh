# Compare two syscall baseline JSON files produced by --trace-json-out.
# Usage: xsh perf/trace-compare.xsh -- <baseline.json> <current.json>
# Prints tests where total syscall count changed. Exits 1 if any test regressed.
type TraceEntry = {syscall_count: Int}

type TraceReport = {tests: Map[TraceEntry]}

let script_args = args

if script_args.len() < 2 {
  print "usage: trace-compare.xsh -- <baseline.json> <current.json>"
  abort(2)
}

let baseline: TraceReport = json.read(fp"${script_args[0]}")?
let current: TraceReport = json.read(fp"${script_args[1]}")?
let b_tests = baseline.tests
let c_tests = current.tests
let b_ids = b_tests.keys()
let c_ids = c_tests.keys()
let all_ids = b_ids.extend(c_ids) |> sort-by .
var regressions = 0

for id in all_ids {
  let b_entry = b_tests.get(id)
  let c_entry = c_tests.get(id)
  let b_total = match b_entry { Ok(e) => e.syscall_count, Err(_) => 0 }
  let c_total = match c_entry { Ok(e) => e.syscall_count, Err(_) => 0 }
  continue when b_total == 0 and c_total == 0
  let delta = c_total - b_total

  if delta != 0 {
    let sign = if delta > 0 { "+" } else { "" }
    print f"${id}  total:${b_total}→${c_total}(${sign}${delta})"

    if delta > 0 {
      regressions = regressions + 1
    }
  }
}

if regressions > 0 {
  print f"""
${regressions} test(s) regressed (syscall count increased)"""

  abort(1)
}
