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

test test_touch_rejects_a_nonexistent_local_timestamp { |ctx|
  let root = test.temp_dir(ctx, name: "touch-dst-gap")?
  let target = fp"{root}/missing"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/touch.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "-m", "-t", "202003080200", target.display()]
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {TZ: "EST+5EDT,M3.2.0/2,M11.1.0/2", LC_ALL: "C"},
    b"",
    stdout,
    stderr,
  )
  let status = process.run(plan)?

  assert status.exit_code()? == 1
  assert ! target.exists()?
  assert stderr.read_text()?.starts_with("touch: invalid date format"), stderr.read_text()?
}
