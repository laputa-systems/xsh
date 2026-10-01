pure identity(value) { value }
var boxed = {callback: identity}
let integer: Int = boxed.callback(7)
let text: Str = boxed.callback("word")
