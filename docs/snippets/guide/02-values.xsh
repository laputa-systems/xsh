type Row = {name: Str, size: Int}

const limit = 3
var seen: List[Str] = []
var counts: Map[Int] = {}
var cursor: Str? = null

let row = Row(name: "a.txt", size: "12" as Int)
seen += [row.name]
counts["txt"] = (counts.get("txt") ?? 0) + 1
cursor = "next-page"

print f"{seen.join(",")} {counts.get("txt") ?? 0} {cursor ?? "none"} {row.size * limit}"
print "plain strings keep {braces}"
print (json.encode({sizes: [row.size, 4], name: row.name})?)
