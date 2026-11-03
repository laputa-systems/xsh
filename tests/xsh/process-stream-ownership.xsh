test test_break_reaps_process_stream_before_continuing {
  var pid = 0
  let rows = run.stream --text sh -c "printf '%s\\n' $$; exec sleep 30" ?
  for row in rows {
    pid = row as Int
    break
  }
  assert pid > 0
  test.error_kind(process.kill(pid, signal: "0"), "process-missing")
}

test test_failed_notify_spawns_release_pipe_descriptors { |ctx|
  if system.uname()?.sysname != "Linux" { return }
  test.expect(ctx, r"""proc descriptor_count() [fs, error] -> Int {
  fs.children(p"/proc/self/fd")? |> count()
}
let command = process.command_argv("/xsh-missing-notify-worker", ["/xsh-missing-notify-worker"])
let before = descriptor_count()
for _ in range(32) {
  assert unix.spawn_process_group(command, notify: true) is Err(_)
}
assert descriptor_count() == before
""", status: 0)?
}
