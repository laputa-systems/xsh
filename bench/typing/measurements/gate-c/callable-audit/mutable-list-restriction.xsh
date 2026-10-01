pure identity(value) { value }
var callbacks = [identity]
let integer: Int = callbacks[0](7)
let text: Str = callbacks[0]("word")
