type Package = {name: Str, version: Str, tags: List[Str]}

let sample = "{\"name\":\"demo\",\"version\":\"1.0\",\"tags\":[\"alpha\",\"beta\"]}"
let package = json.decode(sample)?.require(Package)?
print "all fields present"
print f"${package.name} v${package.version}"
