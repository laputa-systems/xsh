use sshd

test replaces_commented_default {
  let updated = sshd.set_option("Port 22\n#PermitRootLogin yes\n", "PermitRootLogin", "no")
  assert updated == "Port 22\nPermitRootLogin no\n"
}

test appends_missing_key {
  let updated = sshd.set_option("Port 22\n", "PasswordAuthentication", "no")
  assert updated.ends_with("PasswordAuthentication no\n"), "new keys go at the end"
}

test harden_script_edits_file_and_keeps_backup { |ctx|
  let dir = test.temp_dir(ctx)?
  let config = fp"{dir}/sshd_config"
  config.write("#PermitRootLogin yes\n")

  let source = p"bin/harden.xsh".read_text()?
  let _ = test.expect(ctx, source, status: 0, args: ["--config", config])?
  assert config.read_text()? == "PermitRootLogin no\n"
  assert fp"{config}.bak".exists()?
}
