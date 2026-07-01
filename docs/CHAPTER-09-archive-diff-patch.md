# Chapter 9: Archives, Diffs, And Patches

Packaging and repair workflows combine files, permissions, archive formats,
diffs, and patches. They are risky when the only feedback is command text.

By the end of this chapter, you will have built a small staged package, created
archives from it, inspected extracted files, generated a diff, applied a rooted
patch, and checked structured counts before trusting the result.

## Package A Staged Tree

The `archive` module can create, list, extract, compress, and decompress common
archive formats.

```xsh
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
```

The script builds a small `usr/bin` tree, writes an executable file, creates
tar and cpio archives, extracts them, checks permissions, and round-trips
gzip-compressed bytes.

Why XSH shines here: archive entries, paths, modes, and decoded bytes stay
inspectable instead of being hidden inside command output.

Compared with bash and CLI tools: `tar`, `cpio`, `gzip`, `diff`, and `patch`
are still the right tools for many terminal jobs. XSH is useful when packaging
is part of a larger policy script that must verify counts, modes, paths, and
final contents before continuing.

Common trap: after extracting an archive, verify the paths and metadata your
workflow depends on. Creating the archive successfully is not the same as
proving the packaged tree has the shape you intended.

Do not extract archives into an ambient working directory when the script can
use a known destination path and verify what appeared there.

## Apply A Controlled Text Change

Diff and patch operations are useful when a workflow needs to show or apply a
specific text change.

```xsh
let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let original = fp"${root}/config.old"
let modified = fp"${root}/config.new"

original.write("""name=demo
enabled=false
""")

modified.write("""name=demo
enabled=true
""")

let d = diff.unified(original, modified, context: 1)?
let apply_root = fp"${root}/apply"
fs.mkdir(apply_root)

fp"${apply_root}/config".write("""name=demo
enabled=false
""")

let patch_text = """--- config
+++ config
@@ -1,2 +1,2 @@
 name=demo
-enabled=false
+enabled=true
"""

let applied = patch.apply(apply_root, patch_text)?
let config = fp"${apply_root}/config".read_text()?.trim()
print $d.files $d.hunks $applied.files $applied.hunks ${"enabled=true" in config}
```

The script creates old and new config files, generates a unified diff, applies
a patch under a rooted directory, reads the final file, and checks that the
expected value is present.

Why XSH shines here: patch application is rooted and returns structured counts,
so scripts can verify what changed before continuing.

Do not apply a patch and assume success means the intended file changed. Check
the returned counts and read the final file when the content matters.

## What You Know Now

Archives, diffs, and patches become safer when the script treats paths, modes,
counts, and final file contents as values. The next chapter shows how named
types and procs keep those values organized as scripts grow.
