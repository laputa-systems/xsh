error FirstError = Invalid
error SecondError = Invalid

pure check_failure(message: Str) {
  .Invalid(message)
}

print check_failure("invalid input").message
