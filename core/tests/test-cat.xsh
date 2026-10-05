type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/cat.xsh by its real path inside `root`, capturing both streams.
proc cat_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/cat.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_cat_file_and_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  fp"{root}/in.txt".write(b"file\n")

  assert cat_run(ctx, root, ["in.txt"])?.stdout == b"file\n"
  assert cat_run(ctx, root, [], b"stdin\n")?.stdout == b"stdin\n"
  assert cat_run(ctx, root, ["in.txt", "-", "in.txt"], b"mid\n")?.stdout == b"file\nmid\nfile\n"
}

test test_cat_preserves_non_utf8_bytes_across_chunks { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  let block = b"\0\xff\xfe\nabc\r\n"
  let big = bytes.concat([block for _ in range(20000)])
  fp"{root}/big.bin".write(big)

  assert cat_run(ctx, root, ["big.bin"])?.stdout == big
  assert cat_run(ctx, root, [], big)?.stdout == big
}

test test_cat_numbers_squeezes_and_continues_across_files { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  fp"{root}/a".write(b"a\n\n")
  fp"{root}/b".write(b"\n\nb")

  assert cat_run(ctx, root, ["-s", "a", "b"])?.stdout == b"a\n\nb"
  assert cat_run(ctx, root, ["-n", "a", "b"])?.stdout == b"     1\ta\n     2\t\n     3\t\n     4\t\n     5\tb"
  assert cat_run(ctx, root, ["-b", "a", "b"])?.stdout == b"     1\ta\n\n\n\n     2\tb"
  assert cat_run(ctx, root, ["-n", "-b", "-n", "a"])?.stdout == b"     1\ta\n\n", "-b overrides -n"
  assert cat_run(ctx, root, ["-sn", "-"], b"a\n\n\nb")?.stdout == b"     1\ta\n     2\t\n     3\tb"
}

test test_cat_show_modes_use_gnu_notation { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?

  assert cat_run(ctx, root, ["-A"], b"\t\0\n")?.stdout == b"^I^@$\n"
  assert cat_run(ctx, root, ["-e", "-e"], b"\t\0\n")?.stdout == b"\t^@$\n"
  assert cat_run(ctx, root, ["-t"], b"\t\x01\n")?.stdout == b"^I^A\n"
  assert cat_run(ctx, root, ["-v"], b"\x7f\x80\xff\x89\n")?.stdout == b"^?M-^@M-^?M-^I\n"
  assert cat_run(ctx, root, ["-T"], b"\ta\0")?.stdout == b"^Ia\0"
  assert cat_run(ctx, root, ["--show-a", "-"], b"\t\n")?.stdout == b"^I$\n"
}

test test_cat_keeps_carriage_returns_unless_shown { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?

  assert cat_run(ctx, root, ["-n"], b"Hello\r\nWorld")?.stdout == b"     1\tHello\r\n     2\tWorld"
  assert cat_run(ctx, root, ["-E"], b"a\nb\r\n\rc\n\r\n\r")?.stdout == b"a$\nb^M$\n\rc$\n^M$\n\r"
  assert cat_run(ctx, root, ["-v"], b"a\r\nb\r")?.stdout == b"a^M\nb^M"
}

test test_cat_reports_each_failure_and_exits_one { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  fp"{root}/dir".mkdir()
  fp"{root}/ok".write(b"ok\n")

  let result = cat_run(ctx, root, ["missing", "dir", "ok", "two words"])?
  assert result.status == 1
  assert result.stdout == b"ok\n"
  assert result.stderr == "cat: missing: No such file or directory\ncat: dir: Is a directory\ncat: 'two words': No such file or directory\n", result.stderr
}

test test_cat_refuses_to_copy_a_file_onto_itself { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  fp"{root}/loop".write(b"data\n")
  let script = fp"{ctx.core_dir}/cat.xsh"
  let err = fp"{root}/.err"

  cd $root {
    let status = run.status sh -c "exec \"$0\" \"$1\" loop >> loop 2> .err" ${ctx.xsh_bin} $script
    assert ! status.exited_with(0)
  }

  assert fp"{root}/loop".read_text()? == "data\n"
  assert err.read_text()? == "cat: loop: input file is output file\n"
}

test test_cat_invalid_option_uses_getopt_wording { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  let short = cat_run(ctx, root, ["-q"])?
  assert short.status == 1
  assert short.stderr == "cat: invalid option -- 'q'\nTry 'cat --help' for more information.\n", short.stderr

  let long = cat_run(ctx, root, ["--no-such"])?
  assert long.stderr == "cat: unrecognized option '--no-such'\nTry 'cat --help' for more information.\n", long.stderr
}

test test_cat_help_and_version_go_to_stdout { |ctx|
  let root = test.temp_dir(ctx, name: "cat")?
  let help = cat_run(ctx, root, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert "Usage: cat [OPTION]... [FILE]..." in help.stdout.utf8()?

  let version = cat_run(ctx, root, ["--version"])?
  assert version.stdout.starts_with(b"cat")
}
