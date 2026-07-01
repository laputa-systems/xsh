error ParseError = Invalid(message: Str)

pure octal_mode(raw: Str) -> Result[Int] {
  var mode = 0

  for ch in raw.split("") {
    var digit = 0

    match ch {
      "0" => digit = 0
      "1" => digit = 1
      "2" => digit = 2
      "3" => digit = 3
      "4" => digit = 4
      "5" => digit = 5
      "6" => digit = 6
      "7" => digit = 7
      _ => return Err(ParseError.Invalid(message: f"invalid octal digit '${ch}'"))
    }

    mode = mode * 8 + digit
  }

  return mode
}

let mode = octal_mode("755")?
print $mode
