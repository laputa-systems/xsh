error LocalError = Bad(message: Str)
pure wrap(value, gate: Result[Unit]) { gate?; value }
let gate: Result[Unit] = Ok()
let inner: Result[Int] = Ok(7)
let nested: Result[Result[Int]] = wrap(inner, gate)
print ${(nested?)?}
let boolean: Result[Bool] = wrap(false, gate)
print ${boolean?}
pure unit() -> Unit {}
let nothing: Result[Unit] = wrap(unit(), gate)
let maybe: Int? = null
let nullable: Result[Int?] = wrap(maybe, gate)
let failed: Result[Int] = Err(LocalError.Bad("inner error"))
let error_data: Result[Result[Int]] = wrap(failed, gate)
print ${nothing is Ok(_)} ${nullable? == null} ${error_data? is Err(_)}
