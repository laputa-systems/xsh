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

test test_unlzma_corrupt_stream_reports_corrupted_data { |ctx|
  let root = test.temp_dir(ctx, name: "unlzma-corrupt")?
  let result = run_applet(ctx, root, "unlzma", [], b"junk junk junk junk")?
  assert result.status == 1
  assert result.stdout == b""
  assert result.stderr == "unlzma: corrupted data\n", result.stderr
}

test test_unlzma_round_trip_from_standard_input { |ctx|
  let root = test.temp_dir(ctx, name: "unlzma-round-trip")?
  let packed = run_applet(ctx, root, "lzma", ["-c"], b"HELLO\n")?
  let result = run_applet(ctx, root, "unlzma", ["-c"], packed.stdout)?
  assert result.status == 0, result.stderr
  assert result.stdout == b"HELLO\n"
}
