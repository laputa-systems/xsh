proc leaf(value: Int) [error] -> Result[Int] {
  return Ok(value)
}

proc middle(value: Int) [error] -> Result[Int] {
  leaf(value)?
}

proc descend(value: Int) [error] -> Result[Int] {
  if value <= 0 {
    return Ok(value)
  }
  descend(value - 1)?
}

proc main() [error] -> Result[Unit] {
  let values = [1, 2] |> par-map --jobs=2 { |value|
    middle(value)
  }
  print values.len()

  let deep_values = [12000, 12001] |> par-map --jobs=2 { |value|
    descend(value)
  }
  print deep_values.len()
}

main()?
