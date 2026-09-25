stream numbers(size: Int) [] -> Stream[Int] {
  var n = 0
  while n < size {
    yield n
    n += 1
  }
}

proc main(...argv: List[Str]) [io, error] {
  let mode = argv[0]
  let size = argv[1].parse_int()?
  var value = 0
  if mode == "count" {
    value = numbers(size) |> count()
  } else if mode == "map" {
    value = numbers(size) |> map { |n| n } |> count()
  } else if mode == "where" {
    value = numbers(size) |> where { |n| n >= 0 } |> count()
  } else if mode == "map-where" {
    value = numbers(size) |> map { |n| n } |> where { |n| n >= 0 } |> count()
  } else if mode == "par-map" {
    value = numbers(size) |> par-map --jobs=8 { |n| n } |> count()
  } else {
    abort(2)
  }
  print f"value=${value}"
}
