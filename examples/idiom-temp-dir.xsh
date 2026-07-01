let root = fs.tempdir()?
fs.root_write(root, p"data.txt", "hello")?
let content = fs.root_read_text(root, p"data.txt")?
print $content
fs.close_root(root)?
