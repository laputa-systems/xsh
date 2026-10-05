proc disk_used_kb(root: Path) -> Result[Int] {
  let out = run.text du -sk $root ?
  out.fields()[0].parse_int()?
}

pure percent(part: Int, whole: Int) -> Int {
  if whole == 0 { 0 } else { part * 100 / whole }
}

# This script may touch files and run processes, and nothing else.
proc main() [fs, process, error] {
  let scratch = fs.tempdir()?
  defer scratch.close()?
  let dir = scratch.host_path()?
  fp"{dir}/data".write("hello\n")

  print f"measured: {disk_used_kb(dir)? > 0}; 3 of 4 is {percent(3, 4)}%"
}
