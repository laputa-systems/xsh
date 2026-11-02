##! Higher-order utilities used by full inference capability tests.

## Applies a callback to its value.
export proc apply(transform, value) {
  transform(value)
}

## Yields callback results from a lazy producer.
export stream apply_many(transform, value, count) {
  for _ in range(count) {
    yield apply(transform, value)
  }
}
