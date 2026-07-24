proc test_file_audit_findings(ctx: TestContext) [fs, process, error] {
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
  test.contains(output, "broken-symlink broken")?
  test.contains(output, "absolute-symlink absolute")?
  test.contains(output, "escaping-symlink escape")?
  test.contains(output, "world-writable-file world.txt")?
  test.contains(output, "world-writable-dir open-dir")?

  if suid.metadata()?.setuid {
    test.contains(output, "setuid-setgid-file suid.sh")?
  }
}
