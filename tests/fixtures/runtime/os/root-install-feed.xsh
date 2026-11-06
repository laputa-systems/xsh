let site = fp"{args[0]}"
let root = fs.open_root(site)?
let source: RelPath = "feed"
let dest: RelPath = "out"
fs.root_install_file(root, source, root, dest, 0o600, true, args[1] == "replace")?
