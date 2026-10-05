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
  repeat 10 times {
    let rows: List[Process] = process.list()? |> where .pid == pid

    return rows[0].parent_pid when rows.len() > 0

    time.sleep(100ms)
  }

  Err(BusyboxTestError.ProcessList(message: f"spawned process {pid} was not visible"))
}

test test_pstree_renders_tree_with_pid_labels { |ctx|
  let child = spawn run sleep 30 ?
  let parent_pid = parent_for(child.pid)?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pstree.xsh" -- -p $parent_pid ?
  assert f"[{parent_pid}]" in output
  assert f"sleep [{child.pid}]" in output
  assert "├─" in output or "└─" in output or "|-" in output or "`-" in output
  assert ! ("->" in output)
}

test test_pstree_rejects_unknown_pid { |ctx|
  let err = test.temp_path(ctx, name: "pstree.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/pstree.xsh" -- 999999999 2> $err
  assert ! status.exited_with(0)
  assert "no such pid" in err.read_text()?
}

test test_pstree_default_prints_visible_root { |ctx|
  if system.uname()?.sysname == "Darwin" {
    match process.which("pstree") {
      Err(_) => test.skip("macOS pstree is unavailable")
      Ok(_) => {}
    }
  }

  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pstree.xsh" ?
  assert output.trim() != ""

  if system.uname()?.sysname == "Darwin" {
    assert "launchd" in output
    assert "00001" in output
    return
  }

  assert "[1]" in output
}
