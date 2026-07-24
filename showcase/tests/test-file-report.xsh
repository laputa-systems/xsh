proc test_file_report(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "report-root")?
  fp"${root}/a.xsh".write("proc main() {}")?
  fp"${root}/b.xsh".write("proc other() {}")?
  let output = run.text "xsh" "showcase/file-report.xsh" -- --root $root ?
  test.contains(output, "2 files  ")?
}
