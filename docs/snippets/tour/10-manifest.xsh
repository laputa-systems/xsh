enum Kind { File, Binary, Symlink }

type Entry = {path: Path, kind: Kind, executable: Bool = false}

const manifest: List[Entry] = [
  Entry(p"usr/bin/xsh", .Binary, executable: true),
  Entry(p"etc/xsh.conf", .File),
  Entry(p"usr/bin/sh", .Symlink),
]

let binaries = [f"{e.path}" for e in manifest if e.kind == .Binary and e.executable]
print f"binaries: {binaries.join(", ")}"
