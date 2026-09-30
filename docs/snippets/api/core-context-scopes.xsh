let compiler = env ({CC: "clang", BUILD_MODE: "release"}) { env.get("CC")? }?
let directory = cd (p".") { fs.cwd()? }?
print $compiler ${directory.display()}
