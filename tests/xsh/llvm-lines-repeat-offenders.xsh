type LlvmLinesIntegralSummary = {
  filter: Str,
  scope: Str,
  offenders: Int,
  instances: Int,
  duplicated: Int,
  grand_total: Int,
  pct: Int,
}

const llvm_lines_capture = r"""LINES PERCENT CUMULATIVE COPIES PERCENT CUMULATIVE FUNCTION

not a numeric row
100 10% 10% 2 20% 20% xsh[abc]::work::<u8>
60 6% 16% 3 30% 50% xsh[abc]::work::<u16>
40 4% 20% 1 10% 60% dependency::helper::<u8>
20 2% 22% 1 10% 70% dependency::helper::<u16>
1000 100% 100% 10 100% 100% (TOTAL)
"""

test test_llvm_lines_repeat_offenders_preserves_text_and_json_reports [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "llvm-lines-report")?
  let input = fp"${root}/capture.txt"
  input.write(llvm_lines_capture)?
  let source = fp"${fs.cwd()?}/tools/llvm-lines-repeat-offenders.xsh".read_text()?
  let text = test.run_script(ctx, source, args: [input.display(), "--limit", "1", "--examples", "1"])?
  test.ok(text.success, text.stderr)?
  test.ok("top 1 llvm-lines repeat offenders" in text.stdout)?
  test.ok("xsh[abc]::work::<_>" in text.stdout)?
  test.ok("xsh[abc]::work::<u8>" in text.stdout)?
  test.ok(("dependency::helper" not in text.stdout), text.stdout)?

  let owned = test.run_script(ctx, source, args: [input.display(), "--sum", "--json"])?
  test.ok(owned.success, owned.stderr)?
  let summary = json.decode(owned.stdout)?.require(LlvmLinesIntegralSummary)?
  test.eq(summary.scope, "owned")?
  test.eq(summary.offenders, 1)?
  test.eq(summary.instances, 2)?
  test.eq(summary.duplicated, 60)?
  test.eq(summary.grand_total, 1000)?
  test.eq(summary.pct, 6)?

  let dependencies = test.run_script(ctx, source, args: [input.display(), "--sum", "--all", "--filter", "dependency", "--json"])?
  test.ok(dependencies.success, dependencies.stderr)?
  let filtered = json.decode(dependencies.stdout)?.require(LlvmLinesIntegralSummary)?
  test.eq(filtered.scope, "all")?
  test.eq(filtered.filter, "dependency")?
  test.eq(filtered.offenders, 1)?
  test.eq(filtered.instances, 2)?
  test.eq(filtered.duplicated, 20)?
}

test test_llvm_lines_repeat_offenders_keeps_numeric_failures_and_unknown_totals [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "llvm-lines-invalid")?
  let input = fp"${root}/capture.txt"
  let source = fp"${fs.cwd()?}/tools/llvm-lines-repeat-offenders.xsh".read_text()?
  input.write("01 1% 1% 2 1% 1% xsh[abc]::work::<u8>\n")?
  let rejected = test.run_script(ctx, source, args: [input.display(), "--sum", "--json"])?
  test.eq(rejected.status, 3)?
  test.eq(rejected.stdout, "")?
  test.ok("json" in rejected.stderr)?

  input.write(llvm_lines_capture.replace(from: "1000 100% 100% 10 100% 100% (TOTAL)", to: "true 100% 100% 10 100% 100% (TOTAL)"))?
  let unknown = test.run_script(ctx, source, args: [input.display(), "--sum", "--json"])?
  test.ok(unknown.success, unknown.stderr)?
  let summary = json.decode(unknown.stdout)?.require(LlvmLinesIntegralSummary)?
  test.eq(summary.duplicated, 60)?
  test.eq(summary.grand_total, -1)?
  test.eq(summary.pct, 0)?
}
