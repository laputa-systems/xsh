pure first(value: Int) -> Int { value }
pure factory() { (first) }
pure inspect() -> Int { let callback = factory(); let _ = callback.call(true); 1 }
