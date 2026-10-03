test test_path_absolute {
  let absolute = path.absolute(p"docs")?
  assert absolute.display().ends_with("/docs")
}

test test_membership_operator_supports_strings_lists_bytes_and_paths {
  assert "lib" in "usr/lib/libz.so"
  assert "libz.so" in ["libz.so", "libc.so"]
  assert b"TODO" in b"one TODO two"
  assert p"usr/lib" in p"usr/lib/libz.so"
  assert p"bin" in p"usr/lib/libz.so" == false
}

test test_path_methods { |ctx|
  let root = test.temp_dir(ctx, name: "path-methods")?
  let file = fp"${root}/dir/file.txt"
  file.parent().mkdir()?
  file.write("hello")?
  assert file.read_text()? == "hello"
  file.write_atomic(b"bytes")?
  assert file.read_bytes()? == b"bytes"
  assert file.name() == "file.txt"
  assert file.ext() == "txt"
  assert file.basename() == "file.txt"
  assert file.dirname().display() == fp"${root}/dir".display()
  assert p"/".basename() == "/"
  assert p".".basename() == "."
  assert p"a/.".dirname().display() == "a"
  assert p"a/".basename() == "a"
  assert p".profile".ext_or("none") == "none"
  assert p"file.".ext_or("none") == ""
  assert p"plain".ext_or("none") == "none"
  assert file.with_ext("log").name() == "file.log"
  assert fp"${root}/dir/../dir/file.txt".normalize() == file
  assert file.strip_prefix(root)?.display() == "dir/file.txt"
  assert file.relative_to(root).display() == "dir/file.txt"
  assert file.resolve()?.display().ends_with("file.txt")
  assert file.exists()?
  assert ! file.executable()?
  assert file.du()? >= 0
  assert file.metadata()?.kind == "file"
  file.chmod(0o600)?
  file.truncate(2)?
  assert file.read_text()? == "by"
  let copied = fp"${root}/copy.txt"
  file.copy(copied)?
  assert copied.read_text()? == "by"
  let renamed = fp"${root}/renamed.txt"
  copied.rename(renamed)?
  assert renamed.exists()?
  let link = fp"${root}/link.txt"
  file.hardlink(link)?
  assert link.read_text()? == "by"
  let symlink = fp"${root}/symlink.txt"
  fs.symlink(file, symlink)?
  assert symlink.readlink()?.display() == file.display()
  link.unlink()?
  assert ! link.exists()?
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
  assert parsed.display() == "relative/path"
  assert Path.parse_bytes(b"byte/path")?.display() == "byte/path"
}

test test_path_edge_cases_and_standard_record_schema { |ctx|
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
  assert path_entry_name(meta) == "space name"
  let raw_path = Path.parse_bytes(b"bad\xffname")?
  assert "bad" in raw_path.display()

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
  assert raw.stdout_bytes == b"bad\xffname"
}

pure path_entry_name(entry: FsEntry) -> Str {
  entry.name
}

test test_absolute_glob_traverses_symlinked_literal_components { |ctx|
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

  assert output.stdout == """hit.txt
"""
}

test test_path_interpolation_retains_native_bytes_and_text_boundaries { |ctx|
  let raw = Path.parse_bytes(b"raw\xff name")?
  assert fp"prefix/${raw}/../end" == Path.parse_bytes(b"prefix/raw\xff name/../end")?
  assert fp"${p"left"}/${"right"}/${7}/${false}" == p"left/right/7/false"
  assert fp"${raw:>12}" == Path.parse_bytes(b"   raw\xff name")?
  assert f"${raw}" == raw.display()
  assert fp"${raw.display()}" != raw
  let output = test.run_script(
    ctx,
    r"""
let raw = Path.parse_bytes(b"raw\xff name/'\"")?
run printf "%s" "--target=$raw" ?
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout_bytes == b"--target=raw\xff name/'\""
}

test test_path_text_conversions_remain_distinct_from_native_arguments { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let raw = Path.parse_bytes(b"raw\xff name")?
run printf "%s\n" "--target=${raw.display()}" ?
run printf "%s\n" f"${raw.display()}" ?
run printf "%s\n" f"${raw}" ?
run printf "%s\n" (Path(raw.display())) ?
run printf "%s\n" (Path(f"${raw}/child")) ?
run printf "%s\n" (raw) ?
""",
  )?
  assert output.success
  assert output.stdout_bytes == b"--target=raw\xef\xbf\xbd name\nraw\xef\xbf\xbd name\nraw\xef\xbf\xbd name\nraw\xef\xbf\xbd name\nraw\xef\xbf\xbd name/child\nraw\xff name\n"
}

test test_path_interpolation_rejects_nul_and_keeps_effect_order { |ctx|
  let failed = test.run_script(
    ctx,
    r"""
let text = "\0"
let invalid = fp"prefix/${text}"
print "unexpected"
""",
  )?
  assert ! failed.success
  assert "NUL" in failed.stderr
  let argv_failed = test.run_script(
    ctx,
    r"""
let text = "\0"
run printf "%s" "value=$text" ?
""",
  )?
  assert ! argv_failed.success
  assert "NUL" in argv_failed.stderr
  assert argv_failed.stdout_bytes == b""
  let bytes_failed = test.run_script(
    ctx,
    r"""
let invalid = fp"${b"raw"}"
""",
  )?
  assert ! bytes_failed.success
  assert "display" in bytes_failed.stderr
  let ordered = test.run_script(
    ctx,
    r"""
proc piece(label: Str) [io] -> Path { print --flush $label; return Path(label) }
let result = fp"${piece("first")}/${piece("second")}/../last"
print --flush $result
run printf "%s\n" "${piece("third")}/${piece("fourth")}" ?
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = ordered
    assert assertion_condition, assertion_message
  }
  assert ordered.stdout == """first
second
first/second/../last
third
fourth
third/fourth
"""
}
