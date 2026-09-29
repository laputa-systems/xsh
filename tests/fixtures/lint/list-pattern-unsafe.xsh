pure select(input: List[Str]) -> Str {
  var values = input
  if values.len() == 2 and values[0] == "build" {
    let target = values[1]
    target
  } else { "other" }
}

pure reversed(input: List[Str]) -> Str {
  let values = input
  if values[0] == "build" and values.len() == 2 {
    let target = values[1]
    target
  } else { "other" }
}

pure beyond(input: List[Str]) -> Str {
  let values = input
  if values.len() >= 1 and values[0] == "build" {
    let target = values[1]
    target
  } else { "other" }
}

pure annotated(input: List[Str]) -> Str {
  let values = input
  if values.len() == 2 and values[0] == "build" {
    let target: Str = values[1]
    target
  } else { "other" }
}

pure commented(input: List[Str]) -> Str {
  let values = input
  if values.len() == 2 and values[0] == "build" {
    # Keep the extraction comment.
    let target = values[1]
    target
  } else { "other" }
}
