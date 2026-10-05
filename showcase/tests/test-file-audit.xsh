test test_file_audit_findings { |ctx|
  let root = test.temp_dir(ctx, name: "file-audit")?
  let outside = test.temp_dir(ctx, name: "file-audit-outside")?
  fp"{root}/world.txt".write("world", mode: 0o666)
  fp"{root}/open-dir".mkdir()
  fp"{root}/open-dir".chmod(0o777)
  let suid = fp"{root}/suid.sh"

  suid.write(
    """#!/bin/sh
""",
    mode: 0o4755,
  )
  fp"{outside}/target.txt".write("outside")
  fp"{root}/broken".symlink(to: p"missing-target")
  fp"{root}/absolute".symlink(to: fp"{root}/world.txt")
  fp"{root}/escape".symlink(to: fp"{outside}/target.txt")
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
  let prefix = parent.bytes()
  let root_bytes = bytes.concat([prefix, b"/", b"\xff"])
  let outside_bytes = bytes.concat([prefix, b"/", b"\xfe"])
  let root = root_bytes as Path
  let outside = outside_bytes as Path
  root.mkdir()
  outside.mkdir()
  let root_alias = fp"{parent}/root-alias"
  root_alias.symlink(to: root)
  let target = bytes.concat([outside_bytes, b"/target"]) as Path
  let link = bytes.concat([root_bytes, b"/escape"]) as Path
  target.write("outside")
  link.symlink(to: target)

  let output = run.text "xsh" "showcase/file-audit.xsh" -- --root $root_alias ?
  assert "escaping-symlink escape" in output
}
