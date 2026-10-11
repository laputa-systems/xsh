##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_uptime.rs.

use support.uu as uu

# origin: uutils test_uptime::test_invalid_arg
test test_uu_uptime_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uptime", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_uptime::test_uptime
test test_uu_uptime_uptime { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uptime", [])?
  uu.succeeds(r)
  uu.stdout_contains(r, " up ")
  uu.stdout_contains(r, "load average:")
  assert ! (",  ," in r.stdout.utf8()?)
}

# origin: uutils test_uptime::test_uptime_for_file_without_utmpx_records
test test_uu_uptime_uptime_for_file_without_utmpx_records { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "hello")?
  let r = uu.invoke(s, "uptime", [uu.at(s, "file1").display()])?
  uu.fails(r)
  uu.stderr_contains(r, "uptime: couldn't get boot time")
  uu.stdout_contains(r, "up ???? days ??:??")
  uu.stdout_contains(r, "load average")
}

# origin: uutils test_uptime::test_uptime_with_dir
test test_uu_uptime_uptime_with_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  let r = uu.invoke(s, "uptime", ["dir1"])?
  uu.fails(r)
  uu.stderr_contains(r, "uptime: couldn't get boot time: Is a directory")
  uu.stdout_contains(r, "up ???? days ??:??")
}

# origin: uutils test_uptime::test_uptime_with_fifo
test test_uu_uptime_uptime_with_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo1")?
  uu.write(s, "a", "hello")?
  let cp = process.which("cp")?
  let writer = spawn process.command_argv(cp, [cp, uu.at(s, "a"), uu.at(s, "fifo1")], s.root, {}, b"", uu.at(s, "writer-out"), uu.at(s, "writer-err"), timeout: 5s)?
  defer writer.cancel(kill_after: 100ms)?
  let r = uu.invoke(s, "uptime", ["fifo1"], timeout: 5s)?
  uu.fails(r)
  uu.stderr_contains(r, "uptime: couldn't get boot time")
  uu.stdout_contains(r, "up ???? days ??:??")
  uu.stdout_contains(r, "load average")
}

# origin: uutils test_uptime::test_uptime_with_non_existent_file
test test_uu_uptime_uptime_with_non_existent_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uptime", ["file1"])?
  uu.fails(r)
  uu.stderr_contains(r, "uptime: couldn't get boot time: No such file or directory")
  uu.stdout_contains(r, "up ???? days ??:??")
}

# origin: uutils test_uptime::test_write_error_handling
test test_uu_uptime_write_error_handling { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uptime", [], stdout: p"/dev/full")?
  uu.fails(r)
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "No space left on device")
}

