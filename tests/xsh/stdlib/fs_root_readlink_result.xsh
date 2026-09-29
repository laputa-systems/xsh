test test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure [fs, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"nested", parents: true)?
  root.symlink(p"target", p"nested/link")?
  root.write(p"nested/regular", "data")?

  let observed = root.readlink_result(p"nested/link")?
  test.eq(observed.state, "observed")?
  test.eq(observed.target.require(Path)?.display(), "target")?
  test.eq(observed.errno, null)?
  test.eq(root.readlink_result(p"nested/../nested/link")?.target.require(Path)?.display(), "target")?

  let absent = root.readlink_result(p"nested/missing")?
  test.eq(absent.state, "absent")?
  test.eq(absent.target, null)?
  test.eq(absent.error_kind, "not_found")?

  let failed = root.readlink_result(p"nested/regular")?
  test.eq(failed.state, "read_failure")?
  test.eq(failed.target, null)?
  test.ok(failed.errno != null)?

  let missing_parent = root.readlink_result(p"absent/link")?
  test.eq(missing_parent.state, "absent")?
  test.error_kind(root.readlink_result(../escape), "fs-root-readlink-result")?
  test.error_kind(root.readlink_result(p"nested/../../escape"), "fs-root-readlink-result")?
}
