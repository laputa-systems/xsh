proc parsed(value: Str) [error] -> Int { value.parse_int()? }
print ${parsed("7")}
let failure: Result[Int] = try { parsed("bad") }
print ${failure is Err(_)}
