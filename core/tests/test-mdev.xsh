test test_mdev_wrapper_preserves_platform_boundary { |ctx|
  if system.uname()?.sysname == "Linux" {
    test.skip("mdev scan behavior is covered by tests/xsh/stdlib/auth.xsh")
    return
  }

  let err = test.temp_path(ctx, name: "mdev.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- --help 2> $err
  assert ! result.exited_with(0)
  assert "mdev is only available on Linux" in err.read_text()?
}

# One rule moves the node under `disk/`, links the kernel name to it, sets its
# owner and mode, and runs its command on add; remove deletes the node and the
# link. `XSH_MDEV_TEST_PLAIN_FILES` stands a file describing the node in for
# `mknod`, which needs privilege.
test test_mdev_applies_rule_to_temp_device_tree { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/virtual/block/sda"
  let conf = fp"{root}/mdev.conf"
  let log = fp"{root}/mdev.log"
  device_dir.mkdir()
  fp"{device_dir}/dev".write("8:0\n")
  fp"{device_dir}/uevent".write("DEVNAME=sda\nSUBSYSTEM=block\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  let rule_command = r"""@printf '%s:%s' "$MDEV" "$ACTION" > "$LOG"
"""
  conf.write(f"-SUBSYSTEM=block;(sd[a-z]) {uid}:{gid} 640 >disk/%1 {rule_command}")
  let node = fp"{dev_root}/disk/sda"
  let link = fp"{dev_root}/sda"

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 \
      DEVPATH=/devices/virtual/block/sda DEVNAME=sda SUBSYSTEM=block LOG=$log {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert added.status.exited_with(0), added.stderr
    assert node.read_text()? == "b 8:0\n"
    assert node.metadata()?.mode % 512 == 0o640
    assert link.readlink()? == p"disk/sda"
    assert log.read_text()? == "disk/sda:add"

    let removed = run.capture --text ACTION=remove ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert removed.status.exited_with(0), removed.stderr
    assert ! node.exists()?
    assert ! link.exists()?
  }
}
