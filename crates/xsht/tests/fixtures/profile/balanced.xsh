pure work(n: Int) -> Int {
  var total = 0
  for i in range(n * 100000) { total += i }
  total
}
let _ = [5, 5, 5, 5] |> par-map(jobs: 2) { |n| work(n) } |> collect()
