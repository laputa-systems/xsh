let files = [{name: "small.txt", size: 1}, {name: "large.txt", size: 100}, {name: "mid.txt", size: 50}]

for f in files |> sort-by --desc .size {
  print $f.name
}

let words = ["banana", "apple", "cherry"]

for w in words |> sort-by --desc . {
  print $w
}
