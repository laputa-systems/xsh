test test_rgrep [fs, process, error] { |ctx|
  let root = test.temp_dir(ctx, name: "rgrep-root")?
  fp"${root}/a.xsh".write("proc hello() {}")?
  fp"${root}/b.xsh".write("proc world() {}")?
  let output = run.text "xsh" "showcase/rgrep.xsh" -- --pattern proc --root $root ?
  test.contains(output, "a.xsh:1:")?
  test.contains(output, "b.xsh:1:")?
  test.contains(output, "2 matches")?
}
