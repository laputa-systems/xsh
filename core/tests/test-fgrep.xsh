test test_fgrep_defaults_to_literal_patterns { |ctx|
  let root = test.temp_dir(ctx, name: "fgrep")?
  let file = fp"{root}/input"
  file.write("a.b\naxb\n")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/fgrep.xsh" -- a.b $file
  assert result.status.exited_with(0)
  assert result.stderr == "fgrep: warning: fgrep is obsolescent; using grep -F\n", result.stderr
  assert result.stdout == "a.b\n"
}
