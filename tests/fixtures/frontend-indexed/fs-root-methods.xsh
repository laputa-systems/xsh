proc root_methods() [fs, error] -> Result[Str] {
  let root = fs.tempdir()?
  root.mkdir(p"child")?
  let child = root.open_root(p"child")?
  defer child.close()?
  let erased: Any = root
  defer erased.require(FsRoot)?.close()?
  child.write(p"data", "payload")?
  let observed = child.read_result(p"data", max_bytes: 1)?
  if ! observed.truncated { error.fail("bounded read must truncate")? }
  child.read_text(p"data")?
}
