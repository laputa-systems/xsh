proc test_path_absolute() [fs, error] {
  let absolute = path.absolute(p"docs")?
  test.ok(absolute.display().ends_with("/docs"))?
}

proc test_membership_operator_supports_strings_lists_bytes_and_paths() [error] {
  test.ok("lib" in "usr/lib/libz.so")?
  test.ok("libz.so" in ["libz.so", "libc.so"])?
  test.ok(b"TODO" in b"one TODO two")?
  test.ok(p"usr/lib" in p"usr/lib/libz.so")?
  test.eq(p"bin" in p"usr/lib/libz.so", false)?
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

proc test_path_edge_cases_and_standard_record_schema(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "path-edge")?
  let spaced = fp"${root}/space name"

  let lined = fp"""${root}/line
name"""

  let dashed = fp"${root}/-leading"
  spaced.write("a")?
  lined.write("b")?
  dashed.write("c")?
  run test -f $spaced
  run test -f $lined
  run test -f $dashed
  let meta = spaced.metadata()?
  test.eq(path_entry_name(meta), "space name")?
  let raw_path = Path.parse_bytes(b"bad\xffname")?
  test.ok("bad" in raw_path.display())?

  let raw = test.run_script(
    ctx,
    r"""
let raw_path = Path.parse_bytes(b"bad\xffname")?
run printf "%s" (raw_path) ?
""",
  )?

  test.ok(raw.success, raw.stderr)?
  test.eq(raw.stdout_bytes, b"bad\xffname")?
}

pure path_entry_name(entry: FsEntry) -> Str {
  return entry.name
}

proc test_absolute_glob_traverses_symlinked_literal_components(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "absolute-glob-symlink")?
  let real = fp"${root}/real"
  let link = fp"${root}/link"
  real.mkdir()
  fp"${real}/hit.txt".write("ok")?
  fs.symlink(real, link)?

  let output = test.run_script(
    ctx,
    f"""
let files = g"${link.display()}/*.txt" |> map { |entry_path| entry_path.name }
print \${files[0]}
""",
  )?

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """hit.txt
""",
  )?
}
