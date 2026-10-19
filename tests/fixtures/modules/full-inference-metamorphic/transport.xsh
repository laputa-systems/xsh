export proc preserve(value) {
  value
}

export stream replay(value, count) {
  repeat count times {
    yield value
  }
}
