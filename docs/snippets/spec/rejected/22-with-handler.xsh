proc unused() [error] {
  # begin example
  with value = Ok(1) {
    print $value
  } else { |failure| # error: parse.with-handler
    print $failure.message
  }
  # end example
}
