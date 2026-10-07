type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Capture the real applet and its stdin without involving an external checksum.
proc invoke(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sum-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/sum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_sum_stdin_known_vector { |ctx|
  let result = invoke(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == b"16556     1\n"
  assert result.stderr == ""

  let explicit_stdin = invoke(ctx, ["-"], b"abc")?
  assert explicit_stdin.status == 0
  assert explicit_stdin.stdout == b"16556     1\n"
  assert explicit_stdin.stderr == ""
}

test test_sum_continues_after_unreadable_file { |ctx|
  let file = test.temp_file(ctx, name: "sum-data", contents: b"abc")?
  let missing = test.temp_path(ctx, name: "sum-missing")
  let result = invoke(ctx, [missing.display(), file.display()], b"")?
  assert result.status == 1
  assert result.stdout.len() > 0
  assert result.stderr.find("No such file or directory") != null
}

test test_sum_sysv_stdin { |ctx|
  let result = invoke(ctx, ["-s"], b"abc")?
  assert result.status == 0
  assert result.stdout == b"294 1\n"
}

test test_sum_accepts_non_utf8_path_arguments { |ctx|
  let root = test.temp_dir(ctx, name: "sum-non-utf8")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/\xff\xfe"]))?
  file.write("test content")
  let stdout = fp"{root}/out"
  let stderr = fp"{root}/err"
  let script = fp"{ctx.core_dir}/sum.xsh"
  let argv: List[Union[Str, Path]] = [ctx.xsh_bin.display(), script.display(), "--", file]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", stdout, stderr)
  let status = process.run(plan)?

  assert status.exit_code()? == 0
  assert stdout.read_bytes()?.ends_with(bytes.concat([b" ", file.bytes(), b"\n"]))
  assert stderr.read_text()? == ""
}
