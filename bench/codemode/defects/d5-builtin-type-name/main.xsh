type Group = {name: Str, size: Int}

pure label(g: Group) -> Str {
  g.name
}

let g = Group(name: "a", size: 1)
print (label(g))
