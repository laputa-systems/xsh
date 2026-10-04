const manifest_path = p"package.json"
# begin example
type Package = {name: Str, version: Str, files: List[Str]}

let package = json.read(manifest_path)?.require(Package)?
for file in package.files {
  print f"{package.name}-{package.version}: {file}"
}
# end example
