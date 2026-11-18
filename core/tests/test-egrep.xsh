test test_egrep_defaults_to_extended_patterns { |ctx|
  let root = test.temp_dir(ctx, name: "egrep")?
  let file = fp"{root}/input"
  file.write("red\nblue\ngreen\n")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/egrep.xsh" -- "red|blue" $file
  assert result.status.exited_with(0)
  assert result.stderr == "egrep: warning: egrep is obsolescent; using grep -E\n", result.stderr
  assert result.stdout == "red\nblue\n"
}
