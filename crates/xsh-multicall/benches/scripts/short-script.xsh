type Package = {name: Str, enabled: Bool}

let packages: List[Package] = [
  {
    name: "alpha",
    enabled: true,
  },
  {
    name: "beta",
    enabled: false,
  },
  {
    name: "gamma",
    enabled: true,
  },
]

let names = packages
  |> where .enabled
  |> map .name
  |> sort

print names.join(",")
