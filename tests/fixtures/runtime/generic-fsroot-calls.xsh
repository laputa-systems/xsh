proc listed(marker, root: FsRoot) [fs, error] { let _ = marker; root.children(ordered: true, path: p".")? }
proc stored(marker, root: FsRoot, location: RelPath, text: Str) [fs, error] { let _ = marker; root.write(location, text)? }
proc loaded(marker, root: FsRoot, location: RelPath) [fs, error] { let _ = marker; root.read_text(location)? }
proc exercise() [fs, error] {
  tempdir site {
    with root = fs.open_root(site)? {
      stored(7, root, p"value", "kept")?
      let first = listed(7, root)?
      let second = listed("independent", root)?
      assert first.children == second.children
      assert first.children.len() == 1
      assert loaded(7, root, p"value")? == "kept"
      assert loaded("independent", root, p"value")? == "kept"
    }
  }
}
exercise()?
print "rooted calls preserved"
