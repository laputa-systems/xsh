pure identity(value) { value }
var current = identity
let alias = current
let integer: Int = alias(7)
let text: Str = alias("word")
