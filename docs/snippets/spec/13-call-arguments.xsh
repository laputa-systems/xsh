proc build(root: Path, jobs = 1, verbose = false) [io] {
  if verbose {
    print f"building {root} with {jobs} jobs"
  }
}

proc main(...args: List[Str]) [io] {
  print args.join(" ")
}

proc rebuild(root: Path, jobs: Int, args: List[Str]) [error, io] {
  # begin example
  let opts = {jobs: 4, verbose: true}
  build(root, ...opts)
  build(root, jobs:, verbose: false)
  main(@args)
  # end example
}
