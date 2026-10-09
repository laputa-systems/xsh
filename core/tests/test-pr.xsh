type PrRun = {status: Int, stdout: Bytes, stderr: Str}

proc pr_run(ctx: TestContext, args: List[Str], input: Bytes, phrase = "") [fs, process, error] -> Result[PrRun] {
  let root = test.temp_dir(ctx, name: "pr-run")?
  let stdin = test.temp_file(ctx, name: "pr-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pr.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", TZ: "UTC", XSH_EXECUTION_PHRASE: phrase}, stdin, out, err)
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

test test_pr_invalid_expand_tab_arguments { |ctx|
  let invalid_cluster = pr_run(ctx, ["-esdgjiojiosdgjiogd"], b"")?
  assert invalid_cluster.status == 1
  assert invalid_cluster.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘dgjiojiosdgjiogd’\nTry 'pr --help' for more information.\n"

  let two_chars = pr_run(ctx, ["-eab"], b"")?
  assert two_chars.status == 1
  assert two_chars.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘b’\nTry 'pr --help' for more information.\n"

  let bad_width = pr_run(ctx, ["-e1a"], b"")?
  assert bad_width.status == 1
  assert bad_width.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘1a’\nTry 'pr --help' for more information.\n"

  let bad_char_width = pr_run(ctx, ["-ea1a"], b"")?
  assert bad_char_width.status == 1
  assert bad_char_width.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘1a’\nTry 'pr --help' for more information.\n"

  let adapter_phrase = "/tmp/xsh-pr-stage/xsh-uutests pr"
  let oversized = pr_run(ctx, ["-e2147483648"], b"", adapter_phrase)?
  assert oversized.status == 1
  assert oversized.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘2147483648’\nTry 'pr --help' for more information.\n"

  let oversized_char_width = pr_run(ctx, ["-ea2147483648"], b"", adapter_phrase)?
  assert oversized_char_width.status == 1
  assert oversized_char_width.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘2147483648’\nTry 'pr --help' for more information.\n"
}

test test_pr_expand_width_overflow_diagnostic { |ctx|
  let result = pr_run(ctx, ["-e2147483648"], b"")?

  assert result.status == 1
  assert result.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘2147483648’: Value too large for defined data type\nTry 'pr --help' for more information.\n", result.stderr
}

test test_pr_negative_expand_tabs { |ctx|
  let result = pr_run(ctx, ["-e=-1"], b"")?

  assert result.status == 1
  assert result.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘-1’\nTry 'pr --help' for more information.\n", result.stderr
}

test test_pr_number_width_too_large { |ctx|
  let result = pr_run(ctx, ["-n", "18446744073709551615"], b"")?

  assert result.status == 1
  assert result.stderr == "pr: '-n' extra characters or invalid number in the argument: '18446744073709551615': Value too large for defined data type\nTry 'pr --help' for more information.\n", result.stderr
}

test test_pr_zero_expand_tab_width { |ctx|
  let zero_width = pr_run(ctx, ["-e0"], b"")?
  assert zero_width.status == 1
  assert zero_width.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘0’\nTry 'pr --help' for more information.\n", zero_width.stderr

  let zero_char_width = pr_run(ctx, ["-eX0"], b"")?
  assert zero_char_width.status == 1
  assert zero_char_width.stderr == "pr: '-e' extra characters or invalid number in the argument: ‘0’\nTry 'pr --help' for more information.\n", zero_char_width.stderr
}

test test_pr_rejects_tab_expansion_overflow { |ctx|
  let result = pr_run(ctx, ["-t", "-e1073741824"], b"\t\t")?

  assert result.status == 1
  assert result.stderr == "pr: integer overflow\n", result.stderr
}
