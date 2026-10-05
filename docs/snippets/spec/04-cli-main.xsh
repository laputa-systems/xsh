## Copy a build artifact.
cli main(src: Path, dest: Path, jobs: UInt = 4, verbose = false) {
  if verbose {
    print f"copying {src} to {dest} with {jobs} jobs"
  }

  src.copy(dest)
}
