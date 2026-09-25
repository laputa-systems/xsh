proc main(...argv: List[Str]) [fs, io, error] {
  let root = Path(argv[0])
  let files = fs.walk(root, gitignore: false, stat: true)
    |> where .kind == "file"
    |> count()
  print f"${files}"
}
