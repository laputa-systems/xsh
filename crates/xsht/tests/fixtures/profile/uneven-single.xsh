pure work(n: Int) -> Int {
  var total = 0
  for i in range(n * 100000) { total += i }
  total
}
let _ = [1, 1, 1, 20] |> par-map(jobs: 2) { |n| work(n) } |> collect()
