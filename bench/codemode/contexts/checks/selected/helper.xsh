##! A helper module for the guide checks.

## Failure family.
export error HelperError = Bad | Worse

## Greets.
export pure greet(name: Str) -> Str {
  f"hello {name}"
}

## Always fails.
export pure fail() -> Result[Int, HelperError] {
  Err(HelperError.Bad("bad"))
}
