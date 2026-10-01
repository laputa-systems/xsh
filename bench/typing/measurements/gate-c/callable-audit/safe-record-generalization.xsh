pure identity(value) { value }
let boxed = {callback: identity}
let integer: Int = boxed.callback(7)
let text: Str = boxed.callback("word")
