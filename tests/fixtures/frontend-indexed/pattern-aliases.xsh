pure select(values: List[List[Int]]) -> Int {
  match values {
    ([[value, ..tail], [99]] | [[99], [value, ..tail]]) as original => value + tail.len() + original.len()
    _ => 0
  }
}

pure reordered(values: List[Int]) -> Int {
  match values {
    [left, right] | [right, left] => left * 10 + right
    _ => 0
  }
}
