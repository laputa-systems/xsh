let compiler = env ({CC: "clang", BUILD_MODE: "release"}) {
  env.get("CC")?
}?
let directory = cd (p".") {
  fs.cwd()?
}?
let shown_directory = directory.display()
print $compiler $shown_directory
