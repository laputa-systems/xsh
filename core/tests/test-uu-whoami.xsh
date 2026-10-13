##! Transcribed from the uutils coreutils integration tests for whoami.

use support.uu

# origin: uutils test_whoami::test_invalid_arg
test test_uu_whoami_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "whoami", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_whoami::test_normal
test test_uu_whoami_normal { |ctx|
  let s = uu.scene(ctx)?
  let account = user.by_uid(unix.id()?.euid)?
  let r = uu.invoke(s, "whoami", [])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{account.name}\n")
  uu.stderr_is(r, "")
}

# origin: uutils test_whoami::test_normal_compare_env
test test_uu_whoami_normal_compare_env { |ctx|
  let s = uu.scene(ctx)?
  let account = user.by_uid(unix.id()?.euid)?
  let username = env.get_or("USER", env.get_or("USERNAME", "nobody")?)?
  let ci = env.get_or("CI", "")?.lower() == "true"
  if username != "nobody" and !ci {
    let inherited = uu.invoke(s, "whoami", [])?
    uu.succeeds(inherited)
    uu.stdout_is(inherited, f"{username}\n")
  }
  # A scoped account fixture also exercises the comparison when the runner omits USER.
  env ({USER: account.name, CI: "false"}) {
    let expected = env.get("USER")?
    assert expected != "nobody"
    assert env.get("CI")? == "false"
    let r = uu.invoke(s, "whoami", [])?
    uu.succeeds(r)
    uu.stdout_is(r, f"{expected}\n")
  } ?
}

# origin: uutils test_whoami::test_normal_compare_id
test test_uu_whoami_normal_compare_id { |ctx|
  let s = uu.scene(ctx)?
  let account = user.by_uid(unix.id()?.euid)?
  let id_un = uu.invoke(s, "id", ["-un"])?
  uu.succeeds(id_un)
  uu.stdout_is(id_un, f"{account.name}\n")
  let r = uu.invoke(s, "whoami", [])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, id_un.stdout)
}

# origin: uutils test_whoami::test_succeeds_on_all_platforms
test test_uu_whoami_succeeds_on_all_platforms { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "whoami", [])?
  uu.succeeds(r)
  uu.no_stderr(r)
}
