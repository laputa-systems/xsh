let oracle = r"""proc descriptor_count() [fs, error] -> Int {
  fs.children(p"/proc/self/fd")? |> count()
}
proc live_children() [fs, error] -> Str {
  var out = ""
  for task in fs.children(p"/proc/self/task")? {
    out = out + fp"/proc/self/task/{task.name}/children".read_text()?.trim()
  }
  out
}
"""

proc linux_only() [env, error] -> Result[Bool] {
  Ok(system.uname()?.sysname == "Linux")
}

test test_repeated_failed_spawns_leave_no_children_or_descriptors { |ctx|
  guard linux_only()? else { return }
  test.expect(ctx, oracle + r"""proc main() [fs, process, error] {
  let before = descriptor_count()
  let unwritable = Path("/proc/self/cwd/nonexistent-dir/file")
  for _ in range(24) {
    assert (try run.text xsh-missing-program) is Err(_)
    assert (try run.capture --text xsh-missing-program) is Err(_)
    assert (try run.text true | run xsh-missing-program) is Err(_)
    assert (try run.text xsh-missing-program | run cat) is Err(_)
    assert (try run.text true > $unwritable) is Err(_)
    let rows = try run.stream --text xsh-missing-program
    assert rows is Err(_)
    let handle = spawn run xsh-missing-program
    assert handle is Err(_)
  }
  assert live_children() == "", live_children()
  assert descriptor_count() == before
}
""", status: 0)?
}

test test_early_consumer_exit_leaves_no_children_or_descriptors { |ctx|
  guard linux_only()? else { return }
  test.expect(ctx, oracle + r"""proc first_row() [process, error] -> Str {
  let rows = run.stream --text sh -c "yes | head -c 100000000; sleep 30"
  for row in rows { return row }
  ""
}
proc fail_midway() [process, error] -> Result[Unit] {
  let rows = run.stream --text sh -c "yes"
  for row in rows { fail "stop" }
  Ok()
}
proc main() [fs, process, error] {
  let before = descriptor_count()
  for _ in range(16) {
    assert first_row() == "y"
    assert fail_midway() is Err(_)
    let out = run.text yes | run head -n 1
    assert out == "y\n"
    let captured = run.capture --text sh -c "yes | head -n 2"
    assert captured.status.exited_with(0)
    let rows = run.stream --text sh -c "yes"
    let first = collect { for r in rows { yield r; break } }
    assert first == ["y"]
  }
  assert live_children() == "", live_children()
  assert descriptor_count() == before
}
""", status: 0)?
}

test test_partial_input_and_output_leave_no_children_or_descriptors { |ctx|
  guard linux_only()? else { return }
  test.expect(ctx, oracle + r"""proc main() [fs, process, error] {
  let before = descriptor_count()
  let big = bytes.zero(4194304)?
  for _ in range(8) {
    let unread = run.capture --bytes sh -c "exit 0" < $big
    assert unread.status.exited_with(0)
    let one = run.capture --text head -c 1 < $big
    assert one.status.exited_with(0)
    let closed = try run.text sh -c "exec 0<&-; sleep 0.1; echo done" < $big
    assert closed is Ok(_)
    let rows = run.stream --bytes --timeout=3s cat < $big
    let first = collect { for r in rows { yield r; break } }
    assert first.len() == 1
    let killed = run.status sh -c "kill -9 $$"
    assert killed.signal_number()? == 9
  }
  assert live_children() == "", live_children()
  assert descriptor_count() == before
}
""", status: 0)?
}

test test_children_inherit_only_descriptors_a_script_installs_deliberately { |ctx|
  guard linux_only()? else { return }
  test.expect(ctx, r"""let lister = r"for f in /proc/$$/fd/*; do echo ${f##*/}; done"
proc child_fds() [process, error] -> Str {
  let text = run.text sh -c $lister
  text.split("\n") |> where . != "" |> sort |> join(",")
}
proc main() [fs, process, error] {
  let baseline = child_fds()
  let a = unix.open_fd(p"/dev/null")?
  let b = unix.open_fd(p"/dev/null", write: true)?
  let c = unix.duplicate_fd(a)?
  let pty = unix.open_pty()?
  let command = process.command_argv("sh", ["sh", "-c", lister])
  let spawned = unix.spawn_process_group(command, notify: true)?
  assert spawned.notify_fd > 2
  assert child_fds() == baseline
  let rows = run.stream --text sh -c $lister
  let streamed = collect { for r in rows { yield r } }
  assert (streamed |> sort |> join(",")) == baseline
  unix.dup_fd(a, 9)?
  assert child_fds() == f"{baseline},9"
}
""", status: 0)?
}

test test_released_handle_cannot_signal_a_recycled_pid { |ctx|
  guard linux_only()? else { return }
  let root = test.temp_dir(ctx, name: "recycled-pid")?
  let script = fp"{root}/recycle.xsh"
  script.write(r"""proc recycle() [fs, process, error] -> Result[Int] {
  let first = spawn run true ?
  let done = process.wait_any([first])?
  Path("/proc/sys/kernel/ns_last_pid").write(f"{done.pid - 1}")?
  let victim = unix.spawn_process_group(process.command_argv("sleep", ["sleep", "30"]))?
  assert victim.pid == done.pid
  Ok(victim.pid)
}
proc stale_cancel() [fs, process, error] -> Result[Int] {
  let first = spawn run true ?
  let done = process.wait_any([first])?
  Path("/proc/sys/kernel/ns_last_pid").write(f"{done.pid - 1}")?
  let victim = unix.spawn_process_group(process.command_argv("sleep", ["sleep", "30"]))?
  assert victim.pid == done.pid
  first.cancel(signal: "KILL")?
  Ok(victim.pid)
}
proc main() [fs, process, time, error] {
  let released = recycle()?
  time.sleep(200ms)
  assert process.kill(released, signal: "0") is Ok(_), "scope release signaled a recycled pid"
  process.kill(released, signal: "KILL")?
  let cancelled = stale_cancel()?
  assert process.kill(cancelled, signal: "0") is Ok(_), "stale cancel signaled a recycled pid"
}
""")?
  let probe = run.status unshare -Urpf --mount-proc true
  guard probe.exited_with(0) else {
    test.skip("recycling a pid needs an unprivileged user and pid namespace")
    return
  }
  let result = run.capture --text unshare -Urpf --mount-proc $ctx.xsh_bin $script
  assert result.status.exited_with(0), result.stderr
}
