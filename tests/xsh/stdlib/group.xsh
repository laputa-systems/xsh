proc test_group_lookup_and_mutation_contracts(ctx: TestContext) [fs, process, error] {
  let group_file = test.temp_path(ctx, name: "group")

  fs.write(
    group_file,
    """root:x:0:
""",
  )?

  let current_group = group.current()?
  let by_gid = group.by_gid(current_group.gid)?
  test.eq(by_gid.gid, current_group.gid)?
  test.eq(group.lookup(current_group.name)?.gid, current_group.gid)?

  let script = test.temp_file(
    ctx,
    name: "group-child.xsh",
    contents: b"let added_group = group.add(\"builders\", gid: 2000)?\nprint ${added_group.name} ${added_group.gid}\ngroup.remove(\"builders\")?\n",
  )?

  let output = run.text XSH_GROUP_FILE=$group_file "xsh" $script ?
  test.contains(output, "builders 2000")?
  test.error_kind(group.lookup("definitely-missing-xsh-group"), "group-not-found")?
  test.error_kind(group.add("-bad"), "group-name")?
}
