type SliceRow = {label: Str, value: Int}

error SliceError = Negative(message: Str)

pure direct(value: Int) -> Int {
  return value + 1
}

pure factorial(value: Int) -> Int {
  if value <= 1 {
    return 1
  }

  return value * factorial(value - 1)
}

pure even(value: Int) -> Bool {
  if value == 0 {
    return true
  }

  return odd(value - 1)
}

pure odd(value: Int) -> Bool {
  if value == 0 {
    return false
  }

  return even(value - 1)
}

pure nonnegative(value: Int) -> Result[Int] {
  if value < 0 {
    return Err(SliceError.Negative("negative input"))
  }

  return Ok(value)
}

proc exact_error_site() [error] -> Result[Int] {
  return bytes.unpack_be(b"", 1, 0)?
}

proc main() {
  let offset = 3
  let captured = [1, 2]
    |> map { |item| item + offset }
    |> collect()
  var total = direct(captured[0])
  var index = 0

  loop {
    index += 1
    continue when index == 2
    total += index
    break when index >= 4
  }

  guard let positive = nonnegative(total) else |failure| {
    print $failure.message
    return
  }

  let selected = match positive {
    value if value > 0 => value,
    _ => 0,
  }
  let row: SliceRow = {label: "slice", value: selected}
  print $row.label $row.value ${factorial(5)} ${even(4)} ${odd(3)}
}
