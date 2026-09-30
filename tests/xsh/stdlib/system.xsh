test test_system_module [env, error] {
  (system.hostname()? != "")
  let uname = system.uname()?
  (uname.sysname != "")
  let memory = system.memory()?
  (memory.total > 0)
  (memory.free >= 0)
  (memory.swap_total >= 0)
  let release = system.os_release()?
  (release.name != "")
  (release.pretty_name != "")
  (release.id != "")
  let units = system.execution_units()?
  (units.page_size_bytes > 0)
  (units.clock_ticks_per_second > 0)
}

test test_system_memory_reads_the_host_text [env, error] {
  guard system.uname()?.sysname == "Linux" else {
    # The entry reads `/proc/meminfo` on Linux only.
    test.skip("system.memory reads /proc/meminfo on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than values a host could change: every reported count is a
  # kilobyte count scaled to bytes, and a free or available count is part of the
  # total it is reported next to.
  let memory = system.memory()?
  (memory.total > 0)
  (memory.total % 1024) == 0
  (memory.free >= 0 and memory.free <= memory.total)
  (memory.available >= 0 and memory.available <= memory.total)
  (memory.swap_free >= 0 and memory.swap_free <= memory.swap_total)
}

test test_system_os_release_reads_the_host_text [env, error] {
  guard system.uname()?.sysname == "Linux" else {
    # The entry reads `/etc/os-release` on Linux only.
    test.skip("system.os_release reads /etc/os-release on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than values a distribution could change: the identification is
  # resolved, the resolved pretty name is never empty because it defaults to the
  # resolved name, and the unquoted identifier carries no white space. The
  # fallback to `/usr/lib/os-release` is covered by the fixed-path host test.
  let release = system.os_release()?
  (release.name != "")
  (release.id != "")
  (release.pretty_name != "")
  (" " not in release.id)
  release.id == release.id.trim()
}
