enum Kind { File, Binary, Tree(Int) }
error ProofError = Failed(kind: Str, message: Str)
type Entry = {path: Path, kind: Kind}
pure check(entry: Entry) -> Result[Entry, ProofError] {
  if entry.kind == .File { return Err(.Failed("proof-kind", f"{entry.path} is a plain file that should have been a binary")) }
  entry
}
const entries: List[Entry] = [Entry(p"usr/lib/libevdev.so.2.3.0", .Binary), Entry(p"usr/share/doc", .Tree(2)), Entry(p"etc/conf", .File)]
let picked: Kind = match entries[0].kind { File => .Binary
  _ => .File }
let found = [f"{e.path}" for e in entries if check(e) is Ok(_)]
print f"{found.join(" ")}"
