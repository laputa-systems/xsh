proc test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure() [fs, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"nested", parents: true)?
  fs.root_symlink(root, p"target", p"nested/link")?
  fs.root_write(root, p"nested/regular", "data")?

  let observed = fs.root_readlink_result(root, p"nested/link")?
  test.eq(observed.state, "observed")?
  test.eq(observed.target.require(Path)?.display(), "target")?
  test.eq(observed.errno, null)?
  test.eq(fs.root_readlink_result(root, p"nested/../nested/link")?.target.require(Path)?.display(), "target")?

  let absent = fs.root_readlink_result(root, p"nested/missing")?
  test.eq(absent.state, "absent")?
  test.eq(absent.target, null)?
  test.eq(absent.error_kind, "not_found")?

  let failed = fs.root_readlink_result(root, p"nested/regular")?
  test.eq(failed.state, "read_failure")?
  test.eq(failed.target, null)?
  test.ok(failed.errno != null)?

  let missing_parent = fs.root_readlink_result(root, p"absent/link")?
  test.eq(missing_parent.state, "absent")?
  test.error_kind(fs.root_readlink_result(root, p"../escape"), "fs-root-readlink-result")?
  test.error_kind(fs.root_readlink_result(root, p"nested/../../escape"), "fs-root-readlink-result")?
}
