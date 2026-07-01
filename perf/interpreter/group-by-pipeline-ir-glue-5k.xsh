type Row = {name: Str, group: Str, weight: Int}

pure summarize(rows: List[Row], min_weight: Int) -> Str {
  let labels = rows
    |> where .weight >= min_weight
    |> group-by .group
    |> sort-by .key
    |> map { |bucket|
      f"${bucket.key}:${bucket.items.len()}:${bucket.items[0].name.lower()}"
    }

  return labels.join("|")
}

let rows = [
  {name: "Alpha", group: "net", weight: 3},
  {name: "Beta", group: "fs", weight: 7},
  {name: "Gamma", group: "net", weight: 11},
  {name: "Delta", group: "proc", weight: 13},
  {name: "Epsilon", group: "fs", weight: 17},
  {name: "Zeta", group: "proc", weight: 19},
]

var i = 0
var total = 0

while i < 5000 {
  total += summarize(rows, i % 17).count_chars()
  i += 1
}

print $total % 256
