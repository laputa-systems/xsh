type Builder = proc(root: Path) [fs, error] -> Result[Unit]
type Defaulted = proc(root: Path = p".") -> Result[Unit] # error: check.callable-type

proc fetch(root: Path) [fs, net, error] -> Result[Unit] {
  return Ok()
}

proc build_in(dir: Path) [fs, error] -> Result[Unit] {
  return Ok()
}

proc local_build(root: Path) [fs, error] -> Result[Unit] {
  return Ok()
}

proc select(dynamic: Proc, raw: Any, roots: List[Path]) [fs, error] -> Result[Unit] {
  let fetching: Builder = fetch # error: check.callable-mismatch
  let relabeled: Builder = build_in # error: check.callable-mismatch
  let unknown: Builder = dynamic # error: check.callable-mismatch
  let decoded = raw.require(Builder)? # error: check.callable-type
  let build: Builder = local_build
  build(@roots)? # error: check.callable-mismatch
  return build(roots[0])
}
