type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/groups.xsh by its real path (so the invoked name is groups and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "groups")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/groups.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

proc current_groups() [fs, process, error] -> Result[Str] {
  let me = unix.id()?
  let names = [entry.name for entry in me.groups if entry.gid != me.gid]

  Ok([group.by_gid(me.gid)?.name, @names].join(" "))
}

test test_groups_lists_the_current_process_groups { |ctx|
  let result = applet_run(ctx, [])?
  assert result.status == 0
  assert result.stdout == f"{current_groups()?}\n"
}

test test_groups_reports_unknown_users_and_lists_named_user_groups { |ctx|
  let missing = applet_run(ctx, ["no_such_user_xsh"])?
  assert missing.status == 1
  assert missing.stdout == ""
  assert missing.stderr == "groups: 'no_such_user_xsh': no such user\n", missing.stderr

  let named = applet_run(ctx, ["root"])?
  let primary = group.by_gid(user.lookup("root")?.gid)?.name
  assert named.status == 0
  assert named.stdout.starts_with(f"root : {primary}"), named.stdout
  assert named.stdout.ends_with("\n")
  assert applet_run(ctx, ["root", "root"])?.stdout == f"{named.stdout}{named.stdout}", "each operand gets its own line"
}

test test_groups_help_version_and_errors { |ctx|
  assert "Print group memberships for each USERNAME" in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("groups ")

  let bad = applet_run(ctx, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "groups: unrecognized option '--definitely-invalid'\nTry 'groups --help' for more information.\n", bad.stderr
}
