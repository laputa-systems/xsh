test test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"nested", parents: true)
  root.symlink(p"target", p"nested/link")
  root.write(p"nested/regular", "data")

  let observed = root.readlink_result(p"nested/link")?
  assert observed.state == "observed"
  assert observed.target.require(Path)?.display() == "target"
  assert observed.errno == null
  assert root.readlink_result(p"nested/../nested/link")?.target.require(Path)?.display() == "target"

  let absent = root.readlink_result(p"nested/missing")?
  assert absent.state == "absent"
  assert absent.target == null
  assert absent.error_kind == "not_found"

  let failed = root.readlink_result(p"nested/regular")?
  assert failed.state == "read_failure"
  assert failed.target == null
  assert failed.errno != null

  let missing_parent = root.readlink_result(p"absent/link")?
  assert missing_parent.state == "absent"
  test.error_kind(root.readlink_result(../escape), "fs-root-readlink-result")
  test.error_kind(root.readlink_result(p"nested/../../escape"), "fs-root-readlink-result")
}
