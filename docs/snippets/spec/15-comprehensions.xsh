const sources = [p"main.c", p"util.c", p"README"]
const left = ["a", "b"]
const right = ["b", "c"]
let entries = fs.files(p".")?
# begin example
let objects = [fp"{src}.o" for src in sources if src.ext == "c"]
let sizes = {e.path: e.size for e in entries}
let pairs = [
  f"{a}-{b}"
  for a in left
  for b in right
  if a != b
]
# end example
