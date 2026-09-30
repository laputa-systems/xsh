pure unit() -> Unit {}
pure boolean() -> Bool { false }
pure early() -> Bool { return false }
pure optional(flag: Bool) -> Int? { if flag { 7 } else { null } }
pure result() -> Result[Int] { 9 }
pure nested() -> Result[Result[Int]] { Ok(Ok(11)) }
let checked: Unit = unit()
print ${boolean()} ${early()} ${optional(false) ?? 0} ${result()?} ${(nested()?)?}
