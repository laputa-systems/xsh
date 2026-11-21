let rows = [{name: "a", n: 1}, {name: "b", n: 1}, {name: "c", n: 2}, {name: "d", n: 1}]
let down = rows |> sort-by(desc: true) .n
let up = rows |> sort-by .n
print f"desc: {[r.name for r in down].join(" ")}"
print f"asc: {[r.name for r in up].join(" ")}"
