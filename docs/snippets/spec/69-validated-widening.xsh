# begin example
pure names(files: List[Path]) -> List[Str] {
  [file.name() for file in files]
}

proc manifest(tree: Path, files: List[Path]) [error] -> Result[List[RelPath]] {
  Ok(files |> map .strip_prefix(tree)? |> sort-by .display())
}

proc listing(tree: Path, files: List[Path]) [error] -> Result[List[Str]] {
  # A list of RelPaths is a list of paths wherever one is expected.
  let rels = manifest(tree, files)?
  Ok(names(rels))
}

# end example

print listing(p"/srv/tree", [p"/srv/tree/share/doc/README", p"/srv/tree/bin/sh"])?.join(" ")
