pure identity(value) { value }
pure factory(unused: Int) { (identity) }
let callback = factory(1)
let integer: Int = callback(7)
let text: Str = callback("word")
