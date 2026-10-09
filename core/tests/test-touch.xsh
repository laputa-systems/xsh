test test_touch { |ctx|
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"{root}/created.txt"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" $target ?
  assert target.exists()?
  let missing = fp"{root}/missing.txt"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/touch.xsh" -c $missing ?
  assert ! missing.exists()?
}

test test_touch_dash_updates_stdout_timestamps { |ctx|
  let root = test.temp_dir(ctx, name: "touch-dash")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/touch.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "-t", "197001010000", "-"]
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {TZ: "UTC"},
    b"",
    stdout,
    stderr,
  )
  let status = process.run(plan)?
  assert status.exit_code()? == 0
  assert fs.stat(stdout)?.mtime_ns == 0
  assert stderr.read_text()? == ""
}
