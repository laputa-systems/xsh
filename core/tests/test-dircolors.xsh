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

test test_dircolors_gnu_keywords_and_shell_escaping { |ctx|
  let target = test.temp_file(ctx, name: "quoted-colors.txt", contents: b"LEFT 1\nRIGHT 2\nEND 3\nCLRTOEOL 4\nFILE a=b:c\n*.caret a^:b\n*.slash a\\:b\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -b $target
  assert output == "LS_COLORS='lc=1:rc=2:ec=3:cl=4:fi=a\\=b\\:c:*.caret=a^:b:*.slash=a\\:b:';\nexport LS_COLORS\n"
}

test test_dircolors_value_comment_without_space { |ctx|
  let target = test.temp_file(ctx, name: "inline-comment-colors.txt", contents: b"DIR 32# comment\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -b $target
  assert output == "LS_COLORS='di=32:';\nexport LS_COLORS\n"
}

test test_dircolors_non_utf8_file_operand { |ctx|
  let root = test.temp_dir(ctx, name: "dircolors-non-utf8")?
  let database = Path.parse_bytes(bytes.concat([root.bytes(), b"/colors-\xff\xfe"]))?
  database.write(b"NORMAL 00\n*.txt 32\n")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, script, database]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", SHELL: "bash"}, b"", stdout, stderr))?
  assert status.exit_code()? == 0, stderr.read_text()?
  assert stdout.read_text()? == "LS_COLORS='no=00:*.txt=32:';\nexport LS_COLORS\n"
}

test test_dircolors_missing_non_utf8_file_operand_reports_name { |ctx|
  let root = test.temp_dir(ctx, name: "dircolors-missing-non-utf8")?
  let missing = Path.parse_bytes(bytes.concat([root.bytes(), b"/absent-\xff\xfe"]))?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, script, missing]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", SHELL: "bash"}, b"", stdout, stderr))?
  assert status.exit_code()? == 1
  assert stdout.read_text()? == ""
  assert "No such file or directory" in stderr.read_text()?, stderr.read_text()?
}

test test_dircolors_shell_escapes_single_quotes { |ctx|
  let target = test.temp_file(ctx, name: "single-quote-colors.txt", contents: b"EXEC 'echo Hello;:'\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -b $target
  assert output == "LS_COLORS='ex='\\''echo Hello;\\:'\\'':';\nexport LS_COLORS\n"
}

test test_dircolors_long_options_abbreviate_and_clash { |ctx|
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let listing = run.capture --text ${ctx.xsh_bin} $script -- -p --print-ls
  assert listing.status.exited_with(1)
  assert listing.stderr == "dircolors: options --print-database and --print-ls-colors are mutually exclusive\nTry 'dircolors --help' for more information.\n", listing.stderr

  let shell = run.capture --text ${ctx.xsh_bin} $script -- -b --print-database
  assert shell.status.exited_with(1)
  assert shell.stderr == "dircolors: the options to output non shell syntax,\nand to select a shell syntax are mutually exclusive\nTry 'dircolors --help' for more information.\n", shell.stderr

  let abbreviated = run.capture --text ${ctx.xsh_bin} $script -- --c --print-database
  assert abbreviated.status.exited_with(1)
  assert abbreviated.stderr.starts_with("dircolors: the options to output non shell syntax,\n"), abbreviated.stderr

  assert "TERM screen*" in run.text ${ctx.xsh_bin} $script -- --print-d
}

test test_dircolors_missing_second_token_names_the_line { |ctx|
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let target = test.temp_file(ctx, name: "short-colors.txt", contents: b"NORMAL 00\nexec\n")?
  let bad = run.capture --text ${ctx.xsh_bin} $script -- -b $target
  assert bad.status.exited_with(1)
  assert bad.stderr == f"dircolors: {target}:2: invalid line;  missing second token\n", bad.stderr
}

test test_dircolors_default_database_gnu_extensions { |ctx|
  let db = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dircolors.xsh" -- -p
  assert "TERM vt220\n" in db
  assert ".apk 01;31\n" in db
  assert ".crate 01;31\n" in db
  assert ".jxl 01;35\n" in db
  assert ".crdownload 00;90\n" in db
}

test test_dircolors_display_requires_matching_terminal { |ctx|
  let root = test.temp_dir(ctx, name: "dircolors-display")?
  let output = fp"{root}/stdout"
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, script, "--", "--print-ls-colors"]
  let plan = process.command_argv(ctx.xsh_bin, words, env: {TERM: "", COLORTERM: ""}, stdout: output)
  assert process.run(plan)?.exited_with(0)
  assert output.read_bytes()? == b""
  let matching = process.command_argv(ctx.xsh_bin, words, env: {TERM: "screen", COLORTERM: ""}, stdout: output)
  assert process.run(matching)?.exited_with(0)
  assert "\x1b[01;34mdi\t01;34\x1b[0m\n" in output.read_text()?
}

test test_dircolors_directory_operand_reports_read_error { |ctx|
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let bad = run.capture --text ${ctx.xsh_bin} $script -- -c /
  assert bad.status.exited_with(1)
  assert bad.stdout == ""
  assert bad.stderr == "dircolors: /: read error: Is a directory\n", bad.stderr
}
