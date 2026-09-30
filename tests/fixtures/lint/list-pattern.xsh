pure select(input: List[Str]) -> Str {
  let values = input
  if values.len() == 2 and values[0] == "build" {
    let target = values[1]
    target
  } else {
    "other"
  }
}

pure prefix(input: List[Int]) -> Int {
  let values = input
  if values.len() >= 2 and values[0] == 7 {
    let target = values[1]
    target
  } else {
    0
  }
}
