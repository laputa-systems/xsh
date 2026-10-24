use toolchain

const root = p"."
# begin example
let build = toolchain.build
build(root, jobs: 4)
let identity = toolchain.identity
let number: Int = identity(42)
let text: Str = identity("demo")
assert number == 42
assert text == "demo"
# end example
