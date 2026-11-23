var count = 0
proc mark(value: Int) -> Int {
  count += 1
  return value * 10
}
let serial = [3, 1, 2] |> par-map(jobs: 1) { |item| mark(item) } |> take(2)
print "serial-count" $count
for item in serial { print $item }
count = 0
let parallel = [3, 1, 2] |> par-map(jobs: 2) { |item| mark(item) } |> take(2)
print "parallel-count" $count
for item in parallel { print $item }
