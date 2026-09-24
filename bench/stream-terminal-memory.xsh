# Run one terminal per process so peak RSS reflects a single live consumption.
stream numbers() [] -> Stream[Int] {
  for n in range(1000000) {
    yield n
  }
}

proc main(...argv: List[Str]) [io, error] {
  let terminal = argv[0]
  var value = 0
  if terminal == "count" {
    value = numbers() |> count()
  } else if terminal == "last" {
    value = numbers() |> last()?
  } else if terminal == "min" {
    value = numbers() |> min()?
  } else if terminal == "max" {
    value = numbers() |> max()?
  } else {
    abort(2)
  }
  print f"value=${value}"
}
