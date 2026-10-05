const package = {name: "core"}
const src = p"build/core"
const dest = /opt/core
# begin example
ctx f"installing {package.name}" {
  src.copy(to: dest)
}
# end example
