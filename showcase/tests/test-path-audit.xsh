pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_path_audit_findings(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "path-audit")?
  let bin1 = fp"${root}/bin1"
  let bin2 = fp"${root}/bin2"
  let duplicate = fp"${root}/dup-bin1"
  let world = fp"${root}/world"
  let noexec = fp"${root}/noexec"
  let file_entry = fp"${root}/file-entry"
  bin1.mkdir()?
  bin2.mkdir()?
  world.mkdir()?
  world.chmod(0o777)?
  noexec.mkdir()?
  noexec.chmod(0o666)?
  file_entry.write("not a directory")?
  fs.symlink(bin1, duplicate)?

  fp"${bin1}/tool".write("""#!/bin/sh
""")?

  fp"${bin1}/tool".chmod(0o755)?

  fp"${bin2}/tool".write("""#!/bin/sh
""")?

  fp"${bin2}/tool".chmod(0o755)?
  let missing = fp"${root}/missing"
  let raw = f"${bin1.display()}:${bin2.display()}:${duplicate.display()}:${missing.display()}:${file_entry.display()}::${world.display()}:${noexec.display()}"

  env XSH_SHOWCASE_PATH=$raw {
    let output = run.text xsh_bin() "showcase/path-audit.xsh" -- --var XSH_SHOWCASE_PATH ?
    test.contains(output, "Directory problems")?
    test.contains(output, "duplicate-directory")?
    test.contains(output, "missing-directory")?
    test.contains(output, "not-directory")?
    test.contains(output, "empty-entry")?
    test.contains(output, "world-writable-directory")?
    test.contains(output, "non-executable-directory")?
    test.contains(output, "Command shadowing")?
    test.contains(output, "shadowed-command tool")?
  } ?
}
