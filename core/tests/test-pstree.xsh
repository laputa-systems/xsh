error BusyboxTestError = ProcessList(message: Str)

type Process = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  status: Str,
  start_time: Str,
  start_time_ms: Int,
  runtime_seconds: Int,
}

proc parent_for(pid: Int) [process, time, error] -> Result[Int] {
  for _ in range(10) {
    let rows: List[Process] = process.list()? |> where .pid == pid

    if rows.len() > 0 {
      return rows[0].parent_pid
    }

    time.sleep(100ms)?
  }

  return Err(BusyboxTestError.ProcessList(message: f"spawned process ${pid} was not visible"))
}

proc test_pstree_renders_tree_with_pid_labels(ctx: TestContext) [process, env, time, error] {
  let child = spawn run sleep 30 ?
  let parent_pid = parent_for(child.pid)?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/pstree.xsh" -- -p $parent_pid ?
  f"[${parent_pid}]" in output
  f"sleep [${child.pid}]" in output
  ("\u{251c}\u{2500}" in output or "\u{2514}\u{2500}" in output or "|-" in output or "`-" in output)
  ! ("->" in output)
}

proc test_pstree_rejects_unknown_pid(ctx: TestContext) [fs, process, env, error] {
  let err = test.temp_path(ctx, name: "pstree.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/pstree.xsh" -- 999999999 2> $err
  ! status.exited_with(0)
  "no such pid" in (err.read_text()?)
}

proc test_pstree_default_prints_visible_root(ctx: TestContext) [process, env, error] {
  if system.uname()?.sysname == "Darwin" {
    match process.which("pstree") {
      Err(_) => test.skip("macOS pstree is unavailable")?
      Ok(_) => {}
    }
  }

  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/pstree.xsh" ?
  (output.trim() != "")

  if system.uname()?.sysname == "Darwin" {
    "launchd" in output
    "00001" in output
    return
  }

  "[1]" in output
}
