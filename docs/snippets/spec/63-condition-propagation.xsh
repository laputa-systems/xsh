proc stale(cache: Path, source: Path) -> Result[Bool] {
  # begin example
  if ! cache.exists() or ! source.exists() {
    eprint "nothing to compare"
    return Ok(true)
  }

  # end example

  Ok(cache.read_text()? != source.read_text()?)
}

let root = fs.tempdir()?
defer root.close()?
let source = fp"{root.host_path()?}/source"
source.write("v1\n")
print f"{stale(fp"{root.host_path()?}/cache", source)?}"
