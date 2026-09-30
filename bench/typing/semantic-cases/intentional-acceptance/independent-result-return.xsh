pure parsed(value) { let parsed = value.parse_int()?; Ok(parsed) }
let result: Result[Int] = parsed("7")
print ${result?}
