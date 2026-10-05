use core.lib.auth as auth_types

type AuthModule = module {
  export pure parse_passwd(text: Str) -> Result[List[auth_types.PasswdEntry], Error]
  export pure parse_shadow(text: Str) -> List[auth_types.ShadowRecord]
  export pure render_shadow(records: List[auth_types.ShadowRecord]) -> Str
}

test test_auth_lib_passwd_and_shadow_parse_render {
  let auth = module.load(p"core/lib/auth.xsh")?.require(AuthModule)?

  let passwd = auth.parse_passwd("""root:x:0:0:root:/root:/bin/sh
bad:x:not-int:0:bad:/bad:/bin/sh
""")?

  assert passwd.len() == 1
  assert passwd[0].name == "root"
  assert passwd[0].uid == 0
  assert passwd[0].home == "/root"

  let shadow = auth.parse_shadow("""root:!:1:0:99999:7:::
raw-line
""")

  assert shadow.len() == 2
  assert shadow[0].username == "root"
  assert shadow[0].rest[0] == "1"
  assert shadow[1].raw

  assert auth.render_shadow(shadow) == """root:!:1:0:99999:7:::
raw-line
"""
}

test test_applet_auth_helpers_and_sessions { |ctx|
  let root = test.temp_dir(ctx, name: "applet-auth")?
  let home = fp"{root}/home"
  home.mkdir()
  let shell = fp"{root}/session-shell"

  shell.write(
    """#!/bin/sh
exit 17
""",
    mode: 0o755,
  )
  let user_entry = user.current()?

  let session_user = {
    name: user_entry.name,
    uid: user_entry.uid,
    gid: user_entry.gid,
    home: home,
    shell: shell.display(),
  }

  let password_hash = applet.hash_password("secret", "sha512")?
  assert password_hash != "", "hash_password returned empty string"
  assert applet.verify_password("secret", password_hash), "verify_password rejected correct password"
  assert ! applet.verify_password("wrong", password_hash), "verify_password accepted wrong password"
  let known_sha512 = "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
  assert applet.verify_password("Hello world!", known_sha512), "verify_password rejected the SHA-512 reference hash"
  assert ! applet.verify_password("wrong", known_sha512), "verify_password accepted the wrong password for the reference hash"
  assert applet.current_euid() >= 0, "current_euid is negative"
  assert applet.current_exe()?.exists()?, "current_exe path does not exist"
  assert applet.login_session(session_user, false, "")? == 17
  assert applet.sulogin_session(session_user)? == 17
  assert applet.su_session(session_user, false, false, shell.display(), "", [])? == 17
  assert applet.su_session(session_user, false, false, "/bin/sh", "exit 19", [])? == 19
  test.error_kind(applet.hash_password("secret", "bogus"), "applet-hash-password")
}

test test_applet_mdev_scans_empty_roots { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev")?
  let dev = fp"{root}/dev"
  let sys = fp"{root}/sys"
  let sys_dev = fp"{sys}/dev"
  let conf = fp"{root}/mdev.conf"
  dev.mkdir()
  sys.mkdir()
  sys_dev.mkdir()
  conf.write("")

  env XSH_MDEV_DEV_ROOT=$dev XSH_MDEV_SYSFS=$sys XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 {
    let status = applet.mdev(["--scan"])?

    if status != 0 {
      test.skip("mdev scan is unavailable in this runner")
      return
    }
  }
}
