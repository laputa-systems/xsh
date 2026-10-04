# begin example
type Mount = {source: Path, target: Path, options: List[Str] = []}

let boot = Mount(p"/dev/vda1", p"/boot")  # error: check.record-constructor
let root = Mount(source: p"/dev/vda2", target: p"/")
# end example
