type AccountFiles = {passwd: Path, shadow: Path}

# Writes account files holding one user, `name`, with the caller's own ids and
# the password hash `hash`, so the applet changes them without privilege.
proc account_files(ctx: TestContext, name: Str) [fs, process, error] -> Result[AccountFiles] {
  let root = test.temp_dir(ctx, name: "accounts")?
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  let home = fp"{root}/{name}-home"
  home.mkdir()
  let files = {passwd: fp"{root}/passwd", shadow: fp"{root}/shadow"}
  files.passwd.write(f"{name}:x:{uid}:{gid}:{name}:{home}:/bin/sh\n")
  files.shadow.write(f"{name}:hash:0:0:99999:7:::\n")
  Ok(files)
}

# The password field of `name`'s entry in the shadow file.
proc shadow_password(shadow: Path, name: Str) [fs, error] -> Result[Str] {
  for line in shadow.lines()? {
    let fields = line.split(":")
    return Ok(fields[1]) when fields[0] == name
  }

  fail f"no shadow entry for {name}"
}

# Runs the passwd applet on `files` with `typed` on standard input.
proc passwd(
  ctx: TestContext,
  files: AccountFiles,
  arguments: List[Str],
  typed: Bytes,
) [fs, process, error] -> Result[Status] {
  let input = test.temp_file(ctx, name: "typed.txt", contents: typed)?
  let ran = run.capture --text XSH_PASSWD_FILE=${files.passwd} XSH_SHADOW_FILE=${files.shadow} ${ctx.xsh_bin} \
    fp"{ctx.core_dir}/passwd.xsh" -- @arguments < ${input}
  Ok(ran.status)
}

test test_passwd_sets_default_password_hash { |ctx|
  let files = account_files(ctx, "root")?
  let status = passwd(ctx, files, ["root"], b"secret\nsecret\n")?
  assert status.exited_with(0)
  let password = shadow_password(files.shadow, "root")?
  assert password != ""
  assert password != "hash"

  if system.uname()?.sysname == "Linux" {
    assert password.starts_with("$6$"), password
  }
}

test test_passwd_honors_md5_algorithm { |ctx|
  let files = account_files(ctx, "root")?
  let status = passwd(ctx, files, ["-a", "md5", "root"], b"secret\nsecret\n")?
  assert status.exited_with(0)
  let password = shadow_password(files.shadow, "root")?
  assert password != ""
  assert password != "hash"

  if system.uname()?.sysname == "Linux" {
    assert password.starts_with("$1$"), password
  }
}

test test_passwd_delete_lock_and_unlock_update_shadow { |ctx|
  let files = account_files(ctx, "root")?

  assert passwd(ctx, files, ["-l", "root"], b"")?.exited_with(0)
  assert shadow_password(files.shadow, "root")? == "!hash"

  assert passwd(ctx, files, ["-u", "root"], b"")?.exited_with(0)
  assert shadow_password(files.shadow, "root")? == "hash"

  assert passwd(ctx, files, ["-d", "root"], b"")?.exited_with(0)
  assert shadow_password(files.shadow, "root")? == ""
}

test test_passwd_rejects_mismatched_passwords_without_changing_shadow { |ctx|
  let files = account_files(ctx, "root")?
  let status = passwd(ctx, files, ["root"], b"one\ntwo\n")?
  assert ! status.ok
  assert shadow_password(files.shadow, "root")? == "hash"
}

# Without a user operand the applet changes the caller, found by its uid in
# the file `XSH_PASSWD_FILE` names rather than in the system database.
test test_passwd_defaults_to_current_user_from_xsh_passwd_file { |ctx|
  let files = account_files(ctx, "current")?
  assert passwd(ctx, files, ["-d"], b"")?.exited_with(0)
  assert shadow_password(files.shadow, "current")? == ""
}

test test_passwd_rejects_extra_operands { |ctx|
  let err = test.temp_path(ctx, name: "passwd.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/passwd.xsh" -- user extra 2> $err
  assert ! result.exited_with(0)
  assert "extra operand" in err.read_text()?
}
