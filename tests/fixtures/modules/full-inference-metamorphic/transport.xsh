##! Generic values transferred through procedures and streams.

## Returns the supplied value.
export proc preserve(value) {
  value
}

## Yields the supplied value the requested number of times.
export stream replay(value, count) {
  repeat count times {
    yield value
  }
}
