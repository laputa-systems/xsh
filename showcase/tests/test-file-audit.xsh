test test_file_audit_findings { |ctx|
  let root = test.temp_dir(ctx, name: "file-audit")?
  let outside = test.temp_dir(ctx, name: "file-audit-outside")?
  fp"${root}/world.txt".write("world")?
  fp"${root}/world.txt".chmod(0o666)?
  fp"${root}/open-dir".mkdir()?
  fp"${root}/open-dir".chmod(0o777)?
  let suid = fp"${root}/suid.sh"

  suid.write("""#!/bin/sh
""")?

  suid.chmod(0o4755)?
  fp"${outside}/target.txt".write("outside")?
  fs.symlink(p"missing-target", fp"${root}/broken")?
  fs.symlink(fp"${root}/world.txt", fp"${root}/absolute")?
  fs.symlink(fp"${outside}/target.txt", fp"${root}/escape")?
  let output = run.text "xsh" "showcase/file-audit.xsh" -- --root $root ?
  assert "broken-symlink broken" in output
  assert "absolute-symlink absolute" in output
  assert "escaping-symlink escape" in output
  assert "world-writable-file world.txt" in output
  assert "world-writable-dir open-dir" in output

  if suid.metadata()?.setuid {
    assert "setuid-setgid-file suid.sh" in output
  }
}

test test_file_audit_distinguishes_non_utf8_sibling_paths { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("creating non-UTF-8 path components requires the pinned Linux filesystem")
    return
  }

  let parent = test.temp_dir(ctx, name: "file-audit-byte-paths")?
  let prefix = bytes.from_text(parent.display())
  let root_bytes = bytes.concat([prefix, b"/", b"\xff"])
  let outside_bytes = bytes.concat([prefix, b"/", b"\xfe"])
  let root = Path.parse_bytes(root_bytes)?
  let outside = Path.parse_bytes(outside_bytes)?
  root.mkdir()?
  outside.mkdir()?
  let root_alias = fp"${parent}/root-alias"
  fs.symlink(root, root_alias)?
  let target = Path.parse_bytes(bytes.concat([outside_bytes, b"/target"]))?
  let link = Path.parse_bytes(bytes.concat([root_bytes, b"/escape"]))?
  target.write("outside")?
  fs.symlink(target, link)?

  let output = run.text "xsh" "showcase/file-audit.xsh" -- --root $root_alias ?
  assert "escaping-symlink escape" in output
}
