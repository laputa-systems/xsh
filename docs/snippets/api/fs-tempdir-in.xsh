let parent = fs.tempdir()?
let root = fs.tempdir_in(parent.host_path()?)?
root.write(p"chunk", "data")?
root.chmod(p"chunk", 0o600)?
