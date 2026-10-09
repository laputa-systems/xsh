test test_rgrep { |ctx|
  let root = test.temp_dir(ctx, name: "rgrep-root")?
  fp"{root}/a.xsh".write("proc hello() {}")
  fp"{root}/b.xsh".write("proc world() {}")
  let output = run.text "xsh" "showcase/rgrep.xsh" --pattern proc --root $root ?
  assert "a.xsh:1:" in output
  assert "b.xsh:1:" in output
  assert "2 matches" in output
}
