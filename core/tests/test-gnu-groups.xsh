use support.uu as uu

# origin: gnu groups/groups-dash.log
test test_gnu_groups_groups_dash_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "groups", [":invalid", "--"])?
  uu.fails_with_code(r, 1)
  assert bytes.concat([r.stdout, r.stderr]) == b"groups: ':invalid': no such user\n"
}

# origin: gnu groups/groups-process-all.log
test test_gnu_groups_groups_process_all_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "groups", [":1", ":2", ":3"])?
  uu.fails_with_code(r, 1)
  assert r.stderr.utf8()?.split("\n").len() == 4
}

# origin: gnu groups/groups-version.log
test test_gnu_groups_groups_version_log { |ctx|
  let s = uu.scene(ctx)?
  let groups = uu.invoke(s, "groups", ["--version"])?
  let identity = uu.invoke(s, "id", ["--version"])?
  uu.succeeds(groups)
  uu.succeeds(identity)
  let renamed = [if line.starts_with("groups") { "id" + line.byte_slice(6) } else { line } for line in groups.stdout.utf8()?.split("\n\n")[0].split("\n")]
  assert renamed.join("\n") == identity.stdout.utf8()?.split("\n\n")[0]
}
