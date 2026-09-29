proc test_fs_root_children_reads_newly_created_directory(ctx: TestContext) [fs, error] {
  let root_path = test.temp_dir(ctx, name: "root-children")?
  let root = fs.open_root(root_path)?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"nested")?
  fs.root_write(root, p"nested/child", "data")?
  let result = fs.root_children(root, p"nested")?
  test.eq(result.state, "complete")?
  test.ok(result.enumeration_succeeded)?
  test.eq(result.children, [p"nested/child"])?
}

proc test_fs_root_children_rejects_regular_file_as_directory(ctx: TestContext) [fs, error] {
  let root_path = test.temp_dir(ctx, name: "root-children-file")?
  let root = fs.open_root(root_path)?
  defer fs.close_root(root)?
  fs.root_write(root, p"ordinary-file", "data")?
  let result = fs.root_children(root, p"ordinary-file")?
  test.eq(result.state, "read_failure")?
  test.ok(! result.enumeration_succeeded)?
  test.eq(result.children, [])?
  test.ok(result.errno != null)?
}
