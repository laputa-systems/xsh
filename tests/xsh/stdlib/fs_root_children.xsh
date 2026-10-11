test test_fs_root_children_reads_newly_created_directory { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children")?
  let root = fs.open_root(root_path)?
  defer root.close()
  root.mkdir(p"nested")
  root.write(p"nested/child", "data")
  let result = root.children(p"nested")?
  assert result.state == "complete"
  assert result.enumeration_succeeded
  assert result.children == [p"nested/child"]
}

test test_fs_root_children_rejects_regular_file_as_directory { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children-file")?
  let root = fs.open_root(root_path)?
  defer root.close()
  root.write(p"ordinary-file", "data")
  let result = root.children(p"ordinary-file")?
  assert result.state == "read_failure"
  assert ! result.enumeration_succeeded
  assert result.children == []
  assert result.errno != null
}

test test_fs_root_children_order_matches_filesystem_enumeration { |ctx|
  let root_path = test.temp_dir(ctx, name: "root-children-order")?
  let root = fs.open_root(root_path)?
  defer root.close()
  root.mkdir(p"nested")
  for name in ["zebra", "alpha", "middle", "beta"] {
    root.write(fp"nested/{name}", name)
  }
  let native = fs.children(fp"{root_path}/nested", ordered: false)?
    |> map { |entry| fp"nested/{entry.name}" }
    |> collect()
  let unordered = root.children(p"nested", ordered: false)?
  assert unordered.state == "complete"
  assert unordered.enumeration_succeeded
  assert unordered.children == native
  let sorted = native |> sort()
  assert root.children(p"nested")?.children == sorted
  assert root.children(p"nested", ordered: true)?.children == sorted
  let sorted_bounded = root.children(p"nested", max_entries: 2)?
  assert sorted_bounded.state == "truncated"
  assert sorted_bounded.children == sorted[0..2]
  let bounded = root.children(p"nested", max_entries: 2, ordered: false)?
  assert bounded.state == "truncated"
  assert ! bounded.enumeration_succeeded
  assert bounded.children == native[0..2]
  assert bounded.errno == null
  assert bounded.error_kind == null
  let empty = root.children(p"nested", max_entries: 0, ordered: false)?
  assert empty.state == "truncated"
  assert empty.children == []
  root.mkdir(p"empty")
  let complete = root.children(p"empty", max_entries: 0, ordered: false)?
  assert complete.state == "complete"
  assert complete.enumeration_succeeded
  assert complete.children == []
}
