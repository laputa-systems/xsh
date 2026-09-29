proc test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure() [fs, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"nested", parents: true)?
  fs.root_symlink(root, p"target", p"nested/link")?
  fs.root_write(root, p"nested/regular", "data")?

  let observed = fs.root_readlink_result(root, p"nested/link")?
  observed.state == "observed"
  observed.target.require(Path)?.display() == "target"
  observed.errno == null
  fs.root_readlink_result(root, p"nested/../nested/link")?.target.require(Path)?.display() == "target"

  let absent = fs.root_readlink_result(root, p"nested/missing")?
  absent.state == "absent"
  absent.target == null
  absent.error_kind == "not_found"

  let failed = fs.root_readlink_result(root, p"nested/regular")?
  failed.state == "read_failure"
  failed.target == null
  (failed.errno != null)

  let missing_parent = fs.root_readlink_result(root, p"absent/link")?
  missing_parent.state == "absent"
  test.error_kind(fs.root_readlink_result(root, ../escape), "fs-root-readlink-result")?
  test.error_kind(fs.root_readlink_result(root, p"nested/../../escape"), "fs-root-readlink-result")?
}
