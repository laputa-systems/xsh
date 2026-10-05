# begin example
let sizes: List[Int] = collect {
  let seen = [1, 2] |> map {
    yield . # error: check.yield
    . + 1
  }
  yield seen.len()
}
# end example
