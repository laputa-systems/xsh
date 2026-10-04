# begin example
enum Kind { File, Binary, Symlink, Tree(Int) }

error ProofError = Missing(file: Path) | Failed(kind: Str, message: Str)

type Entry = {path: Path, kind: Kind}

pure require_tool(entries: List[Entry], file: Path) -> Result[Entry, ProofError] {
  for entry in entries {
    return entry when entry.path == file and entry.kind == .Binary
  }

  Err(.Missing(file:))
}

const entries: List[Entry] = [Entry(p"usr/bin/xsh", .Binary), {path: p"usr/share", kind: .Tree(3)}]
const linked: Kind? = .Symlink
# end example
let _ = require_tool(entries, p"usr/bin/xsh")
let _ = linked
