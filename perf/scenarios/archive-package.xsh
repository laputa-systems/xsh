let root = fp"${args[0]}"
let pkgroot = fp"${root}/pkgroot"
let work_root = fs.tempdir()?
defer fs.close_root(work_root)?
let work = fs.root_path(work_root)?
let tarball = fp"${work}/package.tar.gz"
archive.tar_create(tarball, pkgroot, [p"."], compression: "gz", overwrite: true)?
let entries = archive.tar_list(tarball)?
let extracted = fp"${work}/extracted"
archive.tar_extract(tarball, extracted)?
let config = fp"${extracted}/etc/demo/config.toml".read_text()?
let payload = fp"${extracted}/usr/share/demo/payload.txt".read_bytes()?
print ${entries |> count()} config.count_lines() payload.sha256().hex()
