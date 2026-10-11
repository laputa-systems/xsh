cli main(root: Path) {
  let tree = fp"{root}/tree"
  for entry in fs.walk(tree, gitignore: false, hidden: true) |> sort-by .path {
    let relative = entry.path.strip_prefix(tree)?.display()
    continue when relative == "."
    let kind = if entry.kind == "symlink" { "link" } else { entry.kind }
    print f"{relative}\t{kind}"
  }
}
