type Row = {name: Str, rank: Int, enabled: Bool}

pure row(name: Str, rank: Int, enabled: Bool) -> Row {
  return {name: name, rank: rank, enabled: enabled}
}

pure score(rows: List[Row], seed: Int) -> Int {
  let names = rows
    |> where .enabled
    |> sort-by .rank
    |> map .name.lower()
    |> sort

  var total = 0

  for name in names {
    total += name.count_chars() + seed % 7
  }

  return total
}

let rows = [
  row("delta", 40, true),
  row("alpha", 10, true),
  row("echo", 50, false),
  row("charlie", 30, true),
  row("bravo", 20, true),
]

var i = 0
var total = 0

while i < 5000 {
  total += score(rows, i)
  i += 1
}

print $total % 256
