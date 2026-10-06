type Process = ProcessEntry

proc parent_for(pid: Int) [process, time, error] -> Result[Int] {
  repeat 10 times {
    let rows: List[Process] = process.list()? |> where .pid == pid

    return rows[0].parent_pid when ! rows.is_empty()

    time.sleep(100ms)
  }

  Err(error.failure(f"spawned process {pid} was not visible"))
}

test test_pstree_renders_tree_with_pid_labels { |ctx|
  let child = spawn run sleep 30 ?
  let parent_pid = parent_for(child.pid)?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pstree.xsh" -- -p $parent_pid
  assert f"[{parent_pid}]" in output
  assert f"sleep [{child.pid}]" in output
  # A parent's row comes before its children's.
  assert f"sleep [{child.pid}]" in output.split(f"[{parent_pid}]")[1], output
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
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pstree.xsh"
  assert output.trim() != ""

  assert "[1]" not in output
}
