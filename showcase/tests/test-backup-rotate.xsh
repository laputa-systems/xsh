pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_backup_rotate(ctx: TestContext) [fs, process, error] {
  let dir = test.temp_dir(ctx, name: "backups")?
  fp"${dir}/backup-2024-01-01.tar.gz".write("old1")?
  fp"${dir}/backup-2024-06-01.tar.gz".write("old2")?
  fp"${dir}/backup-2025-01-01.tar.gz".write("new1")?
  fp"${dir}/backup-2025-12-01.tar.gz".write("newest")?
  let output = run.text xsh_bin() "showcase/backup-rotate.xsh" -- --dir $dir --keep 2 --dry-run=false ?
  test.contains(output, "kept 2")?
  test.contains(output, "deleted 2")?
  test.ok(fp"${dir}/backup-2025-12-01.tar.gz".exists()?)?
  test.eq(fp"${dir}/backup-2024-01-01.tar.gz".exists()?, false)?
}
