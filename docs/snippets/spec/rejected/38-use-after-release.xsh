let root = fs.tempdir()?
root.close()?
assert root.exists(p".")? # error: check.use-after-release
