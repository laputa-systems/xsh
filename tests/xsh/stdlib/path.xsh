test test_path_absolute [fs, error] {
  let absolute = path.absolute(p"docs")?
  absolute.display().ends_with("/docs")
}

test test_membership_operator_supports_strings_lists_bytes_and_paths [error] {
  ("lib" in "usr/lib/libz.so")
  ("libz.so" in ["libz.so", "libc.so"])
  (b"TODO" in b"one TODO two")
  (p"usr/lib" in p"usr/lib/libz.so")
  (p"bin" in p"usr/lib/libz.so") == false
}

test test_path_methods [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "path-methods")?
  let file = fp"${root}/dir/file.txt"
  file.parent().mkdir()?
  file.write("hello")?
  file.read_text()? == "hello"
  file.write_atomic(b"bytes")?
  file.read_bytes()? == b"bytes"
  file.name() == "file.txt"
  file.ext() == "txt"
  file.basename() == "file.txt"
  file.dirname().display() == fp"${root}/dir".display()
  p"/".basename() == "/"
  p".".basename() == "."
  p"a/.".dirname().display() == "a"
  p"a/".basename() == "a"
  p".profile".ext_or("none") == "none"
  p"file.".ext_or("none") == ""
  p"plain".ext_or("none") == "none"
  file.with_ext("log").name() == "file.log"
  fp"${root}/dir/../dir/file.txt".normalize() == file
  file.strip_prefix(root)?.display() == "dir/file.txt"
  file.relative_to(root).display() == "dir/file.txt"
  file.resolve()?.display().ends_with("file.txt")
  file.exists()?
  ! file.executable()?
  (file.du()? >= 0)
  file.metadata()?.kind == "file"
  file.chmod(0o600)?
  file.truncate(2)?
  file.read_text()? == "by"
  let copied = fp"${root}/copy.txt"
  file.copy(copied)?
  copied.read_text()? == "by"
  let renamed = fp"${root}/renamed.txt"
  copied.rename(renamed)?
  renamed.exists()?
  let link = fp"${root}/link.txt"
  file.hardlink(link)?
  link.read_text()? == "by"
  let symlink = fp"${root}/symlink.txt"
  fs.symlink(file, symlink)?
  symlink.readlink()?.display() == file.display()
  link.unlink()?
  ! link.exists()?
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
  parsed.display() == "relative/path"
  Path.parse_bytes(b"byte/path")?.display() == "byte/path"
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
  path_entry_name(meta) == "space name"
  let raw_path = Path.parse_bytes(b"bad\xffname")?
  ("bad" in raw_path.display())

  let raw = test.run_script(
    ctx,
    r"""
let raw_path = Path.parse_bytes(b"bad\xffname")?
run printf "%s" (raw_path) ?
""",
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = raw
    assert assertion_condition, assertion_message
  }
  raw.stdout_bytes == b"bad\xffname"
}

pure path_entry_name(entry: FsEntry) -> Str {
  entry.name
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  output.stdout == """hit.txt
"""
}

test test_path_interpolation_retains_native_bytes_and_text_boundaries [error] { |ctx|
  let raw = Path.parse_bytes(b"raw\xff name")?
  fp"prefix/${raw}/../end" == Path.parse_bytes(b"prefix/raw\xff name/../end")?
  fp"${p"left"}/${"right"}/${7}/${false}" == p"left/right/7/false"
  fp"${raw:>12}" == Path.parse_bytes(b"   raw\xff name")?
  f"${raw}" == raw.display()
  fp"${raw.display()}" != raw
  let output = test.run_script(ctx, r"""
let raw = Path.parse_bytes(b"raw\xff name/'\"")?
run printf "%s" "--target=$raw" ?
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  output.stdout_bytes == b"--target=raw\xff name/'\""
}

test test_path_text_conversions_remain_distinct_from_native_arguments [error] { |ctx|
  let output = test.run_script(ctx, r"""
let raw = Path.parse_bytes(b"raw\xff name")?
run printf "%s\n" "--target=${raw.display()}" ?
run printf "%s\n" f"${raw.display()}" ?
run printf "%s\n" f"${raw}" ?
run printf "%s\n" (Path(raw.display())) ?
run printf "%s\n" (Path(f"${raw}/child")) ?
run printf "%s\n" (raw) ?
""")?
  output.success
  output.stdout_bytes == b"--target=raw\xef\xbf\xbd name\nraw\xef\xbf\xbd name\nraw\xef\xbf\xbd name\nraw\xef\xbf\xbd name\nraw\xef\xbf\xbd name/child\nraw\xff name\n"
}

test test_path_interpolation_rejects_nul_and_keeps_effect_order [error] { |ctx|
  let failed = test.run_script(ctx, r"""
let text = "\0"
let invalid = fp"prefix/${text}"
print "unexpected"
""")?
  !failed.success
  "NUL" in failed.stderr
  let argv_failed = test.run_script(ctx, r"""
let text = "\0"
run printf "%s" "value=$text" ?
""")?
  !argv_failed.success
  "NUL" in argv_failed.stderr
  argv_failed.stdout_bytes == b""
  let bytes_failed = test.run_script(ctx, r"""
let invalid = fp"${b"raw"}"
""")?
  !bytes_failed.success
  "display" in bytes_failed.stderr
  let ordered = test.run_script(ctx, r"""
proc piece(label: Str) [io] -> Path { print --flush $label; return Path(label) }
let result = fp"${piece("first")}/${piece("second")}/../last"
print --flush $result
run printf "%s\n" "${piece("third")}/${piece("fourth")}" ?
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = ordered
    assert assertion_condition, assertion_message
  }
  ordered.stdout == "first\nsecond\nfirst/second/../last\nthird\nfourth\nthird/fourth\n"
}
