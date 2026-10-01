proc clock(value: Int) [time] -> Int { value }
proc setting(value: Int) [env] -> Int { value }
pure pick(select: Bool) { if select { (clock) } else { (setting) } }
proc inspect(select: Bool) [time] -> Int { let callback = pick(select); let _ = callback.call(3); 1 }
