cli main(root: Path) {
  for entry in fs.walk(root, gitignore: false, hidden: true, stat: true) |> sort-by .path {
    let relative = entry.path.strip_prefix(root)?.display()
    continue when relative == "."
    let kind = if entry.kind == "symlink" { "link" } else { entry.kind }
    let size = if kind == "file" { f"{entry.size}" } else { "-" }
    print f"{relative}\t{kind}\t{size}"
  }
}
