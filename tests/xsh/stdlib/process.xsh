proc test_process_module() [fs, process, error] {
  let current_pid = process.current_pid()?
  test.ok(current_pid > 0)?
  test.ok(process.list()? |> any .pid == current_pid, "process list should contain current pid")?
  test.ok(process.threads(current_pid)? |> any .owner_pid == current_pid, "process threads should accept a pid")?
  let stats = process.stats(current_pid)?
  test.ok(stats.rss_kb >= 0)?
  test.ok(stats.vsz_kb >= 0)?
  test.ok(process.list()? |> any .pid > 0, "process list should contain entries")?
  test.ok(process.which("sh")?.display() != "")?
  test.ok((process.port(9)? |> count()) >= 0)?
  test.ok((process.ports()? |> count()) >= 0)?
  test.ok((process.ports(current_pid)? |> count()) >= 0)?
  test.eq(process.signal("TERM")?.name, "TERM")?
  test.eq(process.argv_words("cmd 'two words'")?, ["cmd", "two words"])?
  let command = process.command_argv("sh", ["sh", "-c", "exit 7"])
  let status = process.run(command)?
  test.ok(status.exited_with(7))?

  let ok_command = process.command {
    run true
  }

  test.ok(process.run(ok_command)?.exited_with(0))?
  let sleeper = process.command_argv("sh", ["sh", "-c", "sleep 5"])
  let child = process.spawn(sleeper)?
  test.ok(child.pid > 0)?
  process.kill(child.pid, signal: "TERM")?
  let any_handle = spawn run true ?
  let any = process.wait_any([any_handle])?
  test.eq(any.index, 0)?
  test.ok(any.pid > 0)?
  test.ok(any.status.exited_with(0))?
  let ready_one = spawn run true ?
  let ready_two = spawn run true ?
  let ready = process.wait_ready([ready_one, ready_two])?
  test.ok(ready.len() >= 1)?
  test.ok(ready[0].status.exited_with(0))?
  let handle = spawn run sh -c "sleep 5" ?
  handle.cancel(signal: "TERM", kill_after: 10ms)?
}
