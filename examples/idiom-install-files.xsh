let src_root = fs.tempdir()?
defer fs.close_root(src_root)?
let src_dir = fs.root_path(src_root)?
let dest_root = fs.tempdir()?
defer fs.close_root(dest_root)?
let dest_dir = fs.root_path(dest_root)?

fs.write(
  fp"${src_dir}/script.sh",
  """#!/bin/sh
echo hello
""",
)?

let src = fp"${src_dir}/script.sh"
let dest = fp"${dest_dir}/bin/script.sh"
fs.install(src, dest, 0o755, overwrite: true)?
print (dest.exists()?)
