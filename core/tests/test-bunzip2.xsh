type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs a compression applet by its real path inside `root`, so the invoked name
# and `lib.compress` resolve as they do for an installed applet.
proc run_applet(ctx: TestContext, root: Path, tool: Str, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/{tool}.xsh".display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_bunzip2_unknown_suffix_is_ignored { |ctx|
  let root = test.temp_dir(ctx, name: "bunzip2-suffix")?
  let packed = run_applet(ctx, root, "bzip2", ["-c"], b"HELLO\n")?
  fp"{root}/t1.bz2".write(packed.stdout)
  fp"{root}/t.zz".write("x")
  let result = run_applet(ctx, root, "bunzip2", ["t.zz", "t1.bz2"])?
  assert result.status == 1
  assert result.stderr == "bunzip2: t.zz: unknown suffix - ignored\n", result.stderr
  assert fp"{root}/t.zz".read_text()? == "x"
  assert fp"{root}/t1".read_text()? == "HELLO\n"
  assert ! fp"{root}/t.zz.out".exists()?
}

test test_bunzip2_corrupt_stream_reports_bunzip_error { |ctx|
  let root = test.temp_dir(ctx, name: "bunzip2-corrupt")?
  let result = run_applet(ctx, root, "bunzip2", [], b"BZh9\x31\x41\x59\x26\x53\x59junkjunkjunk")?
  assert result.status == 1
  assert result.stdout == b""
  assert result.stderr == "bunzip2: bunzip error -5\n", result.stderr
}

test test_bunzip2_existing_output_is_kept_and_later_operands_run { |ctx|
  let root = test.temp_dir(ctx, name: "bunzip2-exists")?
  let packed = run_applet(ctx, root, "bzip2", ["-c"], b"HELLO\n")?
  fp"{root}/t1.bz2".write(packed.stdout)
  fp"{root}/t2.bz2".write(packed.stdout)
  fp"{root}/t1".write("")
  let result = run_applet(ctx, root, "bunzip2", ["t1.bz2", "t2.bz2"])?
  assert result.status == 1
  assert result.stderr == "bunzip2: can't open 't1': File exists\n", result.stderr
  assert fp"{root}/t1".read_text()? == ""
  assert fp"{root}/t1.bz2".exists()?
  assert fp"{root}/t2".read_text()? == "HELLO\n"
}
