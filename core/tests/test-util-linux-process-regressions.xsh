type ProcessAppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_process_flock(ctx: TestContext, argv: List[Str]) -> ProcessAppletRun {
  test.run_script(ctx, fp"{ctx.core_dir}/flock.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "flock")?
}

type RevRegressionRun = {status: Int, stdout: Bytes, stderr: Str}

proc run_process_rev(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[RevRegressionRun] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/rev.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_util_linux_flock_keeps_lock_through_waited_new_session_child { |ctx|
  let root = test.temp_dir(ctx, name: "process-regression")?
  let lock_path = fp"{root}/work.lock"
  let child = fp"{root}/child.xsh"
  child.write("let pid = process.current_pid()?\nassert process.session_id(pid)? == pid\nassert process.group_id(pid)? == pid\nlet lock = fs.lock(Path(args[0]), nonblocking: true)\nassert lock is Err(_)\nexit 7\n")

  let result = run_process_flock(ctx, [
    lock_path.display(),
    ctx.xsh_bin.display(),
    fp"{ctx.core_dir}/setsid.xsh".display(),
    "-f",
    "-w",
    ctx.xsh_bin.display(),
    child.display(),
    lock_path.display(),
  ])

  assert result.status == 7, result.stderr
  assert result.stdout == "" and result.stderr == "", result.stderr

  let reacquired = fs.lock(lock_path, nonblocking: true)?
  fs.unlock(reacquired)
}

test test_util_linux_rev_reverses_utf8_split_across_read_chunks { |ctx|
  let root = test.temp_dir(ctx, name: "rev-chunk-boundary")?
  var prefix = b"a"
  while prefix.len() < 65535 {
    prefix = bytes.concat([prefix, prefix])
  }
  prefix = prefix[0..65535]
  fp"{root}/input".write(bytes.concat([prefix, b"\xc3\xa9\n"]))

  let result = run_process_rev(ctx, root, ["input"])?
  assert result.status == 0, result.stderr
  assert result.stderr == "", result.stderr
  assert result.stdout == bytes.concat([b"\xc3\xa9", prefix, b"\n"])
}

test test_util_linux_chrt_preserves_signal_termination { |ctx|
  test.expect(
    ctx,
    fp"{ctx.core_dir}/chrt.xsh".read_text()?,
    status: 143,
    args: ["-o", "0", "/bin/sh", "-c", "kill -TERM $$"],
    env: {XSH_MODULE_PATH: ctx.core_dir},
    stdin: b"",
    name: "chrt",
  )?
}
