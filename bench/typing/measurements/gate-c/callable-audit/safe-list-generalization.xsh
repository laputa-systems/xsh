pure identity(value) { value }
let callbacks = [identity]
let integer: Int = callbacks[0](7)
let text: Str = callbacks[0]("word")
