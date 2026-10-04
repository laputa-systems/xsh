type Package = {name: Str, sources: List[Path]}

const packages: List[Package] = [
  {
    name: "core",
    sources: [
      p"main.xsh",
      p"README.md",
    ],
  },
  {
    name: "tools",
    sources: [
      p"check.xsh",
    ],
  },
]
let sources = [
  source
  for package in packages
  for source in package.sources
  if source.ext() == "xsh"
]
let by_package = {
  package.name: source
  for package in packages
  for source in package.sources
  if source.ext() == "xsh"
}
print sources.len()
print by_package.len()

const empty_counts: Map[Int] = {}
let counts = empty_counts.set("beta", 2).set("alpha", 1)

for {key, value: count} in counts {
  print f"{key}={count}"
}

let doubled = {key: value * 2 for {key, value} in counts}
print ${doubled.get("alpha")?}
