# begin example
type Entry = {rel: RelPath, mode: Int}

proc install(root: FsRoot, entry: Entry, data: Str) [fs, error] {
  # `parent()` of a RelPath is a RelPath; the parent of one name is `.`.
  root.mkdir(entry.rel.parent(), parents: true)
  root.write(entry.rel, data)
  root.chmod(entry.rel, entry.mode)
}

proc stage(root: FsRoot, tree: Path, file: Path) [fs, error] {
  # A literal is checked where it is written.
  let config: RelPath = "etc/app/config.toml"
  install(root, {rel: config, mode: 0o644}, "debug = false\n")

  # Interpolated RelPaths set off by `/` join into a RelPath.
  let backup: RelPath = fp"{config.parent()}/backup/{config}"
  install(root, {rel: backup, mode: 0o600}, "debug = false\n")

  # What is left of a path beneath a prefix is a RelPath, or a failure.
  let rel = file.strip_prefix(tree)?
  install(root, {rel, mode: 0o644}, file.read_text()?)

  # Any other path is validated once, at an explicit boundary.
  let copy = fp"doc/{file.name()}".require(RelPath)?
  install(root, {rel: copy, mode: 0o644}, file.read_text()?)
}

# end example

let scratch = fs.tempdir()?
defer scratch.close()
let tree = fp"{scratch.host_path()?}/tree"
fp"{tree}/share/doc".mkdir()
fp"{tree}/share/doc/README".write("read me\n")
scratch.mkdir("dest")
let dest = scratch.open_root("dest")?
defer dest.close()
stage(dest, tree, fp"{tree}/share/doc/README")
print dest.read_text("share/doc/README")?.trim() (dest.exists("etc/app/backup/etc/app/config.toml")?)
