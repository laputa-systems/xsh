##! Transcribed from uutils tests/by-util/test_id.rs.
use support.uu as uu

# Expected bytes come from the process credentials and account database,
# independently of the applet being exercised.
proc expected_id(s: uu.Scene, args: List[Str], users: List[Str] = []) [fs, process, env, error] {
  let me = unix.id()?
  let names = "--name" in args
  let real = "--real" in args
  let zero = "--zero" in args or "-z" in args
  let mode = if "--user" in args { "user" } else if "--group" in args { "group" } else if "--groups" in args { "groups" } else { "default" }
  let targets: List[Str?] = if users.is_empty() { [null] } else { [name for name in users] }
  var output = ""
  var errors = ""
  for target in targets {
    var uid = if real { me.uid } else { me.euid }
    var gid = if real { me.gid } else { me.egid }
    var gids = [me.gid, @if me.egid != me.gid { [me.egid] } else { [] }, @[g for g in me.supplementary if g != me.gid and g != me.egid]]
    if target != null {
      guard let account = user.lookup(target) else {
        errors += f"id: '{target}': no such user\n"
        continue
      }
      uid = account.uid
      gid = account.gid
      gids = [gid, @[g for g in user.groups(target)? if g != gid]]
    }
    if mode == "user" {
      output += if names { user.by_uid(uid)?.name } else { f"{uid}" }
    } else if mode == "group" {
      output += if names { group.by_gid(gid)?.name } else { f"{gid}" }
    } else if mode == "groups" {
      let words = collect {
        for g in gids { yield if names { group.by_gid(g)?.name } else { f"{g}" } }
      }
      output += words.join(if zero { "\0" } else { " " })
    } else {
      let ruid = if target == null { me.uid } else { uid }
      let rgid = if target == null { me.gid } else { gid }
      output += f"uid={ruid}"
      if let Ok(account) = user.by_uid(ruid) { output += f"({account.name})" }
      output += f" gid={rgid}"
      if let Ok(entry) = group.by_gid(rgid) { output += f"({entry.name})" }
      if target == null and me.euid != me.uid {
        output += f" euid={me.euid}"
        if let Ok(account) = user.by_uid(me.euid) { output += f"({account.name})" }
      }
      if target == null and me.egid != me.gid {
        output += f" egid={me.egid}"
        if let Ok(entry) = group.by_gid(me.egid) { output += f"({entry.name})" }
      }
      let default_gids = if target == null { [me.egid, @[g for g in me.supplementary if g != me.egid]] } else { gids }
      var labels: List[Str] = []
      for g in default_gids {
        var label = f"{g}"
        if let Ok(entry) = group.by_gid(g) { label += f"({entry.name})" }
        labels += [label]
      }
      output += f" groups={labels.join(",")}"
    }
    output += if zero { if mode == "groups" and users.len() > 1 { "\0\0" } else { "\0" } } else { "\n" }
  }
  let r = uu.invoke(s, "id", args.extend(users))?
  uu.fails_with_code(r, if errors == "" { 0 } else { 1 })
  uu.stdout_is_bytes(r, bytes.from_text(output))
  uu.stderr_is(r, errors)
}

proc user_matrix(s: uu.Scene, users: List[Str]) [fs, process, env, error] {
  expected_id(s, [], users)
  for opt in ["--user", "--group", "--groups"] {
    expected_id(s, [opt], users)
    expected_id(s, [opt, "--zero"], users)
    expected_id(s, [opt, "--zero", "--name"], users)
    expected_id(s, [opt, "--zero"], users)
  }
}

