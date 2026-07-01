#!/usr/bin/env -S xsh --
pure ratio_text(used: Int, total: Int) -> Str {
  if total <= 0 {
    return "0%"
  }

  return f"${used * 100 / total}%"
}

proc main(...argv: List[Str]) [fs, process, env, time, error] {
  if argv.len() > 0 and argv[0] == "--help" {
    print "usage: xfetch.xsh"
    return
  }

  let current = user.current()?
  let host = system.hostname()?
  let os = system.os_release()?
  let uname = system.uname()?
  let uptime = unix.uptime_seconds()?
  let memory = system.memory()?
  let root = fs.mount_for(/)?
  let mem_used = memory.total - memory.available
  print f"${current.name}@${host}"
  print f"OS      ${os.pretty_name}"
  print f"Kernel  ${uname.release}"
  print f"Arch    ${uname.machine}"
  print f"Uptime  ${time.duration_compact(uptime)}"
  print f"CPU     ${cpu.count()}"
  print f"Memory  ${bytes.human(mem_used)} / ${bytes.human(memory.total)} (${ratio_text(mem_used, memory.total)})"

  match env.get("SHELL") {
    Ok(shell) => print f"Shell   ${shell}"
    Err(_) => {}
  }

  print f"Root    ${bytes.human(root.used_1k * 1024)} / ${bytes.human(root.blocks_1k * 1024)} (${root.capacity_percent}%)"
}
