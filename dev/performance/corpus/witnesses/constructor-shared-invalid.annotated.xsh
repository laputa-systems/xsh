error FirstError = Invalid
error SecondError = Invalid

pure check_failure(message: Str) -> FirstError {
  .Invalid(message)
}

print check_failure("invalid input").message
