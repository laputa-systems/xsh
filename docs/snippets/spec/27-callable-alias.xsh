use toolchain

const root = p"."
# begin example
let build = toolchain.build
build(root, jobs: 4)
# end example
