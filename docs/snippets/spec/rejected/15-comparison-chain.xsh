proc unused(offset: Int, limit: Int) {
  # begin example
  let inside = 0 <= offset < limit # error: parse.comparison-chain
  # end example
}
