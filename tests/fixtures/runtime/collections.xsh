let base = ["alpha"]
let pushed = base.push("beta")
let extended = pushed.extend(["gamma"])
let concatted = base.extend(["delta"]).extend(["epsilon"])
let list_fallback: Str = concatted.get(9, "omega")
let list_get_or: Str = concatted.get(10, "sigma")
let list_first: Str = concatted.get(0, "")
let m0: Map[Int] = {}
let m1 = m0.set("one", 1)
let m2 = m1.set("two", 2)
let m3 = m2.remove("one")
let got = m2.get("two")?
let fallback = m2.get("missing", 99)
let map_get_or = m2.get("missing", 100)
let keys = m2.keys()
let values = m2.values()
let missing = m3.get("one")
var built = ["seed"]
built = built.push("next")
var dynamic_counts: Map[Int] = {}
dynamic_counts["pkg"] = dynamic_counts.get("pkg", 0) + 1
dynamic_counts["pkg"] = dynamic_counts.get("pkg", 0) + 4
let empty_groups: Map[List[Str]] = {}
let groups = empty_groups.push("pkg", "one").push("pkg", "two").push("tool", "alpha")
let pkg_group = groups.get("pkg")?
let tool_group = groups.get("tool")?
let versions = {row.name: row.version for row in [{name: "pkg", version: "1"}, {name: "tool", version: "2"}]}
let row = {name: "pkg", version: "1"}
let fields = row.keys()
let record_name: Str = row.get("name")?
let missing_field = row.get("missing")

print extended.len() pushed[1] concatted.len() concatted[1] concatted.contains("delta") concatted.contains("zeta") $list_first $list_fallback $list_get_or

print $got m2.has("one") m3.has("one") $fallback $map_get_or keys[0] keys[1] values[0] values[1]
print built.len() built[1] dynamic_counts.get("pkg", 0) pkg_group[1] tool_group[0] versions.get("tool")?

match missing {
  Err(error) => print $error.message
}

print row.has("version") fields[0] $record_name

match missing_field {
  Err(error) => print $error.message
}
