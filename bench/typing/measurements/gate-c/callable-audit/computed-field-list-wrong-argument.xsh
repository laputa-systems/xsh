pure first(value: Int, offset: Int = 1) -> Int { value + offset }
pure second(value: Int, offset: Int = 2) -> Int { value + offset }
pure inspect(select: Bool) -> Int { let chosen = if select { (first) } else { (second) }; let boxed = {callback: chosen}; let callbacks = [boxed.callback]; callbacks[0](value: true) }
