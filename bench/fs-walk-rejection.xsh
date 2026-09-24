# Compare the full entry record with the existing stat:false entry shape.
# Usage: xsh bench/fs-walk-rejection.xsh -- ROOT stat|no-stat
proc main(...argv: List[Str]) [fs, io, error] {
  let root = Path(argv[0])
  let use_stat = argv[1] == "stat"
  let rejected = fs.walk(root, gitignore: false, stat: use_stat)
    |> where .kind == "impossible"
    |> count()
  print f"${rejected}"
}
