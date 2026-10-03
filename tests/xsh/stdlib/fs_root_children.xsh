test test_fs_root_children_reads_newly_created_directory { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children")?
  let root = fs.open_root(root_path)?
  defer root.close()?
  root.mkdir(p"nested")?
  root.write(p"nested/child", "data")?
  let result = root.children(p"nested")?
  assert result.state == "complete"
  assert result.enumeration_succeeded
  assert result.children == [p"nested/child"]
}

test test_fs_root_children_rejects_regular_file_as_directory { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children-file")?
  let root = fs.open_root(root_path)?
  defer root.close()?
  root.write(p"ordinary-file", "data")?
  let result = root.children(p"ordinary-file")?
  assert result.state == "read_failure"
  assert ! result.enumeration_succeeded
  assert result.children == []
  assert result.errno != null
}
