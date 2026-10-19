proc private_wrap(value) { value }

export proc wrap(value) {
  private_wrap(value)
}
