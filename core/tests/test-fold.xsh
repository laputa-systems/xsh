test test_fold_width { |ctx|
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -w 3 $input ?
  assert output == "abc\ndef\n"
}

test test_fold_soft_break_tabs_and_utf8 { |ctx|
  let words = test.temp_file(ctx, name: "words.txt", contents: b"ab cd ef\n")?
  let soft = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -s -w 5 $words ?
  assert soft == "ab \ncd ef\n"

  let wide = test.temp_file(ctx, name: "wide-char.txt", contents: b"界界界\n")?
  let columns = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fold.xsh" -w 5 $wide ?
  assert columns == "界界\n界\n"
}

test test_fold_negative_width_value_diagnostic { |ctx|
  let root = test.temp_dir(ctx, name: "fold-negative-width")?
  let script = fp"{ctx.core_dir}/fold.xsh"
  for flag in ["-w", "--width", "-bw", "-sw"] {
    let out = fp"{root}/stdout-{flag}"
    let err = fp"{root}/stderr-{flag}"
    let argv = [ctx.xsh_bin.display(), script.display(), flag, "-1", "missing"]
    let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
    let status = process.run(plan)?
    assert status.exit_code()? != 0
    assert err.read_text()? == "fold: invalid number of columns: '-1'\n"
  }
}
