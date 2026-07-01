type Metadata = {owner: Str, critical: Bool}

type Package = {name: Str, version: Str, files: List[Path], metadata: Metadata}

pure package_score(pkg: Package) -> Int {
  let base = pkg.name.count_chars() + pkg.version.count_chars()

  let file_score = pkg.files
    |> map .name.count_chars()
    |> sum

  if pkg.metadata.critical {
    return base + file_score + 100
  }

  return base + file_score
}

let packages = [
  {
    name: "core",
    version: "1.0.0",
    files: [p"src/main.xsh", p"src/lib.xsh"],
    metadata: {owner: "runtime", critical: true},
  },
  {
    name: "tools",
    version: "1.2.3",
    files: [p"tools/check.xsh", p"tools/fmt.xsh"],
    metadata: {owner: "cli", critical: false},
  },
]

print ${packages
  |> map package_score(.)
  |> sum}
