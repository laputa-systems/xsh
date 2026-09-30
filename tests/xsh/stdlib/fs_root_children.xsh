test test_fs_root_children_reads_newly_created_directory [fs, error] { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children")?
  let root = fs.open_root(root_path)?
  defer root.close()?
  root.mkdir(p"nested")?
  root.write(p"nested/child", "data")?
  let result = root.children(p"nested")?
  test.eq(result.state, "complete")?
  test.ok(result.enumeration_succeeded)?
  test.eq(result.children, [p"nested/child"])?
}

test test_fs_root_children_rejects_regular_file_as_directory [fs, error] { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children-file")?
  let root = fs.open_root(root_path)?
  defer root.close()?
  root.write(p"ordinary-file", "data")?
  let result = root.children(p"ordinary-file")?
  test.eq(result.state, "read_failure")?
  test.ok(! result.enumeration_succeeded)?
  test.eq(result.children, [])?
  test.ok(result.errno != null)?
}
