enum Kind { File, Binary, Symlink }

# begin example
type Entry = {path: Path, kind: Kind, mode: Int = 0o644}

let tool = Entry(p"usr/bin/xsh", Binary, mode: 0o755)
let config = Entry(p"etc/xsh.conf", File)
let link = Entry(p"usr/bin/sh", Symlink)
# end example
