test test_user_lookup_and_mutation_contracts { |ctx|
  let passwd_file = test.temp_path(ctx, name: "passwd")
  let shadow_file = test.temp_path(ctx, name: "shadow")
  let group_file = test.temp_path(ctx, name: "group")

  passwd_file.write(
    """root:x:0:0:root:/root:/bin/sh
""",
  )

  shadow_file.write(
    """root:*:0:0:99999:7:::
""",
  )

  group_file.write(
    """root:x:0:
""",
  )

  let current_user = user.current()?
  let by_uid = user.by_uid(current_user.uid)?
  assert by_uid.uid == current_user.uid
  assert user.lookup(current_user.name)?.uid == current_user.uid

  let script = test.temp_file(
    ctx,
    name: "user-child.xsh",
    contents: b"let added_user = user.add(\"demo\", uid: 2001, gid: 2001, home: p\"/home/demo\", shell: p\"/bin/false\", gecos: \"Demo User\")?\nprint ${added_user.name} ${added_user.home}\nuser.remove(\"demo\")?\n",
  )?

  let output = run.text XSH_PASSWD_FILE=$passwd_file XSH_SHADOW_FILE=$shadow_file XSH_GROUP_FILE=$group_file "xsh" \
    $script
  assert "demo /home/demo" in output
  test.error_kind(user.lookup("definitely-missing-xsh-user"), "user-not-found")
  test.error_kind(user.add("-bad"), "user-name")
}

test test_user_groups_resolves_account_memberships_without_changing_credentials {
  let account = user.current()?
  let before = unix.id()?
  let memberships = user.groups(account.name)?
  assert account.gid in memberships
  assert memberships == (memberships |> sort |> unique-by .)
  assert unix.id()? == before
  test.error_kind(user.groups("definitely-missing-xsh-user"), "user-not-found")
  test.error_kind(user.groups("invalid\0name"), "user-name")
}
