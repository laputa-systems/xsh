type Copy = {rel: Str, path: Path, size: Int, digest: Str}

type Duplicates = {kept: Str, moved: List[Str]}

let root = p"/work/photos"
let dest = p"/work/duplicates"

let copies = fs.walk(root)?
  |> where { |entry| entry.kind == "file" and entry.size > 0 }
  |> map { |entry|
    Copy(
      rel: entry.path.relative_to(root).display(),
      path: entry.path,
      size: entry.size,
      digest: entry.path.read_bytes()?.sha256().hex(),
    )
  }
  |> sort-by .rel

var groups: List[Duplicates] = []
var moved_bytes = 0

for same in copies |> group-by .digest {
  if same.items.len() < 2 {
    continue
  }

  let extra = same.items |> drop(1)
  for copy in extra {
    let target = fp"{dest}/{copy.rel}"
    target.parent().mkdir(parents: true)?
    copy.path.rename(to: target)?
    moved_bytes += copy.size
  }

  groups += [Duplicates(kept: same.items[0].rel, moved: [c.rel for c in extra])]
}

print (json.encode({groups: groups |> sort-by .kept, moved_bytes})?)
