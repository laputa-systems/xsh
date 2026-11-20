test test_bench_selectors_reject_empty_unknown_and_full { |ctx|
  let unknown = run.capture --text python3 -B dev/performance/bench.py --workload no-such-workload --dry-run
  assert unknown.status.shell_code()? != 0
  assert "unknown workload" in unknown.stderr
  let empty = run.capture --text python3 -B dev/performance/bench.py --tier edit --skip harness-tests --dry-run
  assert empty.status.shell_code()? != 0
  assert "selection is empty" in empty.stderr
  let full = run.capture --text python3 -B dev/performance/bench.py --tier full --dry-run
  assert full.status.shell_code()? != 0
  assert "full requires --full" in full.stderr
}

test test_bench_ceiling_area_and_exclusions { |ctx|
  let selected = run.capture --text python3 -B dev/performance/bench.py --tier change --area startup --dry-run
  assert selected.status.shell_code()? == 0, selected.stderr
  assert "consolidation-startup" in selected.stdout
  assert "excluded stdlib-cold: different area" in selected.stdout
  assert "excluded legacy-baseline: different area" in selected.stdout
  let mismatch = run.capture --text python3 -B dev/performance/bench.py --workload consolidation-startup --area stdlib --dry-run
  assert mismatch.status.shell_code()? != 0
}
