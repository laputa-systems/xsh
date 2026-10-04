const package = {name: "core"}
const source = p"build/core"
const dest = p"/opt/core"
# begin example
ctx f"installing {package.name}" {
  fs.copy(source, dest)?
}
# end example
