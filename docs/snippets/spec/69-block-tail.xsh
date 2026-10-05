proc first_line(file: Path) [fs, error] -> Result[Str] {
  file.read_lines()?[0]
}

# begin example
proc stamp_line(root: Path) [fs, error] -> Result[Str] {
  {
    let stamp = fp"{root}/stamp"
    stamp.write("staged\n")
    first_line(stamp)
  }
}

proc staged_line(root: Path) [fs, error] -> Result[Str] {
  tempdir scratch at fp"{root}/stage" {
    fp"{scratch}/stamp".write("staged\n")
    first_line(fp"{scratch}/stamp")?
  }
}

# end example

let root = fs.tempdir()?
defer root.close()
print ${stamp_line(root.host_path()?)?} ${staged_line(root.host_path()?)?}
