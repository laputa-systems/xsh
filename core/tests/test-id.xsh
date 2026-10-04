type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/id.xsh by its real path (so the invoked name is id and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C"}, stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "id")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/id.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

proc out(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Str] {
  Ok(applet_run(ctx, args)?.stdout)
}

test test_id_default_format_names_ids_and_groups { |ctx|
  let me = unix.id()?
  let account = user.by_uid(me.uid)?.name
  let primary = group.by_gid(me.gid)?.name
  let result = applet_run(ctx, [])?

  assert result.status == 0
  assert result.stdout.starts_with(f"uid={me.uid}({account}) gid={me.gid}({primary})"), result.stdout
  assert " groups=" in result.stdout
  assert result.stdout.ends_with("\n")
}

test test_id_single_id_forms { |ctx|
  let me = unix.id()?

  assert out(ctx, ["-u"])? == f"{me.euid}\n"
  assert out(ctx, ["-ur"])? == f"{me.uid}\n"
  assert out(ctx, ["-g"])? == f"{me.egid}\n"
  assert out(ctx, ["--group", "--real"])? == f"{me.gid}\n"
  assert out(ctx, ["-un"])? == f"{user.by_uid(me.euid)?.name}\n"
  assert out(ctx, ["-gn"])? == f"{group.by_gid(me.egid)?.name}\n"
  assert out(ctx, ["-u", "-u"])? == f"{me.euid}\n"
  assert out(ctx, ["-a", "-u"])? == f"{me.euid}\n", "-a is accepted for SVR4 compatibility"
}

test test_id_group_list_starts_with_the_real_gid { |ctx|
  let me = unix.id()?
  let numbers = out(ctx, ["-G"])?
  let names = out(ctx, ["-Gn"])?

  assert numbers.starts_with(f"{me.gid}")
  assert numbers.trim().fields().len() == names.trim().fields().len()
  assert out(ctx, ["-Gr"])? == numbers, "-r does not change -G"
}

test test_id_zero_delimits_with_nul { |ctx|
  let me = unix.id()?

  assert out(ctx, ["-uz"])? == f"{me.euid}\0"
  assert out(ctx, ["-Gz"])?.starts_with(f"{me.gid}")
  assert "\0" in out(ctx, ["--groups", "--zero"])? and "\n" not in out(ctx, ["--groups", "--zero"])?
}

test test_id_named_users { |ctx|
  assert out(ctx, ["-u", "root"])? == "0\n"
  assert out(ctx, ["-un", "0"])? == "root\n", "a numeric operand is a user ID"
  assert out(ctx, ["-g", "root"])? == "0\n"
  assert out(ctx, ["-u", "root", "root"])? == "0\n0\n"

  let missing = applet_run(ctx, ["-u", "no_such_user_xsh", "root"])?
  assert missing.status == 1
  assert missing.stdout == "0\n", "later users are still printed"
  assert missing.stderr == "id: 'no_such_user_xsh': no such user\n", missing.stderr

  let groups = applet_run(ctx, ["root"])?
  assert groups.status == 1
  assert "not supported yet" in groups.stderr
}

test test_id_option_combinations_are_validated_in_gnu_order { |ctx|
  for flag in ["-n", "--name", "-r", "--real"] {
    let result = applet_run(ctx, [flag])?
    assert result.status == 1, flag
    assert result.stderr == "id: printing only names or real IDs requires -u, -g, or -G\n", flag
  }

  for flag in ["-z", "--zero"] {
    assert applet_run(ctx, [flag])?.stderr == "id: option --zero not permitted in default format\n"
    assert applet_run(ctx, [flag, "-n"])?.stderr == "id: printing only names or real IDs requires -u, -g, or -G\n"
  }

  assert applet_run(ctx, ["-u", "-g"])?.stderr == "id: cannot print \"only\" of more than one choice\n"
  assert applet_run(ctx, ["-Z"])?.stderr == "id: --context (-Z) works only on an SELinux-enabled kernel\n"
}

test test_id_help_version_and_errors { |ctx|
  assert "Print user and group information for each specified USER" in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("id ")

  let bad = applet_run(ctx, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "id: unrecognized option '--definitely-invalid'\nTry 'id --help' for more information.\n", bad.stderr
}
