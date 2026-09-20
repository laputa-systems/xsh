proc test_system_module() [env, error] {
  test.ok(system.hostname()? != "")?
  let uname = system.uname()?
  test.ok(uname.sysname != "")?
  let memory = system.memory()?
  test.ok(memory.total > 0)?
  test.ok(memory.free >= 0)?
  test.ok(memory.swap_total >= 0)?
  let release = system.os_release()?
  test.ok(release.name != "")?
  test.ok(release.pretty_name != "")?
  test.ok(release.id != "")?
}

proc test_system_memory_reads_the_host_text() [env, error] {
  if system.uname()?.sysname != "Linux" {
    # The entry reads `/proc/meminfo` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("system.memory reads /proc/meminfo on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than values a host could change: every reported count is a
  # kilobyte count scaled to bytes, and a free or available count is part of the
  # total it is reported next to.
  let memory = system.memory()?
  test.ok(memory.total > 0)?
  test.eq(memory.total % 1024, 0)?
  test.ok(memory.free >= 0 and memory.free <= memory.total)?
  test.ok(memory.available >= 0 and memory.available <= memory.total)?
  test.ok(memory.swap_free >= 0 and memory.swap_free <= memory.swap_total)?
}

proc test_system_os_release_reads_the_host_text() [env, error] {
  if system.uname()?.sysname != "Linux" {
    # The entry reads `/etc/os-release` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("system.os_release reads /etc/os-release on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than values a distribution could change: the identification is
  # resolved, the resolved pretty name is never empty because it defaults to the
  # resolved name, and the unquoted identifier carries no white space. The
  # fallback to `/usr/lib/os-release` cannot be reached on a host that has
  # `/etc/os-release`, so it stays covered by the reading helper's own tests.
  let release = system.os_release()?
  test.ok(release.name != "")?
  test.ok(release.id != "")?
  test.ok(release.pretty_name != "")?
  test.ok(! release.id.contains(" "))?
  test.eq(release.id, release.id.trim())?
}
