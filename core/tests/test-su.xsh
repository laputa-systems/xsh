const print_user = r"printf %s $USER"

const print_home = r"printf %s $HOME"

type AccountFiles = {root: Path, passwd: Path, shadow: Path}

type Account = {name: Str, shell: Path}

# Writes account files holding `accounts`, each with the caller's own ids and
# a home directory `NAME-home` under the returned root, so the applet switches
# to them without privilege.
proc account_files(ctx: TestContext, accounts: List[Account]) [fs, process, error] -> Result[AccountFiles] {
  let root = test.temp_dir(ctx, name: "accounts")?
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  var passwd = ""
  var shadow = ""
  for account in accounts {
    let home = fp"{root}/{account.name}-home"
    home.mkdir()
    passwd = f"{passwd}{account.name}:x:{uid}:{gid}:{account.name}:{home}:{account.shell}\n"
    shadow = f"{shadow}{account.name}:hash:0:0:99999:7:::\n"
  }

  let files = {root: root, passwd: fp"{root}/passwd", shadow: fp"{root}/shadow"}
  files.passwd.write(passwd)
  files.shadow.write(shadow)
  Ok(files)
}

test test_su_runs_command_as_target_user { |ctx|
  let files = account_files(ctx, [{name: "nobody", shell: /bin/sh}])?
  let switched = run.capture --text XSH_PASSWD_FILE=${files.passwd} XSH_SHADOW_FILE=${files.shadow} ${ctx.xsh_bin} \
    fp"{ctx.core_dir}/su.xsh" -- -s /bin/sh nobody -c $print_user
  assert switched.status.exited_with(0), switched.stderr
  assert switched.stdout == "nobody"
}

test test_su_login_mode_resets_home { |ctx|
  let files = account_files(ctx, [{name: "root", shell: /bin/sh}])?
  let switched = run.capture --text HOME=/tmp/original XSH_PASSWD_FILE=${files.passwd} \
    XSH_SHADOW_FILE=${files.shadow} ${ctx.xsh_bin} fp"{ctx.core_dir}/su.xsh" -- "-" root -c $print_home
  assert switched.status.exited_with(0), switched.stderr
  assert switched.stdout == fp"{files.root}/root-home".display()
}

test test_su_preserve_environment_keeps_home { |ctx|
  let files = account_files(ctx, [{name: "root", shell: /bin/sh}])?
  let switched = run.capture --text HOME=/tmp/original XSH_PASSWD_FILE=${files.passwd} \
    XSH_SHADOW_FILE=${files.shadow} ${ctx.xsh_bin} fp"{ctx.core_dir}/su.xsh" -- -m root -c $print_home
  assert switched.status.exited_with(0), switched.stderr
  assert switched.stdout == "/tmp/original"
}

# Two entries share the caller's uid. The applet must match the caller against
# the file `XSH_PASSWD_FILE` names and then switch to the entry it was asked
# for, running that entry's shell.
test test_su_uses_xsh_passwd_file_for_current_user_match { |ctx|
  let shell = test.temp_path(ctx, name: "shell")
  shell.write(
    r"""#!/bin/sh
printf '%s' "$USER"
""",
    mode: 0o755,
  )
  let files = account_files(ctx, [{name: "first", shell: shell}, {name: "second", shell: shell}])?
  let switched = run.capture --text XSH_PASSWD_FILE=${files.passwd} XSH_SHADOW_FILE=${files.shadow} ${ctx.xsh_bin} \
    fp"{ctx.core_dir}/su.xsh" -- first -c $print_user
  assert switched.status.exited_with(0), switched.stderr
  assert switched.stdout == "first"
}

test test_su_returns_failure_for_unknown_user { |ctx|
  let err = test.temp_path(ctx, name: "su.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/su.xsh" -- xsh-no-such-user-for-test 2> $err
  assert result.exited_with(1)
  assert "su: user was not found" in err.read_text()?
}
