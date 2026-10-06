test test_egrep_defaults_to_extended_patterns { |ctx|
  let root = test.temp_dir(ctx, name: "egrep")?
  let file = fp"{root}/input"
  file.write("red\nblue\ngreen\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/egrep.xsh" -- "red|blue" $file
  assert out == "red\nblue\n"
}
