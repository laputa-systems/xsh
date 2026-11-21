type Row = {name: Str, kind: Str, size: Int}

let rows = [
  Row(name: "a", kind: "x", size: 3),
  Row(name: "b", kind: "y", size: 1),
  Row(name: "c", kind: "x", size: 2),
  Row(name: "a", kind: "z", size: 9),
]
let names = rows |> where .size > 1 |> map .name
let sorted = names |> sort()
let biggest = rows |> sort-by(desc: true) .size |> take(2) |> map .name
let rest = rows |> drop(3) |> map .kind
let unique = rows |> unique-by .name |> count()
let total = rows |> map .size |> sum()
let all_big = rows |> all { .size > 0 }
let any_huge = rows |> any { .size > 8 }
let groups = rows |> group-by .kind |> map { |g| f"{g.key}={g.items.len()}" }
let folded = rows |> fold(0) { |acc, row| acc + row.size }
let squares = [1, 2, 3, 4] |> par-map(jobs: 3) { |n| n * n }
let ids = [r.name for r in rows if r.kind == "x"]
var counts: Map[Int] = {}
for row in rows {
  counts[row.kind] = (counts.get(row.kind) ?? 0) + 1
}

var keys: List[Str] = []
for {key, value} in counts {
  keys += [f"{key}{value}"]
}

print f"{sorted.join(",")} {biggest.join(",")} {rest.join(",")} {unique} {total} {all_big} {any_huge}"
print f"{groups.join(",")} {folded} {json.encode(squares)?} {ids.join(",")} {"b" in names} {json.encode(sorted[1..])?}"
print (keys.join(","))
