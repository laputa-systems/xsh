## Copy a build artifact.
cli main(source: Path, dest: Path, jobs: UInt = 4, verbose = false) {
  if verbose {
    print f"copying {source} to {dest} with {jobs} jobs"
  }

  fs.copy(source, dest)?
}
