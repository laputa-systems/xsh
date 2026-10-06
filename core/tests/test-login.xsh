type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], variables: Record, input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "login-capture")?
  let output = fp"{root}/stdout"
  let errors = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/login.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, variables, input, output, errors))?
  Ok({status: status.exit_code()?, stdout: output.read_text()?, stderr: errors.read_text()?})
}

test test_login_f_requires_actual_root_and_help_is_nonmutating { |ctx|
  let result = invoke(ctx, ["-f", "root"], {})?
  assert result.status == 1
  assert result.stderr.find("requires real and effective UID 0") != null
  assert invoke(ctx, ["--help"], {})?.status == 0
}

test test_login_fixture_cannot_select_another_uid { |ctx|
  let source = test.temp_file(ctx, name: "passwd-foreign", contents: b"foreign:x:0:0:Foreign:/root:/bin/sh\n")?
  let result = invoke(ctx, ["foreign"], {XSH_PASSWD_FILE: source.display()}, b"ignored\n")?
  assert result.status == 1
  assert result.stderr.find("fixture identity must match the caller") != null
}

test test_login_uses_account_fixture_and_resets_environment { |ctx|
  let identity = unix.id()?
  let root = test.temp_dir(ctx, name: "login-account")?
  let shell = fp"{root}/shell"
  shell.write(r"""#!/bin/sh
printf '%s\n' "$USER" "$LOGNAME" "$HOME" "$SHELL" "$PWD"
if [ "${UNTRUSTED+x}" = x ]; then printf 'UNTRUSTED\n'; fi
""", mode: 0o755)
  let passwd = fp"{root}/passwd"
  passwd.write(f"fixture:x:{identity.uid}:{identity.gid}:Fixture:{root}:{shell}\n")
  let password_hash = applet.hash_password("fixture-password", "sha512")?
  let shadow = fp"{root}/shadow"
  shadow.write(f"fixture:{password_hash}:20000:0:99999:7:::\n")
  let group_file = fp"{root}/group"
  var memberships = ""
  for group_entry in identity.groups { memberships += f"g{group_entry.gid}:x:{group_entry.gid}:fixture\n" }
  group_file.write(memberships)
  let result = invoke(ctx, ["fixture"], {XSH_PASSWD_FILE: passwd.display(), XSH_SHADOW_FILE: shadow.display(), XSH_GROUP_FILE: group_file.display(), XSH_NOLOGIN_FILE: fp"{root}/absent-nologin".display(), UNTRUSTED: "value", PATH: "/bad"}, b"fixture-password\n")?
  assert result.status == 0, result.stderr
  assert result.stdout.lines().collect() == ["Password: fixture", "fixture", root.display(), shell.display(), root.display()]
}

test test_login_wrong_password_and_nologin_refuse_session { |ctx|
  let identity = unix.id()?
  let root = test.temp_dir(ctx, name: "login-denied")?
  let passwd = fp"{root}/passwd"
  passwd.write(f"fixture:x:{identity.uid}:{identity.gid}:Fixture:{root}:/bin/sh\n")
  let shadow = fp"{root}/shadow"
  shadow.write(f"fixture:{applet.hash_password("fixture-password", "sha512")?}:20000:0:99999:7:::\n")
  let nologin = fp"{root}/nologin"
  let variables = {XSH_PASSWD_FILE: passwd.display(), XSH_SHADOW_FILE: shadow.display(), XSH_NOLOGIN_FILE: nologin.display()}
  let wrong = invoke(ctx, ["fixture"], variables, b"wrong\n")?
  assert wrong.status == 1
  assert wrong.stderr.find("Login incorrect") != null
  nologin.write("maintenance\n")
  let denied = invoke(ctx, ["fixture"], variables, b"fixture-password\n")?
  assert denied.status == 1
  assert denied.stdout == "maintenance\n"
}
