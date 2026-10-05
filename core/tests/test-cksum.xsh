type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Capture the real applet and its stdin without involving an external checksum.
proc invoke(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "cksum-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/cksum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_cksum_stdin_known_vector { |ctx|
  let result = invoke(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == b"1219131554 3\n"
  assert result.stderr == ""
}

test test_cksum_continues_after_unreadable_file { |ctx|
  let file = test.temp_file(ctx, name: "cksum-data", contents: b"abc")?
  let missing = test.temp_path(ctx, name: "cksum-missing")
  let result = invoke(ctx, [missing.display(), file.display()], b"")?
  assert result.status == 1
  assert result.stdout.len() > 0
  assert result.stderr.find("No such file or directory") != null
}

test test_cksum_digest_modes { |ctx|
  let result = invoke(ctx, ["-a", "sha2", "-l", "224"], b"abc")?
  assert result.status == 0
  assert result.stdout == b"SHA224 (-) = 23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\n"
  let raw = invoke(ctx, ["-a", "md5", "--raw"], b"abc")?
  assert raw.status == 0
  assert raw.stdout.len() == 16
}
