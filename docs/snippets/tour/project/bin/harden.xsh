use sshd

cli main(config: Path = /etc/ssh/sshd_config) {
  let before = config.read_text()?
  let after = sshd.set_option(before, "PermitRootLogin", "no")
  if after != before {
    config.copy(to: fp"{config}.bak", overwrite: true)
    config.write_atomic(after)
    print f"updated {config}"
  }
}
