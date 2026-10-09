test test_file_report { |ctx|
  let root = test.temp_dir(ctx, name: "report-root")?
  fp"{root}/a.xsh".write("proc main() {}")
  fp"{root}/b.xsh".write("proc other() {}")
  let output = run.text "xsh" "showcase/file-report.xsh" --root $root ?
  assert "2 files  " in output
}
