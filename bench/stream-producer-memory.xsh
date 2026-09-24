# Run one mode per process so peak RSS belongs to a single consumption path.
# Usage: xsh bench/stream-producer-memory.xsh -- MODE COUNT
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
  if mode == "first" {
    let value = numbers(size) |> first()?
    print f"value=${value}"
  } else if mode == "take-first" {
    let value = numbers(size) |> take(1) |> par-map --jobs=8 { |n| n } |> first()?
    print f"value=${value}"
  } else if mode == "par-first" {
    let value = numbers(size) |> par-map --jobs=8 { |n| n } |> first()?
    print f"value=${value}"
  } else if mode == "count" {
    let value = numbers(size) |> count()
    print f"value=${value}"
  } else if mode == "fused" {
    let totals = numbers(size)
      |> par-map --jobs=8 { |n| n }
      |> reduce-by --sum { |n| {key: "all", value: 1} }
    print f"value=${totals.get("all", 0)}"
  } else if mode == "unfused" {
    let totals = numbers(size)
      |> par-map --jobs=8 { |n| n }
      |> reduce-by --sum --jobs=8 { |n| {key: "all", value: 1} }
    print f"value=${totals.get("all", 0)}"
  } else {
    abort(2)
  }
}
