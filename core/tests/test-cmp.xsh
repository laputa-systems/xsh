type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "cmp-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/cmp.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_cmp_byte_difference_status { |ctx|
  let left = test.temp_file(ctx, name: "left", contents: b"a\nb")?
  let right = test.temp_file(ctx, name: "right", contents: b"a\nc")?
  let result = invoke(ctx, [left.display(), right.display()])?
  assert result.status == 1
  assert result.stdout.ends_with("differ: byte 3, line 2\n")
  assert invoke(ctx, ["-s", left.display(), right.display()])?.stdout == ""
  assert invoke(ctx, ["-n", "2", left.display(), right.display()])?.status == 0
}

test test_cmp_stdin_offsets_and_listing { |ctx|
  let file = test.temp_file(ctx, name: "right", contents: b"zab")?
  assert invoke(ctx, ["-i", "1:1", "-", file.display()], b"xab")?.status == 0
  let result = invoke(ctx, ["-l", "-", file.display()], b"yab")?
  assert result.status == 1
  assert result.stdout.trim() == "1 171 172"
}

test test_cmp_chunk_boundary_and_zero_limit_missing_file { |ctx|
  let left = test.temp_file(ctx, name: "large-left", contents: bytes.zero(65537)?)?
  let right = test.temp_file(ctx, name: "large-right", contents: bytes.zero(65537)?)?
  assert bytes.write_at(left, 65536, b"a")? == 1
  assert bytes.write_at(right, 65536, b"b")? == 1
  let result = invoke(ctx, [left.display(), right.display()])?
  assert result.status == 1
  assert result.stdout.ends_with("differ: byte 65537, line 1\n"), result.stdout
  let missing = test.temp_path(ctx, name: "missing")
  assert invoke(ctx, ["-n0", missing.display(), right.display()])?.status == 2
}

test test_cmp_eof_is_difference_and_io_errors_are_trouble { |ctx|
  let file = test.temp_file(ctx, name: "longer", contents: b"abc")?
  let shorter = invoke(ctx, ["-", file.display()], b"ab")?
  assert shorter.status == 1
  assert shorter.stderr.find("EOF") != null
  let missing = test.temp_path(ctx, name: "missing")
  let failed = invoke(ctx, ["-s", missing.display(), file.display()])?
  assert failed.status == 2
  assert failed.stderr == ""
}
