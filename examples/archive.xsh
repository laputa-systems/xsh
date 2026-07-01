let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let src = fp"${root}/src"
let bin = fp"${src}/usr/bin"
bin.mkdir()
let tool = fp"${bin}/tool"

tool.write("""demo
""")

tool.chmod(0o755)
let tarball = fp"${root}/pkg.tar.gz"
archive.tar_create(tarball, src, [p"."])
let entries = archive.tar_list(tarball)?
let dest = fp"${root}/dest"
archive.tar_extract(tarball, dest)
let installed = fp"${dest}/usr/bin/tool"
print entries.len() installed.read_text()?.trim() ${installed.metadata()?.mode % 512 == 493}
let cpio = fp"${root}/pkg.cpio"
archive.cpio_create(cpio, src, [p"."])
let cpio_entries = archive.cpio_list(cpio)?
let cpio_dest = fp"${root}/cpio"
archive.cpio_extract(cpio, cpio_dest)
let gz = fp"${root}/tool.gz"
archive.compress(tool, gz, format: "gzip")
let decoded = archive.decompress_bytes(gz)?.utf8()?.trim()
let cpio_installed = fp"${cpio_dest}/usr/bin/tool"
print cpio_entries.len() cpio_installed.read_text()?.trim() $decoded
