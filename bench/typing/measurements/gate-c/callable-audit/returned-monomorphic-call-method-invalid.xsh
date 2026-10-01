pure identity(value) { value }
pure factory(unused: Int) { (identity) }
pure inspect() -> Int { let callback = factory(1); let _ = callback.call(7); let _ = callback.call("word"); 1 }
