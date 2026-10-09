type Ran = {status: Int, stdout: Str, stderr: Str}

proc mknod_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/mknod.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_mknod_fifo_and_mode { |ctx|
  let root = test.temp_dir(ctx, name: "mknod")?
  let fifo = fp"{root}/pipe"
  let result = mknod_run(ctx, root, ["-m", "0640", "pipe", "p"])?
  assert result.status == 0, result.stderr
  assert fs.stat(fifo)?.kind == "fifo"
  assert fifo.metadata()?.mode % 512 == 0o640
}

test test_mknod_fifo_rejects_device_numbers { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-extra")?
  let result = mknod_run(ctx, root, ["node", "p", "1", "2"])?
  assert result.status == 1
  assert "Fifos do not have major and minor device numbers" in result.stderr
}

test test_mknod_requires_device_numbers { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-device")?
  let result = mknod_run(ctx, root, ["node", "c"])?
  assert result.status == 1
  assert "major and minor" in result.stderr
}
