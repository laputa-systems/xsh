proc first_line(file: Path) [fs, error] -> Result[Str] {
  file.read_lines()?[0]
}

# begin example
proc staged_line(root: Path) [fs, error] -> Result[Str] {
  tempdir scratch at fp"{root}/stage" { # error: check.type-mismatch
    fp"{scratch}/stamp".write("staged\n")
    first_line(fp"{scratch}/stamp") # error: check.type-mismatch
  }
}
# end example
