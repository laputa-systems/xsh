type Package = {name: Str, ver: Str, optional: Bool}

let pkgs: List[Package] = [
  {name: "curl", ver: "8.0", optional: false},
  {name: "jq", ver: "1.7", optional: true},
  {name: "git", ver: "2.40", optional: false},
]

# basic transform
let names = [pkg.name for pkg in pkgs]
print names[0] names[1] names[2]

# with guard
let required = [pkg.name for pkg in pkgs if ! pkg.optional]
print required[0] required[1]

# record destructuring — bind fields directly
let labels = [f"${name}@${ver}" for {name, ver, ..} in pkgs]
print labels[0] labels[1] labels[2]

# destructuring + guard
let optional_names = [name for {name, optional} in pkgs if optional]
print optional_names[0]
