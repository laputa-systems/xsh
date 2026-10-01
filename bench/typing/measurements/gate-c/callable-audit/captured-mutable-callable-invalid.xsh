pure identity(value) { value }
var current = identity
pure apply(value) { current.call(value) }
let integer = apply(7)
let text = apply("word")
