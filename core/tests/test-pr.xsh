type PrRun = {status: Int, stdout: Bytes, stderr: Str}

proc pr_run(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[PrRun] {
  let root = test.temp_dir(ctx, name: "pr-run")?
  let stdin = test.temp_file(ctx, name: "pr-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pr.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", TZ: "UTC"}, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_pr_omit_header_numbers_and_double_spacing { |ctx|
  assert pr_run(ctx, ["-t", "-n"], b"a\nb\n")?.stdout == b"    1\ta\n    2\tb\n"
  assert pr_run(ctx, ["-t", "-d"], b"a\nb\n")?.stdout == b"a\n\nb\n\n"
}

test test_pr_column_order_spacing_and_page_width { |ctx|
  assert pr_run(ctx, ["-t", "-2", "-w", "20"], b"a\nb\nc\n")?.stdout == b"a        \tc        \nb        \n"
  assert pr_run(ctx, ["-t", "-a", "-2", "-w", "20"], b"a\nb\nc\n")?.stdout == b"a        \tb        \nc        \n"
}

test test_pr_expand_tabs { |ctx|
  assert pr_run(ctx, ["-t", "-e"], b"a\tb\n")?.stdout == b"a       b\n"
  assert pr_run(ctx, ["-t", "-e2"], b"a\tb\n")?.stdout == b"a b\n"
  assert pr_run(ctx, ["-t", "-ea2"], b"abc\tdef\n")?.stdout == b"  bc    def\n"
}

test test_pr_rejects_tab_expansion_overflow { |ctx|
  let result = pr_run(ctx, ["-t", "-e1073741824"], b"\t\t")?

  assert result.status == 1
  assert result.stderr == "pr: integer overflow\n", result.stderr
}
