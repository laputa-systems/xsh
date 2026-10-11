type Package = {name: Str, version: Int}

cli main(root: Path) {
  let rows = json.read(fp"{root}/index.json")?.require(List[Package])?
  let replacement = json.read(fp"{root}/replacement.json")?.require(Package)?
  let addition = json.read(fp"{root}/addition.json")?.require(Package)?
  let updated = collect {
    for row in rows {
      yield if row.name == replacement.name { replacement } else { row }
    }
    yield addition
  }
  let sorted = updated |> sort-by .name
  print json.encode(sorted)?
}
