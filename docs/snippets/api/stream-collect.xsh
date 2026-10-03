let files = fs.files(p".")?.collect()

for file in files {
  let shown_path = file.path.display()
  print $shown_path
}
