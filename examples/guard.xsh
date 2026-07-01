error ParseError = InvalidPositive(message: Str) : InvalidData

pure to_positive(s: Str) -> Result[Int] {
  if s == "5" {
    return 5
  }

  if s == "10" {
    return 10
  }

  return Err(ParseError.InvalidPositive(message: f"not a known positive: ${s}"))
}

proc describe(s: Str) {
  guard let n = to_positive(s) else |e| {
    print f"skipped: ${e.message}"
    return
  }

  print f"${s} -> ${n * 2}"
}

describe("5")
describe("bad")
describe("10")
