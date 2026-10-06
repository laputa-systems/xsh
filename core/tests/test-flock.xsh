type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_flock(ctx: TestContext, argv: List[Str]) -> AppletRun {
  test.run_script(ctx, fp"{ctx.core_dir}/flock.xsh".read_text()?, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "flock")?
}

test test_flock_conflict_does_not_run_command_and_releases_on_exit { |ctx|
  let root = test.temp_dir(ctx, name: "flock")?
  let path_value = fp"{root}/lock"
  let marker = fp"{root}/ran"
  let held = fs.lock(path_value)?
  let blocked = run_flock(ctx, ["-n", "-E", "73", "-c", f"printf ran > {marker}", path_value.display()])
  assert blocked.status == 73, blocked.stderr
  assert ! marker.exists()?
  fs.unlock(held)
  let ran = run_flock(ctx, ["-c", f"printf ran > {marker}; exit 9", path_value.display()])
  assert ran.status == 9, ran.stderr
  assert marker.read_text()? == "ran"
  let released = fs.lock(path_value, nonblocking: true)?
  fs.unlock(released)
}

test test_flock_shared_locks_allow_other_readers { |ctx|
  let root = test.temp_dir(ctx, name: "flock-shared")?
  let path_value = fp"{root}/lock"
  let held = fs.lock(path_value, shared: true)?
  defer fs.unlock(held)
  let result = run_flock(ctx, ["-s", "-n", path_value.display(), "/bin/sh", "-c", "exit 4"])
  assert result.status == 4, result.stderr
}


test test_flock_holds_the_lock_while_the_child_runs { |ctx|
  let root = test.temp_dir(ctx, name: "flock-child")?
  let lock_path = fp"{root}/lock"
  let child = fp"{root}/child.xsh"
  child.write("let result = fs.lock(Path(args[0]), nonblocking: true)\nassert result is Err(_)\n")
  let result = run_flock(ctx, [lock_path.display(), ctx.xsh_bin.display(), child.display(), lock_path.display()])
  assert result.success, result.stderr
  let shell_form = run_flock(ctx, [lock_path.display(), "-c", "exit 6"])
  assert shell_form.status == 6, shell_form.stderr
}
