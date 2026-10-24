##! Generic procedure wrappers for checked module imports.

proc private_wrap(value) { value }

## Returns its input through the procedure's Result envelope.
export proc wrap(value) {
  private_wrap(value)
}
