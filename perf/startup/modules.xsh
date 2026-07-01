let payload = json.decode("{\"name\":\"demo\",\"count\":7,\"tags\":[\"fs\",\"json\",\"regex\"]}")?
let matcher = regex.compile("demo|prod")?
let root = /tmp/xsh/startup/module.xsh
let shown = root.parent().name()

print f"${matcher.matches(payload.name)} ${payload.tags.len()} ${shown}"
