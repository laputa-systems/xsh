pure first(value: Int) -> Int { value }
pure second(value: Int) -> Int { value }
pure inspect(select: Bool) -> Int { let chosen: Pure = if select { (first) } else { (second) }; let _ = chosen.call(true); 1 }
