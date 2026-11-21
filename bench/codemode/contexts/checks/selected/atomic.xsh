tempdir dir {
  let file = fp"{dir}/conf"
  let witness = fp"{dir}/witness"
  file.write("old\n")
  file.hardlink(at: witness)
  file.write_atomic("new\n")
  let leftovers = fs.children(dir)? |> count()
  print f"{file.read_text()?.trim()} {witness.read_text()?.trim()} {leftovers}"
}
