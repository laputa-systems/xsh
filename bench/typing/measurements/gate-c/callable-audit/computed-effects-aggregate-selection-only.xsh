proc clock(value: Int) [time] -> Int { let _ = time.now(); value }
proc setting(value: Int) [env] -> Int { let _ = env.get("NOT_READ"); value }
pure pick(select: Bool) { if select { (clock) } else { (setting) } }
proc inspect(select: Bool) [] -> Int { let boxed = {callback: pick(select)}; let callbacks = [boxed.callback]; let _ = callbacks; 3 }
