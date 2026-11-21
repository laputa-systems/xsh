tempdir dir {
  let file = fp"{dir}/sub/notes.txt"
  file.parent().mkdir()
  file.write("one\ntwo\n")
  file.write_atomic("one\ntwo\nthree\n")
  let link = fp"{dir}/link"
  link.symlink(to: p"sub/notes.txt")
  print f"{file.name()} {file.parent().name()} {file.exists()?} {file.parent().is_dir()?}"
  print f"{file.read_text()?.lines().len()} {file.read_text()?.byte_len()} {link.readlink()?.display()}"
  print (file.relative_to(dir).display())
  file.remove()
  print f"{file.exists()?}"
}
