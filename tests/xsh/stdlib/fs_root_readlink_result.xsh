test test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"nested", parents: true)?
  root.symlink(p"target", p"nested/link")?
  root.write(p"nested/regular", "data")?

  let observed = root.readlink_result(p"nested/link")?
  (observed.state) == ("observed")
  (observed.target.require(Path)?.display()) == ("target")
  (observed.errno) == (null)
  (root.readlink_result(p"nested/../nested/link")?.target.require(Path)?.display()) == ("target")

  let absent = root.readlink_result(p"nested/missing")?
  (absent.state) == ("absent")
  (absent.target) == (null)
  (absent.error_kind) == ("not_found")

  let failed = root.readlink_result(p"nested/regular")?
  (failed.state) == ("read_failure")
  (failed.target) == (null)
  (failed.errno != null)

  let missing_parent = root.readlink_result(p"absent/link")?
  (missing_parent.state) == ("absent")
  test.error_kind(root.readlink_result(../escape), "fs-root-readlink-result")?
  test.error_kind(root.readlink_result(p"nested/../../escape"), "fs-root-readlink-result")?
}
