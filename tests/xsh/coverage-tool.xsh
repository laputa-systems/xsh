proc test_combined_coverage_report_includes_standard_api_hits(ctx: TestContext) [fs, process, env, error] {
  let repo = fs.cwd()?
  let root = test.temp_dir(ctx, name: "combined-coverage")?.resolve()?
  let tests = fp"${root}/tests/xsh"
  tests.mkdir()?
  fp"${tests}/smoke.xsh".write("""proc test_cpu_count() [error] { test.ok(cpu.count() > 0)? }
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

  let report: Record = json.read(report_path)?
  let standard_apis: List[Str] = report.get("standard_apis")?
  let api_hits: Record = report.get("api_hits")?
  test.ok(standard_apis.len() > 0)?
  test.ok(api_hits.keys().len() > 0)?
  test.ok(text_path.exists()?)?
}
