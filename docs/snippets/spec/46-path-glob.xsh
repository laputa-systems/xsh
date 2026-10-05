proc sources(package: Path, suffix: Str) [fs, error] -> Result[List[Path]] {
  # begin example
  let manifests = package.glob("*.toml")? # directly inside package
  let tests = package.rglob(f"*_test.{suffix}")? # at any depth
  let nested = package.glob("src/**/*.xsh")? # the same pattern as g"..."
  # end example
  [@manifests, @tests, @nested]
}
