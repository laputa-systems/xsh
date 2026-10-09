test test_path_audit_findings { |ctx|
  let root = test.temp_dir(ctx, name: "path-audit")?
  let bin1 = fp"{root}/bin1"
  let bin2 = fp"{root}/bin2"
  let duplicate = fp"{root}/dup-bin1"
  let world = fp"{root}/world"
  let noexec = fp"{root}/noexec"
  let file_entry = fp"{root}/file-entry"
  bin1.mkdir()
  bin2.mkdir()
  world.mkdir()
  world.chmod(0o777)
  noexec.mkdir()
  noexec.chmod(0o666)
  file_entry.write("not a directory")
  fs.symlink(bin1, duplicate)

  fp"{bin1}/tool".write("""#!/bin/sh
""")

  fp"{bin1}/tool".chmod(0o755)

  fp"{bin2}/tool".write("""#!/bin/sh
""")

  fp"{bin2}/tool".chmod(0o755)
  let missing = fp"{root}/missing"
  let raw = f"{bin1}:{bin2}:{duplicate}:{missing}:{file_entry}::{world}:{noexec}"

  env XSH_SHOWCASE_PATH=$raw {
    let output = run.text "xsh" "showcase/path-audit.xsh" --var XSH_SHOWCASE_PATH ?
    assert "Directory problems" in output
    assert "duplicate-directory" in output
    assert "missing-directory" in output
    assert "not-directory" in output
    assert "empty-entry" in output
    assert "world-writable-directory" in output
    assert "non-executable-directory" in output
    assert "Command shadowing" in output
    assert "shadowed-command tool" in output
  }
}

test test_path_audit_distinguishes_non_utf8_command_names { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("creating non-UTF-8 path components requires the pinned Linux filesystem")
    return
  }

  let root = test.temp_dir(ctx, name: "path-audit-byte-names")?
  let bin1 = fp"{root}/bin1"
  let bin2 = fp"{root}/bin2"
  bin1.mkdir()
  bin2.mkdir()
  let first = Path.parse_bytes(bytes.concat([bin1.bytes(), b"/tool-\xff"]))?
  let second = Path.parse_bytes(bytes.concat([bin2.bytes(), b"/tool-\xfe"]))?
  first.write("""#!/bin/sh
""")
  second.write("""#!/bin/sh
""")
  first.chmod(0o755)
  second.chmod(0o755)

  let raw = f"{bin1}:{bin2}"
  env XSH_SHOWCASE_PATH=$raw {
    let output = run.text "xsh" "showcase/path-audit.xsh" --var XSH_SHOWCASE_PATH ?
    let distinct_commands = "shadowed-command" not in output
    let audit_message = output
    assert distinct_commands, audit_message
  }
}
