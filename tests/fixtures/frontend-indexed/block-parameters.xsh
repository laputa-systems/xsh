error HeaderError = failed(message: Str)

pure failed() -> Result[Int, HeaderError] {
  Err(HeaderError.failed(message: "failed"))
}

proc selected() [] -> Int {
  with first = Ok(1), second = first + 1 {
    return second
  } else { |_| return 0 }
}

proc main() [] {
  guard let value = failed() else { |failure|
    print ${failure.message} ${selected()}
    return
  }
  print $value
}
