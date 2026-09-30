let flags = ["-O2", "-g"]
let sources = ["main.xsh", "helpers.xsh"]
let argv = ["cc", @flags, @sources, "-o", "app"]
let groups = [["head"], @[flags, sources]]
print argv.join(" ")
print groups.len()
