pure identity(value) { value }
pure unit() -> Unit {}
let number: Int = identity(1)
let text: Str = identity("one")
let no: Bool = identity(false)
let nothing: Unit = identity(unit())
let maybe: Int? = 7
let nullable: Int? = identity(maybe)
let nested: Result[Result[Int]] = Ok(Ok(9))
let retained: Result[Result[Int]] = identity(nested)
print $number $text $no ${nullable ?? 0} ${(retained?)?}
