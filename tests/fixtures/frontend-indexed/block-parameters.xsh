error HeaderError = failed(message: Str)

pure failed() -> Result[Int, HeaderError] {
  Err(HeaderError.failed(message: "failed"))
}

proc selected() [] -> Int {
  match Ok(1) {
    Ok(first) => { return first + 1 }
    Err(_) => { return 0 }
  }
}

proc main() [] {
  guard let value = failed() else { |failure|
    print ${failure.message} ${selected()}
    return
  }
  print $value
}
