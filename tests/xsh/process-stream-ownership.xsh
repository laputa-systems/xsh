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
  let worker = fp"{test.temp_dir(ctx, name: "failed-notify-spawn")?}/worker"
  worker.write("#!/xsh-missing-interpreter\n")?
  worker.chmod(0o755)?
  test.expect(ctx, r"""proc descriptor_count() [fs, error] -> Int {
  fs.children(p"/proc/self/fd")? |> count()
}
let worker = Path(args[0])
let command = process.command_argv(worker, [worker])
let before = descriptor_count()
for _ in range(32) {
  assert unix.spawn_process_group(command, notify: true) is Err(_)
}
assert descriptor_count() == before
""", status: 0, args: [worker])?
}
