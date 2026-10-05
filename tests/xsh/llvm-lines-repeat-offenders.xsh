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

test test_llvm_lines_repeat_offenders_preserves_text_and_json_reports { |ctx|
  let root = test.temp_dir(ctx, name: "llvm-lines-report")?
  let input = fp"{root}/capture.txt"
  input.write(llvm_lines_capture)
  let source = fp"{fs.cwd()?}/tools/llvm-lines-repeat-offenders.xsh".read_text()?
  let text = test.run_script(ctx, source, args: [input, "--limit", "1", "--examples", "1"])?
  let {success: text_succeeded, stderr: text_failure_details, ..} = text
  assert text_succeeded, text_failure_details
  assert "top 1 llvm-lines repeat offenders" in text.stdout
  assert "xsh[abc]::work::<_>" in text.stdout
  assert "xsh[abc]::work::<u8>" in text.stdout
  let dependencies_absent = "dependency::helper" not in text.stdout
  let report_text = text.stdout
  assert dependencies_absent, report_text

  let owned = test.run_script(ctx, source, args: [input, "--sum", "--json"])?
  let {success: owned_succeeded, stderr: owned_failure_details, ..} = owned
  assert owned_succeeded, owned_failure_details
  let summary = json.decode(owned.stdout)?.require(LlvmLinesIntegralSummary)?
  assert summary.scope == "owned"
  assert summary.offenders == 1
  assert summary.instances == 2
  assert summary.duplicated == 60
  assert summary.grand_total == 1000
  assert summary.pct == 6

  let dependencies = test.run_script(
    ctx,
    source,
    args: [input, "--sum", "--all", "--filter", "dependency", "--json"],
  )?
  let {success: dependencies_succeeded, stderr: dependencies_failure_details, ..} = dependencies
  assert dependencies_succeeded, dependencies_failure_details
  let filtered = json.decode(dependencies.stdout)?.require(LlvmLinesIntegralSummary)?
  assert filtered.scope == "all"
  assert filtered.filter == "dependency"
  assert filtered.offenders == 1
  assert filtered.instances == 2
  assert filtered.duplicated == 20
}

test test_llvm_lines_repeat_offenders_keeps_numeric_failures_and_unknown_totals { |ctx|
  let root = test.temp_dir(ctx, name: "llvm-lines-invalid")?
  let input = fp"{root}/capture.txt"
  let source = fp"{fs.cwd()?}/tools/llvm-lines-repeat-offenders.xsh".read_text()?
  input.write("""01 1% 1% 2 1% 1% xsh[abc]::work::<u8>
""")
  let rejected = test.expect(ctx, source, status: 3, args: [input, "--sum", "--json"])?
  assert rejected.stdout == ""
  assert "json" in rejected.stderr

  input.write(
    llvm_lines_capture.replace(from: "1000 100% 100% 10 100% 100% (TOTAL)", to: "true 100% 100% 10 100% 100% (TOTAL)"),
  )
  let unknown = test.run_script(ctx, source, args: [input, "--sum", "--json"])?
  let {success: succeeded, stderr: failure_details, ..} = unknown
  assert succeeded, failure_details
  let summary = json.decode(unknown.stdout)?.require(LlvmLinesIntegralSummary)?
  assert summary.duplicated == 60
  assert summary.grand_total == -1
  assert summary.pct == 0
}
