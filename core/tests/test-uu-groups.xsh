##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_groups.rs.

use support.uu as uu

# Account databases vary between test images; derive the GNU expectation from
# their contents, retaining database order for supplementary memberships.
type ExpectedGroups = {status: Int, stdout: Str, stderr: Str}

proc account_groups(name: Str) -> ExpectedGroups {
  for line in p"/etc/passwd".read_text()?.lines() {
    let fields = line.split(":")
    if fields[0] != name { continue }
    let primary = fields[3].parse_int()?
    var gids = [primary]
    for row in p"/etc/group".read_text()?.lines() {
      let group_fields = row.split(":")
      let gid = group_fields[2].parse_int()?
      if name in group_fields[3].split(",") and gid not in gids { gids += [gid] }
    }
    let names = [group.by_gid(gid)?.name for gid in gids]
    return {status: 0, stdout: f"{name} : {names.join(" ")}\n", stderr: ""}
  }
  {status: 1, stdout: "", stderr: f"groups: '{name}': no such user\n"}
}

# The upstream helper selects USER, then USERNAME, then nobody.
proc test_username() -> Str {
  env.get("USER") ?? env.get("USERNAME") ?? "nobody"
}

# origin: uutils test_groups::test_groups
test test_uu_groups_groups { |ctx|
  let s = uu.scene(ctx)?
  let me = unix.id()?
  var gids = [me.gid]
  if me.egid not in gids { gids += [me.egid] }
  for gid in me.supplementary {
    if gid not in gids { gids += [gid] }
  }
  let expected = [group.by_gid(gid)?.name for gid in gids].join(" ") + "\n"
  let r = uu.invoke(s, "groups", [])?
  uu.succeeds(r)
  uu.stdout_only(r, expected)
}

# origin: uutils test_groups::test_groups_username
test test_uu_groups_groups_username { |ctx|
  let s = uu.scene(ctx)?
  let name = test_username()
  let expected = account_groups(name)
  let r = uu.invoke(s, "groups", [name])?
  uu.succeeds(r)
  assert r.status == expected.status
  uu.stdout_is(r, expected.stdout)
  uu.stderr_is(r, expected.stderr)
}

# origin: uutils test_groups::test_groups_username_multiple
test test_uu_groups_groups_username_multiple { |ctx|
  let s = uu.scene(ctx)?
  let names = ["root", "man", "postfix", "sshd", test_username()]
  var stdout = ""
  var stderr = ""
  var status = 0
  for name in names {
    let expected = account_groups(name)
    stdout += expected.stdout
    stderr += expected.stderr
    if expected.status != 0 { status = expected.status }
  }
  let r = uu.invoke(s, "groups", names)?
  uu.fails(r)
  assert r.status == status
  uu.stdout_is(r, stdout)
  uu.stderr_is(r, stderr)
}

# origin: uutils test_groups::test_invalid_arg
test test_uu_groups_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "groups", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}
