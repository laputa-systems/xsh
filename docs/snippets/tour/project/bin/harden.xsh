use sshd

cli main(config: Path = /etc/ssh/sshd_config) {
  let before = config.read_text()?
  let after = sshd.set_option(before, "PermitRootLogin", "no")
  if after != before {
    fs.copy(config, fp"${config}.bak", overwrite: true)?
    config.write_atomic(after)?
    print f"updated ${config}"
  }
}
