pure first(value: Int) -> Int { value }
pure second(value: Int) -> Int { value }
pure inspect(select: Bool) -> Int { let chosen = first; let _ = chosen.call(true); 1 }
