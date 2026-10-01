pure identity(value) { value }
pure inspect() -> Int { var current = identity; let alias = current; let _ = alias.call(7); let _ = alias.call("word"); 1 }
