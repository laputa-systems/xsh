pure outer(value) { pure inner() { value }; inner() }
let integer: Int = outer(7)
let text: Str = outer("word")
