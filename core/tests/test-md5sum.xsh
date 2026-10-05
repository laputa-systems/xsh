type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Capture the real applet and its stdin without involving an external checksum.
proc invoke(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "md5sum-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/md5sum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_md5sum_stdin_known_vector { |ctx|
  let result = invoke(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == b"900150983cd24fb0d6963f7d28e17f72  -\n"
  assert result.stderr == ""
}

test test_md5sum_continues_after_unreadable_file { |ctx|
  let file = test.temp_file(ctx, name: "md5sum-data", contents: b"abc")?
  let missing = test.temp_path(ctx, name: "md5sum-missing")
  let result = invoke(ctx, [missing.display(), file.display()], b"")?
  assert result.status == 1
  assert result.stdout.len() > 0
  assert result.stderr.find("No such file or directory") != null
}

test test_md5sum_check_and_strict_status { |ctx|
  let file = test.temp_file(ctx, name: "md5sum-verify", contents: b"abc")?
  let list = test.temp_file(ctx, name: "md5sum-list", contents: bytes.from_text(f"900150983cd24fb0d6963f7d28e17f72  {file}\n"))?
  let checked = invoke(ctx, ["-c", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n")
  let quiet = invoke(ctx, ["-c", "--quiet", list.display()], b"")?
  assert quiet.status == 0
  assert quiet.stdout == b""
  let malformed = test.temp_file(ctx, name: "md5sum-bad-list", contents: bytes.from_text(f"junk\n900150983cd24fb0d6963f7d28e17f72  {file}\n"))?
  let strict = invoke(ctx, ["-c", "--strict", "--status", malformed.display()], b"")?
  assert strict.status == 1
  assert strict.stdout == b""
}

test test_md5sum_zero_binary_and_tagged { |ctx|
  let binary = invoke(ctx, ["-bz"], b"abc")?
  assert binary.status == 0
  assert binary.stdout == b"900150983cd24fb0d6963f7d28e17f72 *-\0"
  let tagged = invoke(ctx, ["--tag"], b"abc")?
  assert tagged.status == 0
  assert tagged.stdout == b"MD5 (-) = 900150983cd24fb0d6963f7d28e17f72\n"
}
