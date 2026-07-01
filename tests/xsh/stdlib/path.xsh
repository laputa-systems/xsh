proc test_path_absolute() [fs, error] {
  let absolute = path.absolute(p"docs")?
  test.ok(absolute.display().ends_with("/docs"))?
}

proc test_path_methods(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "path-methods")?
  let file = fp"${root}/dir/file.txt"
  file.parent().mkdir()?
  file.write("hello")?
  test.eq(file.read_text()?, "hello")?
  file.write_atomic(b"bytes")?
  test.eq(file.read_bytes()?, b"bytes")?
  test.eq(file.name(), "file.txt")?
  test.eq(file.ext(), "txt")?
  test.eq(file.with_ext("log").name(), "file.log")?
  test.eq(fp"${root}/dir/../dir/file.txt".normalize(), file)?
  test.eq(file.strip_prefix(root)?.display(), "dir/file.txt")?
  test.eq(file.relative_to(root).display(), "dir/file.txt")?
  test.ok(file.resolve()?.display().ends_with("file.txt"))?
  test.ok(file.exists()?)?
  test.ok(! file.executable()?)?
  test.ok(file.du()? >= 0)?
  test.eq(file.metadata()?.kind, "file")?
  file.chmod(0o600)?
  file.truncate(2)?
  test.eq(file.read_text()?, "by")?
  let copied = fp"${root}/copy.txt"
  file.copy(copied)?
  test.eq(copied.read_text()?, "by")?
  let renamed = fp"${root}/renamed.txt"
  copied.rename(renamed)?
  test.ok(renamed.exists()?)?
  let link = fp"${root}/link.txt"
  file.hardlink(link)?
  test.eq(link.read_text()?, "by")?
  let symlink = fp"${root}/symlink.txt"
  fs.symlink(file, symlink)?
  test.eq(symlink.readlink()?.display(), file.display())?
  link.unlink()?
  test.ok(! link.exists()?)?
  renamed.remove()?
  let empty_dir = fp"${root}/empty"
  empty_dir.mkdir()?
  empty_dir.remove_dir()?
  let touched = fp"${root}/touched"
  touched.touch()?
  touched.touch_from(file)?
  touched.remove(missing_ok: true)?
  let relative_text = "relative/path"
  let parsed = fp"${relative_text}"
  test.eq(parsed.display(), "relative/path")?
  test.eq(Path.parse_bytes(b"byte/path")?.display(), "byte/path")?
}
