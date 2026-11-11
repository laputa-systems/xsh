pure keeps(value: Int) -> Result[Int, Int] {
  return Ok(value)
}

pure stops(value: Int) -> Result[Int, Int] {
  return Err(value)
}

pure adds(value: Int) -> Result[Int, Int] {
  return Ok(value + 1)
}
