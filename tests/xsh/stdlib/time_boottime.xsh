test test_time_boottime_is_a_nanosecond_count_since_boot_that_never_moves_backward {
  let first = time.boottime()?
  time.sleep(3ms)
  let second = time.boottime()?
  assert first > 0
  assert second - first >= 3000000
  assert time.boottime()? >= second
}

test test_time_boottime_tracks_proc_uptime_on_linux {
  if system.uname()?.sysname != "Linux" {
    test.skip("/proc/uptime is Linux-only")
  }
  let before = Path("/proc/uptime").read_text()?.split(" ")[0].parse_float()?
  let boot = time.boottime()?
  let after = Path("/proc/uptime").read_text()?.split(" ")[0].parse_float()?
  let seconds = boot.float() / 1000000000.0
  assert before - 0.05 <= seconds
  assert seconds <= after + 0.05
}
