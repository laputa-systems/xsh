test test_group_lookup_and_mutation_contracts { |ctx|
  let group_file = test.temp_path(ctx, name: "group")

  group_file.write(
    """root:x:0:
""",
  )

  let current_group = group.current()?
  let by_gid = group.by_gid(current_group.gid)?
  assert by_gid.gid == current_group.gid
  assert group.lookup(current_group.name)?.gid == current_group.gid

  let script = test.temp_file(
    ctx,
    name: "group-child.xsh",
    contents: b"let added_group = group.add(\"builders\", gid: 2000)?\nprint ${added_group.name} ${added_group.gid}\ngroup.remove(\"builders\")?\n",
  )?

  let output = run.text XSH_GROUP_FILE=$group_file "xsh" $script
  assert "builders 2000" in output
  test.error_kind(group.lookup("definitely-missing-xsh-group"), "group-not-found")
  test.error_kind(group.add("-bad"), "group-name")
}

test test_group_account_records_remain_intact { |ctx|
  let file = test.temp_path(ctx, name: "records-group")
  let original = "admins:*:0002:alice,,bob\n"
  file.write(original)
  let script = test.temp_file(ctx, name: "records-group.xsh", contents: b"let _ = group.add(\"demo\", gid: 2001)?\ngroup.remove(\"demo\")?\n")?
  run XSH_GROUP_FILE=$file "xsh" $script
  assert file.read_text()? == original
}

test test_group_invalid_records_are_rejected { |ctx|
  let file = test.temp_path(ctx, name: "invalid-group")
  let original = "admins:*:not-a-number:alice\n"
  file.write(original)
  let script = test.temp_file(ctx, name: "invalid-group.xsh", contents: b"test.error_kind(group.add(\"demo\"), \"group-add\")\ntest.error_kind(group.remove(\"admins\"), \"group-remove\")\n")?
  run XSH_GROUP_FILE=$file "xsh" $script
  assert file.read_text()? == original
  file.write("admins:*:2:alice\n")
  let invalid_id = test.temp_file(ctx, name: "invalid-group-id.xsh", contents: b"test.error_kind(group.add(\"demo\", gid: -1), \"group-add\")\n")?
  run XSH_GROUP_FILE=$file "xsh" $invalid_id
  assert file.read_text()? == "admins:*:2:alice\n"
}
