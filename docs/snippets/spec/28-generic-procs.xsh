# begin example
proc wrap(value) { value }
let number: Int = wrap(42)?
let text: Str = wrap("demo")?
let nested: Result[Int] = wrap(Ok(7))?
let failed: Result[Int, Str] = wrap(Err("kept"))?
assert failed == Err("kept")
assert number == 42
assert text == "demo"
assert nested == Ok(7)
# end example
