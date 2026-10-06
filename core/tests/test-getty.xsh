test test_getty_requires_baud_and_tty { |ctx|
  let err = test.temp_path(ctx, name: "getty.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -- -n -i 2> $err
  assert ! result.exited_with(0)
  assert "missing operand" in err.read_text()?
}

test test_getty_no_prompt_hands_off_to_login_with_term { |ctx|
  let root = test.temp_dir(ctx, name: "getty")?
  let fake_login = fp"{root}/fake-login"
  fake_login.write(
    r"""#!/bin/sh
printf '%s:%s' "${TERM-}" "$#"
""",
    mode: 0o755,
  )

  let handed_off = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -- -i -n -l $fake_login 0 /dev/null \
    vt100
  assert handed_off.status.exited_with(0), handed_off.stderr
  assert handed_off.stdout == "vt100:0"
}

test test_getty_prompts_and_passes_username { |ctx|
  let root = test.temp_dir(ctx, name: "getty")?
  let fake_login = fp"{root}/fake-login"
  fake_login.write(
    r"""#!/bin/sh
printf '%s' "$1"
""",
    mode: 0o755,
  )
  let typed = test.temp_file(ctx, name: "typed.txt", contents: b"alice\n")?

  let prompted = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/getty.xsh" -- -i -l $fake_login 0 /dev/null \
    < ${typed}
  assert prompted.status.exited_with(0), prompted.stderr
  assert prompted.stdout == "login: alice"
}


test test_getty_serial_flags_change_only_the_named_owned_terminal { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let table = unix.tty_table()
  var mask = 0
  for flag in table.flags {
    if flag.name in ["clocal", "crtscts"] { mask = mask.bit_or(flag.value) }
  }
  let original = unix.tty_attrs(pty.replica)?
  unix.set_tty_attrs({...original, cflag: original.cflag.clear_bits(mask)}, pty.replica)
  let root = test.temp_dir(ctx, name: "getty-serial")?
  let login = fp"{root}/login"
  login.write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let result = test.run_script(ctx, fp"{ctx.core_dir}/getty.xsh".read_text()?, ["-i", "-n", "-h", "-L", "-l", login.display(), "0", pty.name], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "getty")?
  assert result.success, result.stderr
  let changed = unix.tty_attrs(pty.replica)?
  assert changed.cflag.bit_and(mask) == mask
  assert changed.iflag == original.iflag
  assert changed.oflag == original.oflag
  assert changed.lflag == original.lflag
}

test test_getty_modem_options_fail_before_terminal_or_login { |ctx|
  for option in ["-m", "-w"] {
    let result = test.run_script(ctx, fp"{ctx.core_dir}/getty.xsh".read_text()?, [option, "-i", "-n", "-l", "/missing-login", "0", "/missing-terminal"], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "getty")?
    assert ! result.success
    assert "not supported" in result.stderr
    assert "modem" in result.stderr
  }
}


test test_getty_serial_flag_failure_prevents_login { |ctx|
  let root = test.temp_dir(ctx, name: "getty-invalid-terminal")?
  let marker = fp"{root}/login-ran"
  let login = fp"{root}/login"
  login.write(f"#!/bin/sh\nprintf ran > {marker}\n", mode: 0o755)
  let terminal_file = fp"{root}/not-a-terminal"
  terminal_file.write("original")
  let result = test.run_script(ctx, fp"{ctx.core_dir}/getty.xsh".read_text()?, ["-h", "-i", "-n", "-l", login.display(), "0", terminal_file.display()], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "getty")?
  assert ! result.success
  assert ! marker.exists()?
  assert terminal_file.read_text()? == "original"
}


test test_getty_timeout_is_rejected_before_prompt_terminal_and_login { |ctx|
  for option in ["-t", "--timeout"] {
    let result = test.run_script(ctx, fp"{ctx.core_dir}/getty.xsh".read_text()?, [option, "1", "-I", "not-written", "-l", "/missing-login", "0", "/missing-terminal"], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "getty")?
    assert ! result.success
    assert "not supported" in result.stderr
    assert "timeout" in result.stderr
    assert result.stdout == ""
  }
}

test test_getty_baud_changes_the_named_owned_terminal { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let original = unix.tty_attrs(pty.replica)?
  let root = test.temp_dir(ctx, name: "getty-baud")?
  let login = fp"{root}/login"
  login.write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let result = test.run_script(ctx, fp"{ctx.core_dir}/getty.xsh".read_text()?, ["-i", "-n", "-l", login.display(), "9600", pty.name], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "getty")?
  assert result.success, result.stderr
  let changed = unix.tty_attrs(pty.replica)?
  assert changed.ispeed == 9600
  assert changed.ospeed == 9600
  for flag in unix.tty_table().flags {
    if flag.field == "cflag" { assert changed.cflag.bit_and(flag.mask) == original.cflag.bit_and(flag.mask) }
  }
  assert changed.iflag == original.iflag
  assert changed.oflag == original.oflag
  assert changed.lflag == original.lflag
}

test test_getty_invalid_or_multiple_baud_rates_fail_before_login { |ctx|
  for baud in ["invalid", "9600,19200", "12345", "-1"] {
    let result = test.run_script(ctx, fp"{ctx.core_dir}/getty.xsh".read_text()?, ["-i", "-n", "-l", "/missing-login", "--", baud, "/missing-terminal"], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "getty")?
    assert ! result.success
    assert "baud" in result.stderr
    assert "missing-login" not in result.stderr
  }
}
