test test_fgrep_defaults_to_literal_patterns { |ctx|
  let root = test.temp_dir(ctx, name: "fgrep")?
  let file = fp"{root}/input"
  file.write("a.b\naxb\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fgrep.xsh" -- a.b $file
  assert out == "a.b\n"
}
