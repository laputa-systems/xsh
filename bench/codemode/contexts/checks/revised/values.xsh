type Row = {name: Str, size: Int}

const limit = 3
let fixed = "a"
var seen: List[Str] = []
var counts: Map[Int] = {}
var cursor: Str? = null
seen += [fixed]
counts["x"] = 1
cursor = "next"
let name = "b"
let size = 2
let punned = {name, size}
let built = Row(name: "c", size: limit)
let converted = "8" as Int
let fallback = "eight".parse_int() ?? 0
let short = 100ms
let long = 2s
print f"{seen.join(",")} {counts.get("x") ?? 0} {cursor ?? "none"} {punned.name}{punned.size} {built.size}"
print f"{converted + 1} {fallback} {short} {long}"
print "{name} stays literal"
print (seen.len() + 1)
print $name
eprint "to stderr"
