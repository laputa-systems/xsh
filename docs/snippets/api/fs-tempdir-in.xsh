let parent = fs.tempdir()?
defer parent.close()
let root = fs.tempdir_in(parent.host_path()?)?
defer root.close()
root.write(p"chunk", "data")?
root.chmod(p"chunk", 0o600)?
