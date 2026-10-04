##! A build step imported by the callable-alias example.

## Runs make in `root` with `jobs` parallel jobs.
export proc build(root: Path, jobs = 1) [process, error] {
  run make -C $root f"-j{jobs}"
}
