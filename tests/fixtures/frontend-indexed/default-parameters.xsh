error DefaultError = invalid(message: Str)

pure initial() -> Int { 4 }
pure choose(value = initial()) -> Int { value }
pure nested(value = choose() + 1) -> Int { value }
pure supplied() -> Int { nested(value: 9) }
pure missing() -> Result[Int, DefaultError] { Err(DefaultError.invalid(message: "default failed")) }
pure guarded(value = missing()?) -> Result[Int, DefaultError] { Ok(value) }
pure caught() -> Result[Int, DefaultError] { guarded() }
pure skipped() -> Result[Int, DefaultError] { guarded(8) }

pure alias_default() -> Int { let selected = choose; selected() }
pure alias_named() -> Int { let selected = choose; selected(value: 11) }
