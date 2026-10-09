type CutRun = {status: Int, stderr: Str}

proc cut_run(ctx: TestContext, args: List[Str], phrase: Str = "") [fs, process, error] -> Result[CutRun] {
  let root = test.temp_dir(ctx, name: "cut-run")?
  let stdin = test.temp_file(ctx, name: "cut-stdin", contents: b"")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/cut.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: phrase}, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stderr: err.read_text()?})
}

test test_cut_fields { |ctx|
  let input = test.temp_file(ctx, name: "table.txt", contents: b"a,b,c\n1,2,3\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -d , -f 2 $input ?
  assert "b" in output
  assert "2" in output

  let empty = test.temp_file(ctx, name: "empty.txt", contents: b"")?
  let no_output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -d "\n" -f 1 $empty ?
  assert no_output == ""
}

test test_cut_range_error_keeps_gnu_plain_diagnostic { |ctx|
  let result = cut_run(ctx, ["-f", "1,0"])?
  assert result.status != 0
  assert result.stderr == "cut: fields are numbered from 1\nTry 'cut --help' for more information.\n"
}

test test_cut_uutils_phrase_keeps_plain_diagnostic_when_stderr_is_a_pipe { |ctx|
  let result = cut_run(ctx, ["-f", "1,0"], "xsh-uutests cut")?
  assert result.status != 0
  assert result.stderr == "cut: fields are numbered from 1\nTry 'xsh-uutests cut --help' for more information.\n"
}

test test_cut_reads_files_and_stdin_in_operand_order { |ctx|
  let first = test.temp_file(ctx, name: "first.txt", contents: b"a,b\n")?
  let middle = test.temp_file(ctx, name: "middle.txt", contents: b"c,d\n")?
  let last = test.temp_file(ctx, name: "last.txt", contents: b"e,f\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cut.xsh" -d , -f 2 $first - $last < ${middle} ?
  assert output == """b
d
f
"""
}
