test test_path_absolute [fs, error] {
  let absolute = path.absolute(p"docs")?
  test.ok(absolute.display().ends_with("/docs"))?
}

test test_membership_operator_supports_strings_lists_bytes_and_paths [error] {
  test.ok("lib" in "usr/lib/libz.so")?
  test.ok("libz.so" in ["libz.so", "libc.so"])?
  test.ok(b"TODO" in b"one TODO two")?
  test.ok(p"usr/lib" in p"usr/lib/libz.so")?
  test.eq(p"bin" in p"usr/lib/libz.so", false)?
}

test test_path_methods [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "path-methods")?
  let file = fp"${root}/dir/file.txt"
  file.parent().mkdir()?
  file.write("hello")?
  test.eq(file.read_text()?, "hello")?
  file.write_atomic(b"bytes")?
  test.eq(file.read_bytes()?, b"bytes")?
  test.eq(file.name(), "file.txt")?
  test.eq(file.ext(), "txt")?
  test.eq(file.basename(), "file.txt")?
  test.eq(file.dirname().display(), fp"${root}/dir".display())?
  test.eq(p"/".basename(), "/")?
  test.eq(p".".basename(), ".")?
  test.eq(p"a/.".dirname().display(), "a")?
  test.eq(p"a/".basename(), "a")?
  test.eq(p".profile".ext_or("none"), "none")?
  test.eq(p"file.".ext_or("none"), "")?
  test.eq(p"plain".ext_or("none"), "none")?
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

test test_path_edge_cases_and_standard_record_schema [fs, process, error] { |ctx|
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

test test_absolute_glob_traverses_symlinked_literal_components [fs, error] { |ctx|
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

proc test_path_interpolation_retains_native_bytes_and_text_boundaries(ctx: TestContext) [error] {
  let raw = Path.parse_bytes(b"raw\xff name")?
  test.ok(fp"prefix/${raw}/../end" == Path.parse_bytes(b"prefix/raw\xff name/../end")?)?
  test.ok(fp"${p"left"}/${"right"}/${7}/${false}" == p"left/right/7/false")?
  test.ok(fp"${raw:>12}" == Path.parse_bytes(b"   raw\xff name")?)?
  test.eq(f"${raw}", raw.display())?
  test.ok(fp"${raw.display()}" != raw)?
  let output = test.run_script(ctx, r"""
let raw = Path.parse_bytes(b"raw\xff name/'\"")?
run printf "%s" "--target=$raw" ?
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout_bytes, b"--target=raw\xff name/'\"")?
}

proc test_path_interpolation_rejects_nul_and_keeps_effect_order(ctx: TestContext) [error] {
  let failed = test.run_script(ctx, r"""
let text = "\0"
let invalid = fp"prefix/${text}"
print "unexpected"
""")?
  test.ok(!failed.success)?
  test.ok("NUL" in failed.stderr)?
  let argv_failed = test.run_script(ctx, r"""
let text = "\0"
run printf "%s" "value=$text" ?
""")?
  test.ok(!argv_failed.success)?
  test.ok("NUL" in argv_failed.stderr)?
  test.eq(argv_failed.stdout_bytes, b"")?
  let bytes_failed = test.run_script(ctx, r"""
let invalid = fp"${b"raw"}"
""")?
  test.ok(!bytes_failed.success)?
  test.ok("display" in bytes_failed.stderr)?
  let ordered = test.run_script(ctx, r"""
proc piece(label: Str) [io] -> Path { print --flush $label; return Path(label) }
let result = fp"${piece("first")}/${piece("second")}/../last"
print --flush $result
run printf "%s\n" "${piece("third")}/${piece("fourth")}" ?
""")?
  test.ok(ordered.success, ordered.stderr)?
  test.eq(ordered.stdout, "first\nsecond\nfirst/second/../last\nthird\nfourth\nthird/fourth\n")?
}