# origin: uutils test_id::test_invalid_arg
test test_uu_id_invalid_arg { |ctx|
  uu.fails_with_code(uu.invoke(uu.scene(ctx)?, "id", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_id::test_id_ignore
test test_uu_id_id_ignore { |ctx|
  uu.succeeds(uu.invoke(uu.scene(ctx)?, "id", ["-a"])?)
}

# origin: uutils test_id::test_id_no_specified_user
test test_uu_id_id_no_specified_user { |ctx|
  expected_id(uu.scene(ctx)?, [])
}

# origin: uutils test_id::test_id_single_user
test test_uu_id_id_single_user { |ctx|
  user_matrix(uu.scene(ctx)?, [user.current()?.name])
}

# origin: uutils test_id::test_id_single_user_non_existing
test test_uu_id_id_single_user_non_existing { |ctx|
  expected_id(uu.scene(ctx)?, [], ["hopefully_non_existing_username"])
}

# origin: uutils test_id::test_id_name
test test_uu_id_id_name { |ctx|
  let s = uu.scene(ctx)?
  for opt in ["--user", "--group", "--groups"] { expected_id(s, [opt, "--name"]) }
  let r = uu.invoke(s, "id", ["--user", "--name"])?
  assert r.stdout.utf8()? == f"{user.current()?.name}\n"
}

# origin: uutils test_id::test_id_real
test test_uu_id_id_real { |ctx|
  let s = uu.scene(ctx)?
  for opt in ["--user", "--group", "--groups"] { expected_id(s, [opt, "--real"]) }
}

# origin: uutils test_id::test_id_groups_ordering
test test_uu_id_id_groups_ordering { |ctx|
  let s = uu.scene(ctx)?
  let groups = uu.invoke(s, "id", ["-G"])?
  uu.succeeds(groups)
  let from_flag = groups.stdout.utf8()?.fields()
  assert !from_flag.is_empty()
  let rgid = uu.invoke(s, "id", ["-g", "-r"])?
  uu.succeeds(rgid)
  assert f"{from_flag[0]}\n" == rgid.stdout.utf8()?
  let default = uu.invoke(s, "id", [])?
  uu.succeeds(default)
  let field = default.stdout.utf8()?.split(" groups=")[1].fields()[0]
  let from_default = [g.split("(")[0] for g in field.split(",")]
  assert (from_flag |> sort) == (from_default |> sort)
  let real = uu.invoke(s, "id", ["-G", "-r"])?
  uu.succeeds(real)
  uu.stdout_is_bytes(real, groups.stdout)
}

# origin: uutils test_id::test_id_multiple_users
test test_uu_id_id_multiple_users { |ctx|
  user_matrix(uu.scene(ctx)?, ["root", "man", "postfix", "sshd", user.current()?.name])
}

# origin: uutils test_id::test_id_multiple_users_non_existing
test test_uu_id_id_multiple_users_non_existing { |ctx|
  let who = user.current()?.name
  user_matrix(uu.scene(ctx)?, ["root", "hopefully_non_existing_username1", who, "man", "hopefully_non_existing_username2", "hopefully_non_existing_username3", "postfix", "sshd", "hopefully_non_existing_username4", who])
}

# origin: uutils test_id::test_id_name_or_real_with_default_format
test test_uu_id_id_name_or_real_with_default_format { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-n", "--name", "-r", "--real"] {
    let r = uu.invoke(s, "id", [flag])?
    uu.fails(r)
    uu.stderr_only(r, "id: printing only names or real IDs requires -u, -g, or -G\n")
  }
}

# origin: uutils test_id::test_id_default_format
test test_uu_id_id_default_format { |ctx|
  let s = uu.scene(ctx)?
  for opt1 in ["--name", "--real"] {
    for opt2 in ["--user", "--group", "--groups"] { expected_id(s, [opt2, opt1]) }
  }
  for opt in ["--user", "--group", "--groups"] {
    expected_id(s, [opt])
    expected_id(s, [opt, opt])
  }
}

# origin: uutils test_id::test_id_zero_with_default_format
test test_uu_id_id_zero_with_default_format { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-z", "--zero"] {
    let r = uu.invoke(s, "id", [flag])?
    uu.fails(r)
    uu.stderr_only(r, "id: option --zero not permitted in default format\n")
  }
}

# origin: uutils test_id::test_id_zero_with_name_or_real
test test_uu_id_id_zero_with_name_or_real { |ctx|
  let s = uu.scene(ctx)?
  for zero in ["-z", "--zero"] {
    for flag in ["-n", "--name", "-r", "--real"] {
      let r = uu.invoke(s, "id", [zero, flag])?
      uu.fails(r)
      uu.stderr_only(r, "id: printing only names or real IDs requires -u, -g, or -G\n")
    }
  }
}

# origin: uutils test_id::test_id_zero
test test_uu_id_id_zero { |ctx|
  let s = uu.scene(ctx)?
  for zero in ["-z", "--zero"] {
    for opt1 in ["--name", "--real"] {
      for opt2 in ["--user", "--group", "--groups"] { expected_id(s, [opt2, zero, opt1]) }
    }
    for opt in ["--user", "--group", "--groups"] { expected_id(s, [opt, zero]) }
  }
}

# origin: uutils test_id::test_id_no_specified_user_posixly
test test_uu_id_id_no_specified_user_posixly { |ctx|
  let r = uu.invoke(uu.scene(ctx)?, "id", [], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  assert "context=" not in r.stdout.utf8()?
}

# A numeric login name takes precedence over the numeric UID interpretation.
# The upstream test runs only when the system has a name or UID resolving 200.
# origin: uutils test_id::test_id_digital_username
test test_uu_id_id_digital_username { |ctx|
  var exists = false
  if let Ok(account) = user.lookup("200") { exists = true }
  if let Ok(account) = user.by_uid(200) { exists = true }
  if exists {
    uu.succeeds(uu.invoke(uu.scene(ctx)?, "id", ["200"])?)
  }
}
