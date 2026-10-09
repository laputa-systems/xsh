type TrRun = {status: Int, stdout: Bytes, stderr: Str}

proc tr_run(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[TrRun] {
  let root = test.temp_dir(ctx, name: "tr-run")?
  let stdin = test.temp_file(ctx, name: "tr-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/tr.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_tr_translate_delete_squeeze_and_stdin { |ctx|
  assert tr_run(ctx, ["a", "A"], b"abbc\n")?.stdout == b"Abbc\n"
  assert tr_run(ctx, ["a-z", "A-Z"], b"abbc\n")?.stdout == b"ABBC\n"
  assert tr_run(ctx, ["-d", "b"], b"abbc\n")?.stdout == b"ac\n"
  assert tr_run(ctx, ["-cd", "[:digit:]"], b"a1-b2\n")?.stdout == b"12"
  assert tr_run(ctx, ["-s", "b", "B"], b"abbc\n")?.stdout == b"aBc\n"
}

test test_tr_rejects_bad_usage { |ctx|
  let result = tr_run(ctx, ["a"], b"")?
  assert result.status != 0
  assert "missing operand" in result.stderr
}

test test_tr_repeat_and_set2_padding { |ctx|
  assert tr_run(ctx, ["[a*3]bc", "x[y*]z"], b"abc")?.stdout == b"yyz"
  assert tr_run(ctx, ["a", "[b*]"], b"aabbc")?.stdout == b"bbbbc"
  assert tr_run(ctx, ["abc", "[b*0]"], b"abcd")?.stdout == b"bbbd"
  assert tr_run(ctx, ["-d", "[=a=]"], b"a=b")?.stdout == b"=b"
  assert tr_run(ctx, ["-d", "\\501"], b"(1Ł)")?.stdout == b"\xC5\x81)"
}

test test_tr_range_and_class_diagnostics { |ctx|
  let backwards = tr_run(ctx, ["-d", "\\046-\\048"], b"")?
  assert backwards.status != 0
  assert backwards.stderr == "tr: range-endpoints of '&-\\004' are in reverse collating sequence order\n"

  let class_mismatch = tr_run(ctx,
    ["-c", "[a*18446744073709551615]b[:upper:]", "[x*18446744073709551615][:upper:]"], b"")?
  assert class_mismatch.status != 0
  assert "must be matched by" in class_mismatch.stderr
}
