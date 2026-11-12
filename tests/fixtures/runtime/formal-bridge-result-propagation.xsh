pure keeps(value: Int) -> Result[Int, Int] {
  return Ok(value + 1)
}

pure stops(value: Int) -> Result[Int, Int] {
  return Err(value)
}

pure passes(value: Int) -> Result[Int, Int] {
  let kept = keeps(value)?
  return Ok(kept)
}

pure blocks(value: Int) -> Result[Int, Int] {
  let kept = stops(value)?
  return Ok(kept)
}
