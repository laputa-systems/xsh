proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

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

proc test_pstree_renders_tree_with_pid_labels() [process, env, time, error] {
  let child = spawn run sleep 30 ?
  let parent_pid = parent_for(child.pid)?
  let output = run.text xsh_bin() core_script("pstree.xsh") -- -p $parent_pid ?
  test.contains(output, f"[${parent_pid}]")?
  test.contains(output, f"sleep [${child.pid}]")?
  test.ok("\u{251c}\u{2500}" in output or "\u{2514}\u{2500}" in output or "|-" in output or "`-" in output)?
  test.ok(! ("->" in output))?
}

proc test_pstree_rejects_unknown_pid(ctx: TestContext) [fs, process, env, error] {
  let err = test.temp_path(ctx, name: "pstree.err")
  let status = run.status xsh_bin() core_script("pstree.xsh") -- 999999999 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "no such pid")?
}

proc test_pstree_default_prints_visible_root() [process, env, error] {
  let output = run.text xsh_bin() core_script("pstree.xsh") ?
  test.ok(output.trim() != "")?
  test.contains(output, "pstree.xsh")?
  test.contains(output, "[")?
}
