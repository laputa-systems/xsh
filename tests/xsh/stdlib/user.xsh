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

test test_user_groups_primary_gid_override_does_not_grant_account_default {
  let account = user.current()?
  let default_groups = user.groups(account.name)?
  assert user.groups(account.name, primary_gid: null)? == default_groups
  let selected_gid = if account.gid == 0 { 1 } else { 0 }
  let selected_groups = user.groups(account.name, primary_gid: selected_gid)?
  assert selected_gid in selected_groups
  if system.uname()?.sysname != "Darwin" {
    if let Ok(primary_group) = group.by_gid(account.gid) {
      if account.name not in primary_group.members {
        assert account.gid not in selected_groups
      }
    }
  }
  test.error_kind(user.groups("definitely-missing-xsh-user", primary_gid: -1), "user-groups")
  test.error_kind(user.groups(account.name, primary_gid: 4294967295), "user-groups")
}

# The process's supplementary list is what the kernel holds, in its order: it
# is the Groups line of the process status, not the sorted identity set that
# `groups` presents with the primary gid added.
test test_unix_id_supplementary_is_the_kernel_group_list {
  if ! p"/proc/self/status".exists() {
    test.skip("the host has no /proc/self/status")
    return
  }
  let identity = unix.id()?
  let from_status = collect {
    for line in fp"/proc/self/status".lines()? {
      if line.starts_with("Groups:") {
        for word in line.byte_slice(7).fields() { yield word as Int }
      }
    }
  }
  assert identity.supplementary == from_status
  for gid in identity.supplementary {
    assert gid in [entry.gid for entry in identity.groups]
  }
}

test test_user_account_records_remain_intact { |ctx|
  let passwd = test.temp_path(ctx, name: "records-passwd")
  let shadow = test.temp_path(ctx, name: "records-shadow")
  let original = "root:*:000:0:root:/root:/bin/sh\n"
  passwd.write(original)
  shadow.write("root:$hash:0:0:99999:7:::\n")
  let script = test.temp_file(ctx, name: "records-user.xsh", contents: b"let _ = user.add(\"demo\", uid: 2001, gid: 2001)?\nuser.remove(\"demo\")?\n")?
  run XSH_PASSWD_FILE=$passwd XSH_SHADOW_FILE=$shadow "xsh" $script
  assert passwd.read_text()? == original
  assert shadow.read_text()? == "root:$hash:0:0:99999:7:::\n"
}

test test_user_add_preserves_shadow_mode { |ctx|
  let passwd = test.temp_path(ctx, name: "mode-passwd")
  let shadow = test.temp_path(ctx, name: "mode-shadow")
  passwd.write("root:x:0:0:root:/root:/bin/sh\n")
  shadow.write("root:*:0:0:99999:7:::\n", mode: 0o640)
  assert shadow.metadata()?.mode % 4096 == 0o640
  let script = test.temp_file(ctx, name: "mode-user.xsh", contents: b"let _ = user.add(\"demo\", uid: 2001, gid: 2001)?\n")?
  run XSH_PASSWD_FILE=$passwd XSH_SHADOW_FILE=$shadow sh -c "umask 022; exec xsh \"$1\"" sh $script
  assert shadow.metadata()?.mode % 4096 == 0o640
}

test test_account_write_errors_keep_operation_kind { |ctx|
  let missing = test.temp_path(ctx, name: "missing-account-directory")
  let passwd = fp"{missing}/passwd"
  let shadow = fp"{missing}/shadow"
  let group_file = fp"{missing}/group"
  let script = test.temp_file(ctx, name: "account-write-errors.xsh", contents: b"test.error_kind(user.add(\"demo\"), \"user-add\")\ntest.error_kind(group.add(\"demo\"), \"group-add\")\n")?
  run XSH_PASSWD_FILE=$passwd XSH_SHADOW_FILE=$shadow XSH_GROUP_FILE=$group_file "xsh" $script
}

test test_user_account_invalid_records_are_rejected { |ctx|
  let passwd = test.temp_path(ctx, name: "invalid-passwd")
  let shadow = test.temp_path(ctx, name: "invalid-shadow")
  let script = test.temp_file(ctx, name: "invalid-user.xsh", contents: b"test.error_kind(user.add(\"demo\", gecos: \"bad:value\"), \"user-add\")\ntest.error_kind(user.add(\"demo\", home: p\"/bad:home\"), \"user-add\")\ntest.error_kind(user.add(\"demo\", shell: p\"/bad\\nsh\"), \"user-add\")\ntest.error_kind(user.add(\"demo\", uid: -1), \"user-add\")\ntest.error_kind(user.add(\"demo\", gid: 4294967295), \"user-add\")\ntest.error_kind(user.add(\"demo\", gecos: \"bad\\0value\"), \"user-add\")\ntest.error_kind(user.add(\"demo\", home: Path.parse_bytes(b\"/bad\\xff\")?), \"user-add\")\n")?
  let original = "root:*:0:0:root:/root:/bin/sh\n"
  passwd.write(original)
  shadow.write("root:*:0:0:99999:7:::\n")
  run XSH_PASSWD_FILE=$passwd XSH_SHADOW_FILE=$shadow "xsh" $script
  assert passwd.read_text()? == original
  assert shadow.read_text()? == "root:*:0:0:99999:7:::\n"
}

test test_user_malformed_database_is_unchanged { |ctx|
  let passwd = test.temp_path(ctx, name: "malformed-passwd")
  let shadow = test.temp_path(ctx, name: "malformed-shadow")
  let original = "root:*:0:0:root:/root:/bin/sh\nbroken:!:not-a-number:5::/:/bin/sh\n"
  let shadow_original = "root:hash:0:0:99999:7:::\n"
  passwd.write(original)
  shadow.write(shadow_original)
  let script = test.temp_file(ctx, name: "malformed-user.xsh", contents: b"test.error_kind(user.add(\"demo\"), \"user-add\")\ntest.error_kind(user.remove(\"root\"), \"user-remove\")\n")?
  run XSH_PASSWD_FILE=$passwd XSH_SHADOW_FILE=$shadow "xsh" $script
  assert passwd.read_text()? == original
  assert shadow.read_text()? == shadow_original
}
