let config = {root: "src", build: {jobs: 3, target: "native"}}
let {root, build: {jobs, target: target_name, ..}, ..} = config
print $root $jobs $target_name
