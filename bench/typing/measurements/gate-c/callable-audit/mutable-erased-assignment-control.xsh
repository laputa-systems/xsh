pure first(value: Int) -> Int { value }
pure second(value: Str) -> Str { value }
pure inspect() -> Int { var current: Pure = first; current = second; 1 }
