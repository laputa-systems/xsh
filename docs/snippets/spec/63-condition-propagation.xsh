proc stale(cache: Path, src: Path) -> Result[Bool] {
  # begin example
  if ! cache.exists() or ! src.exists() {
    eprint "nothing to compare"
    return Ok(true)
  }

  # end example

  Ok(cache.read_text()? != src.read_text()?)
}

let root = fs.tempdir()?
defer root.close()?
let src = fp"{root.host_path()?}/src"
src.write("v1\n")
print f"{stale(fp"{root.host_path()?}/cache", src)?}"
