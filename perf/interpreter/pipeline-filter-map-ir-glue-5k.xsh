pure make_row(name: Str, weight: Int) -> Record {
  return {name: name, weight: weight}
}

pure score(rows: List[Record], min_weight: Int, offset: Int) -> Int {
  let selected = rows
    |> where .enabled and .weight >= min_weight
    |> map make_row(.name.lower(), .weight + offset)
    |> drop(1)
    |> take(8)

  var total = 0

  for row in selected {
    total += row.name.count_chars() + row.weight
  }

  return total
}

let rows = [
  {name: "Alpha", weight: 3, enabled: true},
  {name: "Beta", weight: 7, enabled: false},
  {name: "Gamma", weight: 11, enabled: true},
  {name: "Delta", weight: 13, enabled: true},
  {name: "Epsilon", weight: 17, enabled: true},
  {name: "Zeta", weight: 19, enabled: false},
  {name: "Eta", weight: 23, enabled: true},
  {name: "Theta", weight: 29, enabled: true},
  {name: "Iota", weight: 31, enabled: true},
  {name: "Kappa", weight: 37, enabled: true},
]

var i = 0
var total = 0

while i < 5000 {
  total += score(rows, i % 19, i % 5)
  i += 1
}

print $total % 256
