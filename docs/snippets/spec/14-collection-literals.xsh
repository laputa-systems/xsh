const flags = ["-O2", "-Wall"]
const output = "app"
const root = p"src"
const ext = "c"
const defaults = {jobs: 1, verbose: false}
const overrides = {jobs: 8}
# begin example
let argv = ["cc", @flags, "-o", output]
let entry = {name: "core", path: root, "content-type": "text/plain"}
let counts = {[ext]: 1}
let merged = {...defaults, ...overrides}
# end example
