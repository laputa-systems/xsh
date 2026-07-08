#!/usr/bin/env -S xsh --
# Demonstrate the archive dependency trim and a tar workflow that covers the
# lower-level writer path XSH now uses.
# Run from the repository root:
#   xsh tools/archive-fat-trim-demo.xsh
let tokio_probe = run.capture --text cargo tree -i tokio ?
let async_fs_probe = run.capture --text cargo tree -i async-fs ?
let blocking_probe = run.capture --text cargo tree -i blocking ?
let async_channel_probe = run.capture --text cargo tree -i async-channel ?

print "dependency trim"
print f"  tokio absent: ${! tokio_probe.status.exited_with(0)}"
print f"  async-fs absent: ${! async_fs_probe.status.exited_with(0)}"
print f"  blocking absent: ${! blocking_probe.status.exited_with(0)}"
print f"  async-channel absent: ${! async_channel_probe.status.exited_with(0)}"

let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let src = fp"${root}/src"
let bin = fp"${src}/usr/bin"
bin.mkdir()
let tool = fp"${bin}/xsh-demo"

tool.write("""#!/bin/sh
echo archive demo
""")?

tool.chmod(0o755)

let long_rel_text = "share/xsh/archive-demo/really-long-directory-name-for-tar-writer-paths/another-long-directory-name-for-gnu-and-pax-coverage/final/demo-payload.txt"
let long_rel = fp"${long_rel_text}"
let long_file = fp"${src}/${long_rel_text}"
long_file.parent().mkdir()

long_file.write("""long path payload
""")?

fs.symlink(p"usr/bin/xsh-demo", fp"${src}/xsh-demo-link")?
fs.symlink(long_rel, fp"${src}/long-target-link")?

let tarball = fp"${root}/payload.tar.gz"
archive.tar_create(tarball, src, [p"."], "auto", true)?
let entries = archive.tar_list(tarball)?
let dest = fp"${root}/dest"
archive.tar_extract(tarball, dest)?

let selected = fp"${root}/selected"
archive.tar_extract(tarball, selected, 0, "auto", false, [long_rel])?

let installed = fp"${dest}/usr/bin/xsh-demo"
let link = fp"${dest}/xsh-demo-link"
let long_link = fp"${dest}/long-target-link"
let selected_long = fp"${selected}/${long_rel_text}"
let entry_paths = entries |> map .path.display()
let entry_links = entries |> map .link_name

print "tar behavior"
print f"  gzip archive entries: ${entries.len()}"
print f"  executable mode preserved: ${installed.metadata()?.mode % 512 == 493}"
print f"  symlink target preserved: ${link.readlink()?.display() == "usr/bin/xsh-demo"}"
print f"  long path listed: ${long_rel_text in entry_paths}"
print f"  long path extracted: ${selected_long.read_text()?.trim() == "long path payload"}"
print f"  long symlink target listed: ${long_rel_text in entry_links}"
print f"  long symlink target extracted: ${long_link.readlink()?.display() == long_rel_text}"
print f"  member filter excluded unrelated file: ${! fp"${selected}/usr/bin/xsh-demo".exists()?}"
