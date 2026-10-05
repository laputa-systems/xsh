const package = {name: "core"}
const source = p"build/core"
const dest = /opt/core
# begin example
ctx f"installing {package.name}" {
  fs.copy(source, dest)
}
# end example
