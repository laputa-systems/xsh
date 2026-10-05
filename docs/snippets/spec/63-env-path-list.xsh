const root = p"/opt/stage"

# begin example
let search = [fp"{root}/usr/bin", fp"{root}/bin", @env.PathList.PATH ?? []]
env ({PATH: search}) {
  run make
}
# end example
