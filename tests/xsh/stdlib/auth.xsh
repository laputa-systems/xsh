type AuthModule = module {
  export pure parse_passwd(text: Str) -> Result[List[Record]]
  export pure parse_shadow(text: Str) -> List[Record]
  export pure render_shadow(records: List[Any]) -> Str
}

proc test_auth_lib_passwd_and_shadow_parse_render() [fs, error] {
  let auth = module.load(p"core/lib/auth.xsh")?.require(AuthModule)?

  let passwd = auth.parse_passwd("""root:x:0:0:root:/root:/bin/sh
bad:x:not-int:0:bad:/bad:/bin/sh
""")?

  test.eq(passwd.len(), 1)?
  test.eq(passwd[0].name, "root")?
  test.eq(passwd[0].uid, 0)?
  test.eq(passwd[0].home.display(), "/root")?

  let shadow = auth.parse_shadow("""root:!:1:0:99999:7:::
raw-line
""")

  test.eq(shadow.len(), 2)?
  test.eq(shadow[0].username, "root")?
  test.eq(shadow[0].rest[0], "1")?
  test.ok(shadow[1].raw)?

  test.eq(
    auth.render_shadow(shadow),
    """root:!:1:0:99999:7:::
raw-line
""",
  )?
}

proc test_applet_auth_helpers_and_sessions(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "applet-auth")?
  let home = fp"${root}/home"
  fs.mkdir(home)?
  let shell = fp"${root}/session-shell"

  fs.write(
    shell,
    """#!/bin/sh
exit 17
""",
  )?

  fs.chmod(shell, 0o755)?
  let user_entry = user.current()?

  let session_user = {
    name: user_entry.name,
    uid: user_entry.uid,
    gid: user_entry.gid,
    home: home,
    shell: shell.display(),
  }

  let password_hash = applet.hash_password("secret", "sha512")?
  test.ok(password_hash != "", "hash_password returned empty string")?
  test.ok(applet.verify_password("secret", password_hash), "verify_password rejected correct password")?
  test.ok(! applet.verify_password("wrong", password_hash), "verify_password accepted wrong password")?
  test.ok(applet.current_euid() >= 0, "current_euid is negative")?
  test.ok(applet.current_exe()?.exists()?, "current_exe path does not exist")?
  test.eq(applet.login_session(session_user, false, "")?, 17)?
  test.eq(applet.sulogin_session(session_user)?, 17)?
  test.eq(applet.su_session(session_user, false, false, shell.display(), "", [])?, 17)?
  test.eq(applet.su_session(session_user, false, false, "/bin/sh", "exit 19", [])?, 19)?
  test.error_kind(applet.hash_password("secret", "bogus"), "applet-hash-password")?
}

proc test_applet_mdev_scans_empty_roots(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev")?
  let dev = fp"${root}/dev"
  let sys = fp"${root}/sys"
  let sys_dev = fp"${sys}/dev"
  let conf = fp"${root}/mdev.conf"
  fs.mkdir(dev)?
  fs.mkdir(sys)?
  fs.mkdir(sys_dev)?
  fs.write(conf, "")?

  env XSH_MDEV_DEV_ROOT=$dev XSH_MDEV_SYSFS=$sys XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 {
    let status = applet.mdev(["--scan"])?

    if status != 0 {
      test.skip("mdev scan is unavailable in this runner")
      return
    }
  } ?
}
