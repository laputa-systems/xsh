type PackageName = Str

type Package = {name: PackageName, root: Path, files: List[Path]}

let demo_pkg: Package = {name: "demo", root: p"src", files: [p"src/main.c"]}

proc describe(pkg: Package, prefix: Str = "pkg", ...labels: List[Str]) {
  print $prefix $pkg.name $pkg.root.name

  for label in labels {
    print $label
  }
}

describe(demo_pkg)
describe(demo_pkg, "named", "extra")
