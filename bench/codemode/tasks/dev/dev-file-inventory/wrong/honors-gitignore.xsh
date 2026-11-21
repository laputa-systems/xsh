type File = {path: Str, kind: Str, size: Int}

type Link = {path: Str, kind: Str, target: Str}

type Row = {key: Str, line: Str}

let root = p"/work/data"
var rows: List[Row] = []
var files = 0
var symlinks = 0
var total = 0

for entry in fs.walk(root, hidden: true)? {
  let relative = entry.path.relative_to(root)
  let key = relative.display()
  if entry.kind == "file" {
    files += 1
    total += entry.size
    rows += [Row(key: key, line: json.encode(File(path: key, kind: "file", size: entry.size))?)]
  } else if entry.kind == "symlink" {
    symlinks += 1
    let target = entry.path.readlink()?.display()
    rows += [Row(key: key, line: json.encode(Link(path: key, kind: "symlink", target: target))?)]
  }
}

for row in rows |> sort-by .key {
  print $row.line
}

p"/work/summary.json".write(json.encode({files: files, symlinks: symlinks, bytes: total})?)
