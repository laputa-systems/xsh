const manifest = {name: "core", version: "1.0", build: {jobs: 4, opt: 2}, license: "MIT"}
const entries = [{path: p"a", size: 1}, {path: p"b", size: 2}]
# begin example
let {name, version: v, build: {jobs, ..}, ..} = manifest

for {path: file, size} in entries {
  print f"{file}: {size}"
}
# end example
