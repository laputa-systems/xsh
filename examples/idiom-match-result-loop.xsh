let inputs = ["1", "bad", "2", "also-bad", "3"]

error ParseError = InvalidDigit(message: Str) : InvalidData

pure to_int(s: Str) -> Result[Int] {
  if s == "1" {
    return 1
  }

  if s == "2" {
    return 2
  }

  if s == "3" {
    return 3
  }

  return Err(ParseError.InvalidDigit(message: f"not a digit: ${s}"))
}

var total = 0

for item in inputs {
  var n = 0

  match to_int(item) {
    Ok(v) => n = v
    Err(_) => continue
  }

  total += n
}

print $total
