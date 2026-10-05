let compiler = env ({CC: "clang", BUILD_MODE: "release"}) {
  e"CC"?
}?
let directory = cd (p".") {
  fs.cwd()?
}?
let shown_directory = directory.display()
print $compiler $shown_directory
