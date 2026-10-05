pure percent(value: Int) -> Result[Int] {
  fail f"{value} is not a percentage" unless value >= 0 and value <= 100
  Ok(value)
}

match percent(140) {
  Ok(value) => print $value
  Err(problem) => print $problem.message
}
