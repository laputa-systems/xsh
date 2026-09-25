proc main(...argv: List[Str]) [fs, io, error] {
  let root = Path(argv[0])
  let use_stat = argv[1] == "stat"
  let iterations = argv[2].parse_int()?
  var rejected = 0
  var index = 0
  while index < iterations {
    let count = fs.walk(root, gitignore: false, stat: use_stat)
      |> where .kind == "impossible"
      |> count()
    rejected += count
    index += 1
  }
  print f"${rejected}"
}
