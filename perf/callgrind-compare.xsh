# Diff total executed instructions (Ir) between two callgrind_annotate text
# reports. Callgrind Ir is deterministic, so a drop in total Ir is a real,
# noise-free speedup signal — the headline check for optimization commits.
#
# Per-function detail lives in the annotated .txt artifacts; we don't diff it
# automatically because callgrind prints "file:function" and source paths differ
# between git worktrees, which breaks cross-revision matching.
#
# Usage: xsh perf/callgrind-compare.xsh -- <before.txt> <after.txt>

# Pull the "PROGRAM TOTALS" Ir count out of a callgrind_annotate report.
proc total_ir(report: Path) [fs, error] -> Result[Int] {
  for line in report.lines()? {
    if line.contains("PROGRAM TOTALS") {
      # e.g. "  12,345,678 (100.0%)  PROGRAM TOTALS"
      let head = line.split("(")[0]
      let digits = head.replace(",", "").replace(" ", "").trim()

      if digits != "" {
        return digits.parse_int()
      }
    }
  }

  return -1
}

let script_args = args

if script_args.len() < 2 {
  eprint "usage: callgrind-compare.xsh -- <before.txt> <after.txt>"
  abort(2)
}

let before = total_ir(fp"${script_args[0]}")?
let after = total_ir(fp"${script_args[1]}")?

if before <= 0 or after <= 0 {
  eprint "callgrind-compare: could not read PROGRAM TOTALS from one of the reports"
  abort(2)
}

let delta = after - before
let adelta = if delta < 0 { -delta } else { delta }
let dsign = if delta > 0 { "+" } else if delta < 0 { "-" } else { "" }
let pm = adelta * 1000 / before
print "callgrind total instructions (Ir):"
print f"  before: ${before}"
print f"  after:  ${after}"
print f"  delta:  ${dsign}${adelta} (${dsign}${pm / 10}.${pm % 10}%)"

if delta > 0 {
  print ""
  print f"instruction count increased by ${delta} (after is slower)"
  abort(1)
}
