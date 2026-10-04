#!/usr/bin/env -S xsh --
# Show the largest files under ROOT.
cli main(root: Path, limit: UInt = 5, hidden = false) {
  let largest = fs.files(root, stat: true, hidden:)?
    |> sort-by(desc: true) .size
    |> take(limit)

  for entry in largest {
    print f"{entry.size:>12} {entry.path}"
  }
}
