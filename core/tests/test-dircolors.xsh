test test_dircolors_database_and_parser { |ctx|
  let db = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -p
  assert "TERM screen*" in db
  assert "DIR 01;34" in db
  let target = test.temp_file(ctx, name: "colors.txt", contents: b"NORMAL 00\nDIR 01;34\n.txt 32\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -b $target
  assert output == "LS_COLORS='no=00:di=01;34:*.txt=32:';\nexport LS_COLORS\n"
  let csh = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -c $target
  assert csh == "setenv LS_COLORS 'no=00:di=01;34:*.txt=32:'\n"
}
