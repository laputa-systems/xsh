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
