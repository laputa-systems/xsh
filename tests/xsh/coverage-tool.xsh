type CoverageReport = {standard_apis: List[Str], api_hits: Record}

type CoverageHitCounts = {tests: Int, examples: Int}

proc coverage_merge_script(source: Str, root: Path) [error] -> Result[Str] {
  return source.replace("proc main(", "proc coverage_main(")
    + "\nlet root = p" + json.encode(root.display())?
    + "\nlet report = merge_reports(root, [{name: \"sample\", path: \"input.json\"}])?\nprint json.encode(report)?\n"
}

test test_combined_coverage_report_includes_standard_api_hits [fs, process, env, error] { |ctx|
  let repo = fs.cwd()?
  let root = test.temp_dir(ctx, name: "combined-coverage")?.resolve()?
  let tests = fp"${root}/tests/xsh"
  tests.mkdir()?
  fp"${tests}/smoke.xsh".write("""test test_cpu_count [error] { test.ok(cpu.count() > 0)? }
""")?

  let out_dir = fp"${root}/coverage"
  let report_path = fp"${out_dir}/coverage.json"
  let text_path = fp"${out_dir}/coverage.txt"
  let stdout = fp"${root}/stdout.txt"
  let stderr = fp"${root}/stderr.txt"
  let xsh = fp"${repo}/target/debug/xsh"
  let xsht = fp"${repo}/target/debug/xsht"
  let tool = fp"${repo}/tools/xsh-cov.xsh"
  let command = process.command {
    cwd = root
    stdout = stdout
    stderr = stderr
    run $xsh $tool
  }

  env XSHT=$xsht XSH_COV_DIR=$out_dir XSH_COV_JSON=$report_path XSH_COV_REPORT=$text_path {
    let status = process.run(command)?
    test.ok(status.exited_with(0), stdout.read_text()? + stderr.read_text()?)?
  } ?

  let report = json.read(report_path)?.require(CoverageReport)?
  let standard_apis = report.standard_apis
  let api_hits = report.api_hits
  (standard_apis.len() > 0)
  (api_hits.keys().len() > 0)
  text_path.exists()?
}

test test_coverage_report_wire_counts_keep_missing_defaults_and_reject_invalid_values [fs, process, error] { |ctx|
  let repo = fs.cwd()?
  let root = test.temp_dir(ctx, name: "coverage-wire")?.resolve()?
  let input = fp"${root}/input.json"
  let source = fp"${repo}/tools/xsh-cov.xsh".read_text()?
  let script = coverage_merge_script(source, root)?

  for example in [
    {json: r"""{"standard_apis":["module.cpu.count"],"api_hits":{"module.cpu.count":{}}}""", tests: 0, examples: 0},
    {json: r"""{"standard_apis":["module.cpu.count"],"api_hits":{"module.cpu.count":{"tests":2}}}""", tests: 2, examples: 0},
    {json: r"""{"standard_apis":["module.cpu.count"],"api_hits":{"module.cpu.count":{"tests":2,"examples":3,"extra":true}}}""", tests: 2, examples: 3},
  ] {
    input.write(example.json)?
    let output = test.run_script(ctx, script)?
    assert output.success, output.stderr
    let report = json.decode(output.stdout)?.require(CoverageReport)?
    let hits = report.api_hits.get("module.cpu.count")?.require(CoverageHitCounts)?
    report.standard_apis == ["module.cpu.count"]
    hits.tests == example.tests
    hits.examples == example.examples
  }

  for invalid in [
    r"""{"standard_apis":[1],"api_hits":{}}""",
    r"""{"standard_apis":[],"api_hits":[]}""",
    r"""{"standard_apis":[],"api_hits":{"module.cpu.count":null}}""",
    r"""{"standard_apis":[],"api_hits":{"module.cpu.count":{"tests":"2"}}}""",
    r"""{"standard_apis":[],"api_hits":{"module.cpu.count":{"examples":null}}}""",
  ] {
    input.write(invalid)?
    let output = test.run_script(ctx, script)?
    output.status != 0
    output.stdout == ""
    assert "schema" in output.stderr, output.stderr
  }
}
