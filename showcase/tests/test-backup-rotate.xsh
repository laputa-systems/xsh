proc test_backup_rotate(ctx: TestContext) [fs, process, error] {
  let dir = test.temp_dir(ctx, name: "backups")?
  fp"${dir}/backup-2024-01-01.tar.gz".write("old1")?
  fp"${dir}/backup-2024-06-01.tar.gz".write("old2")?
  fp"${dir}/backup-2025-01-01.tar.gz".write("new1")?
  fp"${dir}/backup-2025-12-01.tar.gz".write("newest")?
  let output = run.text "xsh" "showcase/backup-rotate.xsh" -- --dir $dir --keep 2 --dry-run=false ?
  test.contains(output, "kept 2")?
  test.contains(output, "deleted 2")?
  test.ok(fp"${dir}/backup-2025-12-01.tar.gz".exists()?)?
  test.eq(fp"${dir}/backup-2024-01-01.tar.gz".exists()?, false)?
}

proc test_backup_rotate_only_deletes_direct_backup_files(ctx: TestContext) [fs, process, error] {
  let dir = test.temp_dir(ctx, name: "backups-with-subdir")?
  fp"${dir}/backup-2024-01-01.tar.gz".write("old")?
  fp"${dir}/backup-2025-01-01.tar.gz".write("new")?
  let nested = fp"${dir}/a-unrelated"
  nested.mkdir()?
  fp"${nested}/notes.txt".write("keep me")?

  let output = run.text "xsh" "showcase/backup-rotate.xsh" -- --dir $dir --keep 1 --dry-run=false ?
  test.contains(output, "deleted 1")?
  test.ok(fp"${dir}/backup-2025-01-01.tar.gz".exists()?)?
  test.ok(! fp"${dir}/backup-2024-01-01.tar.gz".exists()?)?
  test.eq(fp"${nested}/notes.txt".read_text()?, "keep me")?
}

proc test_backup_rotate_reports_failed_deletion_without_claiming_it_happened(ctx: TestContext) [fs, process, error] {
  if user.current()?.uid == 0 {
    test.skip("permission-denied deletion requires an unprivileged test user")
    return
  }

  let dir = test.temp_dir(ctx, name: "backups-read-only")?
  let old = fp"${dir}/backup-2024-01-01.tar.gz"
  let newest = fp"${dir}/backup-2025-01-01.tar.gz"
  old.write("old")?
  newest.write("new")?
  dir.chmod(0o555)?
  defer dir.chmod(0o700)?

  let output = run.capture --text "xsh" "showcase/backup-rotate.xsh" -- --dir $dir --keep 1 --dry-run=false ?
  test.ok(! output.status.exited_with(0), "deletion must report permission failure")?
  test.ok(! output.stdout.contains("delete: backup-2024-01-01.tar.gz"), "failed deletion must not be reported as done")?
  test.eq(old.read_text()?, "old")?
  test.eq(newest.read_text()?, "new")?
}
