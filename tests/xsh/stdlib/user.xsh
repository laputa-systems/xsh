pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_user_lookup_and_mutation_contracts(ctx: TestContext) [fs, process, error] {
  let passwd_file = test.temp_path(ctx, name: "passwd")
  let shadow_file = test.temp_path(ctx, name: "shadow")
  let group_file = test.temp_path(ctx, name: "group")

  fs.write(
    passwd_file,
    """root:x:0:0:root:/root:/bin/sh
""",
  )?

  fs.write(
    shadow_file,
    """root:*:0:0:99999:7:::
""",
  )?

  fs.write(
    group_file,
    """root:x:0:
""",
  )?

  let current_user = user.current()?
  let by_uid = user.by_uid(current_user.uid)?
  test.eq(by_uid.uid, current_user.uid)?
  test.eq(user.lookup(current_user.name)?.uid, current_user.uid)?

  let script = test.temp_file(
    ctx,
    name: "user-child.xsh",
    contents: b"let added_user = user.add(\"demo\", uid: 2001, gid: 2001, home: p\"/home/demo\", shell: p\"/bin/false\", gecos: \"Demo User\")?\nprint ${added_user.name} ${added_user.home.display()}\nuser.remove(\"demo\")?\n",
  )?

  test.ok(xsh_bin().exists()?, "child xsh binary should be built before stdlib tests")?
  let output = run.text XSH_PASSWD_FILE=$passwd_file XSH_SHADOW_FILE=$shadow_file XSH_GROUP_FILE=$group_file xsh_bin() $script ?
  test.contains(output, "demo /home/demo")?
  test.error_kind(user.lookup("definitely-missing-xsh-user"), "user-not-found")?
  test.error_kind(user.add("-bad"), "user-name")?
}
