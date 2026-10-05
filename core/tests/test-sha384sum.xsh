type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Capture the real applet and its stdin without involving an external checksum.
proc invoke(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "sha384sum-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/sha384sum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_sha384sum_stdin_known_vector { |ctx|
  let result = invoke(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == b"cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  -\n"
  assert result.stderr == ""
}

test test_sha384sum_continues_after_unreadable_file { |ctx|
  let file = test.temp_file(ctx, name: "sha384sum-data", contents: b"abc")?
  let missing = test.temp_path(ctx, name: "sha384sum-missing")
  let result = invoke(ctx, [missing.display(), file.display()], b"")?
  assert result.status == 1
  assert result.stdout.len() > 0
  assert result.stderr.find("No such file or directory") != null
}

test test_sha384sum_check_and_strict_status { |ctx|
  let file = test.temp_file(ctx, name: "sha384sum-verify", contents: b"abc")?
  let list = test.temp_file(ctx, name: "sha384sum-list", contents: bytes.from_text(f"cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  {file}\n"))?
  let checked = invoke(ctx, ["-c", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n")
  let quiet = invoke(ctx, ["-c", "--quiet", list.display()], b"")?
  assert quiet.status == 0
  assert quiet.stdout == b""
  let malformed = test.temp_file(ctx, name: "sha384sum-bad-list", contents: bytes.from_text(f"junk\ncb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  {file}\n"))?
  let strict = invoke(ctx, ["-c", "--strict", "--status", malformed.display()], b"")?
  assert strict.status == 1
  assert strict.stdout == b""
}

test test_sha384sum_zero_binary_and_tagged { |ctx|
  let binary = invoke(ctx, ["-bz"], b"abc")?
  assert binary.status == 0
  assert binary.stdout == b"cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7 *-\0"
  let tagged = invoke(ctx, ["--tag"], b"abc")?
  assert tagged.status == 0
  assert tagged.stdout == b"SHA384 (-) = cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\n"
}
