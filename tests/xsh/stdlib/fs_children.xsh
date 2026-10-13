test test_fs_children_list_comprehension_iterates_stream { |ctx|
  let root = test.temp_dir(ctx, name: "fs-children-list-comprehension")?
  fp"{root}/first".write("1")
  fp"{root}/second".write("2")

  let from_loop = collect {
    for entry in fs.children(root)? { yield entry.name }
  }
  let from_comprehension = [entry.name for entry in fs.children(root)?]
  let from_live_comprehension = [entry.name for entry in fs.children(root, ordered: false)?]
    |> sort

  assert from_loop == ["first", "second"]
  assert from_comprehension == from_loop
  assert from_live_comprehension == from_loop
}
