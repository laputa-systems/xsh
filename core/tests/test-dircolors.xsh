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

test test_dircolors_terminal_selection_and_bad_database { |ctx|
  let target = test.temp_file(ctx, name: "terminal-colors.txt", contents: b"NORMAL 00\nTERM screen*\nDIR 34\nTERM linux\nDIR 32\n")?
  let output = test.temp_file(ctx, name: "colors-output.txt", contents: b"")?
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, script, "--", "-b", target], env: {TERM: "screen-256color"}, stdout: output)
  assert process.run(plan)?.exited_with(0)
  assert output.read_text()? == "LS_COLORS='no=00:di=34:';\nexport LS_COLORS\n"
  let invalid = test.temp_file(ctx, name: "bad-colors.txt", contents: b"UNKNOWN 34\n")?
  let bad = run.capture --text ${ctx.xsh_bin} $script -- -b $invalid
  assert bad.status.exited_with(1)
  assert "unrecognized keyword 'UNKNOWN'" in bad.stderr
}
