let files = fs.files(p".")?.collect()
for file in files { print ${file.path.display()} }
