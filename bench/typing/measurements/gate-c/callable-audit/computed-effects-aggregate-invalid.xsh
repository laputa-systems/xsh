proc clock(value: Int) [time] -> Int { let _ = time.now(); value }
proc setting(value: Int) [env] -> Int { let _ = env.get("NOT_READ"); value }
proc inspect(select: Bool) [time] -> Int { let chosen = if select { (clock) } else { (setting) }; let boxed = {callback: chosen}; let callbacks = [boxed.callback]; callbacks[0](value: 3) }
