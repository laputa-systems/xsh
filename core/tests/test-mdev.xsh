test test_mdev_reports_linux_platform_requirement { |ctx|
  if system.uname()?.sysname == "Linux" {
    test.skip("Linux mdev behavior has dedicated tests")
    return
  }

  let err = test.temp_path(ctx, name: "mdev.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- --help 2> $err
  assert ! result.exited_with(0)
  assert "mdev is only available on Linux" in err.read_text()?
}

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
  let rule_command = r"""@printf '%s:%s:%s' "$MDEV" "$ACTION" "$0" > "$LOG"
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
    assert log.read_text()? == "disk/sda:add:sh"
    assert log.metadata()?.mode % 512 == 0o666

    let removed = run.capture --text ACTION=remove ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert removed.status.exited_with(0), removed.stderr
    assert ! node.exists()?
    assert ! link.exists()?

    let changed = run.capture --text ACTION=change ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert changed.status.exited_with(0), changed.stderr
    assert ! node.exists()?
    assert ! link.exists()?
    assert log.read_text()? == "disk/sda:add:sh"
  }
}

test test_mdev_scan_matches_major_minor_ranges { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-scan")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/dev/block/8:16"
  let conf = fp"{root}/mdev.conf"
  device_dir.mkdir()
  fp"{device_dir}/dev".write("8:16\n")
  fp"{device_dir}/uevent".write("DEVNAME=sdb\nSUBSYSTEM=block\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"@8,16-31 {uid}:{gid} 604 =disk/\n")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 {
    let scanned = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- --scan
    let node = fp"{dev_root}/disk/sdb"
    assert scanned.status.exited_with(0), scanned.stderr
    assert node.read_text()? == "b 8:16\n"
    assert node.metadata()?.mode % 512 == 0o604
    assert ! fp"{dev_root}/sdb".exists()?
  }
}

test test_mdev_accepts_positive_decimal_signs { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-positive-decimals")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/virtual/block/sda"
  let conf = fp"{root}/mdev.conf"
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("DEVNAME=sda\nSUBSYSTEM=block\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"@+1,+3 {uid}:{gid} 600 =disk/device-id")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 DEVPATH=/devices/virtual/block/sda DEVNAME=sda SUBSYSTEM=block {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert added.status.exited_with(0), added.stderr
    assert fp"{dev_root}/disk/device-id".read_text()? == "b 1:3\n"

    conf.write(f"sda +{uid}:+{gid} 600 =disk/owner")
    let owned = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert owned.status.exited_with(0), owned.stderr
    assert fp"{dev_root}/disk/owner".read_text()? == "b 1:3\n"
  }
}

test test_mdev_accepts_host_word_boundary_patterns { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-host-regex")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/test"
  let conf = fp"{root}/mdev.conf"
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("SUBSYSTEM=test\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"\\bdev0\\b {uid}:{gid} 600 =alias")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 DEVPATH=/devices/test DEVNAME=dev0 SUBSYSTEM=test {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert added.status.exited_with(0), added.stderr
    assert fp"{dev_root}/alias".read_text()? == "c 1:3\n"
  }
}

test test_mdev_concurrent_alias_parent_creation_is_idempotent { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-concurrent-alias")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/test"
  let conf = fp"{root}/mdev.conf"
  dev_root.mkdir()
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("SUBSYSTEM=test\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  var alias_prefix = ""
  for index in range(60) {
    alias_prefix = if alias_prefix.is_empty() { f"d{index}" } else { f"{alias_prefix}/d{index}" }
  }
  conf.write(f"(dev[0-9]+) {uid}:{gid} 600 ={alias_prefix}/%1")
  let source = r"""proc main(...args: List[Str]) [process, error] -> Int {
  let executable = Path.parse_bytes(bytes.from_text(args[0]))?
  let script = Path.parse_bytes(bytes.from_text(args[1]))?
  let dev_root = bytes.from_text(args[2])
  let sysfs_root = bytes.from_text(args[3])
  let conf = bytes.from_text(args[4])
  var workers: List[ProcessHandle] = []
  for index in range(8) {
    var environment: Map[Str, Bytes] = {}
    environment = environment.set("XSH_MDEV_DEV_ROOT", dev_root)
    environment = environment.set("XSH_MDEV_SYSFS", sysfs_root)
    environment = environment.set("XSH_MDEV_CONF", conf)
    environment = environment.set("XSH_MDEV_TEST_PLAIN_FILES", b"1")
    environment = environment.set("ACTION", b"add")
    environment = environment.set("DEVPATH", b"/devices/test")
    environment = environment.set("DEVNAME", bytes.from_text(f"dev{index}"))
    environment = environment.set("SUBSYSTEM", b"test")
    let command = process.command_argv(executable, [executable, script, p"--"], env: environment)
    workers += [spawn command?]
  }
  for worker in workers {
    if let completed = process.wait_timeout([worker], 10s)? {
      if ! completed.status.exited_with(0) { return 1 }
    } else {
      return 2
    }
  }
  0
}
"""
  let ran = test.run_xsh(ctx, source, script_args: [ctx.xsh_bin.display(), f"{ctx.core_dir}/mdev.xsh", dev_root.display(), sysfs_root.display(), conf.display()])?
  assert ran.success, ran.stderr
  for index in range(8) {
    let node = fp"{dev_root}/{alias_prefix}/dev{index}"
    assert node.read_text()? == "c 1:3\n"
  }
}

test test_mdev_refuses_a_file_as_an_alias_parent { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-file-alias-parent")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/test"
  let conf = fp"{root}/mdev.conf"
  dev_root.mkdir()
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("SUBSYSTEM=test\n")
  fp"{dev_root}/blocked".write("keep")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"dev0 {uid}:{gid} 600 =blocked/dev0")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 DEVPATH=/devices/test DEVNAME=dev0 SUBSYSTEM=test {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert ! added.status.exited_with(0)
    assert fp"{dev_root}/blocked".read_text()? == "keep"
  }
}

test test_mdev_creates_alias_descendants_through_directory_symlinks { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-symlink-alias-parent")?
  let dev_root = fp"{root}/dev"
  let target_root = fp"{root}/directory-target"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/test"
  let conf = fp"{root}/mdev.conf"
  dev_root.mkdir()
  target_root.mkdir()
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("SUBSYSTEM=test\n")
  fp"{dev_root}/alias".symlink(to: target_root)
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"dev0 {uid}:{gid} 600 =alias/deep/%0")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 DEVPATH=/devices/test DEVNAME=dev0 SUBSYSTEM=test {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert added.status.exited_with(0), added.stderr
    assert fp"{dev_root}/alias".readlink()? == target_root
    assert fp"{target_root}/deep/dev0".read_text()? == "c 1:3\n"
  }
}

test test_mdev_alias_directories_inherit_setgid { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-setgid-alias-parent")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/test"
  let conf = fp"{root}/mdev.conf"
  dev_root.mkdir()
  dev_root.chmod(0o2755)
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("SUBSYSTEM=test\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"dev0 {uid}:{gid} 600 =one/two/three/%0")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 DEVPATH=/devices/test DEVNAME=dev0 SUBSYSTEM=test {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert added.status.exited_with(0), added.stderr
    assert fp"{dev_root}/one/two/three/dev0".read_text()? == "c 1:3\n"
  }

  for directory in [fp"{dev_root}/one", fp"{dev_root}/one/two", fp"{dev_root}/one/two/three"] {
    assert fs.stat(directory, follow_symlinks: true)?.mode.bit_and(0o2000) == 0o2000
  }
}

test test_mdev_scan_accepts_empty_roots { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-empty-scan")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let conf = fp"{root}/mdev.conf"
  dev_root.mkdir()
  fp"{sysfs_root}/dev".mkdir(parents: true)
  conf.write("")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 {
    let scanned = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- --scan
    assert scanned.status.exited_with(0), scanned.stderr
  }
}

test test_mdev_keep_matching_runs_rules_in_order { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-order")?
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
  conf.write(f"-SUBSYSTEM=block;(sd[a-z]) {uid}:{gid} 600 @printf '%s:' \"$MDEV\" >> \"$LOG\"\nSUBSYSTEM=block;(sd[a-z]) {uid}:{gid} 640 >disk/ @printf '%s' \"$MDEV\" >> \"$LOG\"")

  env XSH_MDEV_DEV_ROOT=$dev_root XSH_MDEV_SYSFS=$sysfs_root XSH_MDEV_CONF=$conf XSH_MDEV_TEST_PLAIN_FILES=1 DEVPATH=/devices/virtual/block/sda DEVNAME=sda SUBSYSTEM=block LOG=$log {
    let added = run.capture --text ACTION=add ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert added.status.exited_with(0), added.stderr
    assert log.read_text()? == "sda:disk/sda"
    assert fp"{dev_root}/disk/sda".read_text()? == "b 8:0\n"
    assert fp"{dev_root}/sda".readlink()? == p"disk/sda"
  }
}

test test_mdev_preserves_non_utf8_device_names { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-bytes")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/devices/test"
  let conf = fp"{root}/mdev.conf"
  device_dir.mkdir()
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("SUBSYSTEM=test\n")
  let uid = (run.text id -u).trim()
  let gid = (run.text id -g).trim()
  conf.write(f"@1,3 {uid}:{gid} 600")
  let source = r"""proc main(...args: List[Str]) [process, error] -> Int {
  let executable = Path.parse_bytes(bytes.from_text(args[0]))?
  let script = Path.parse_bytes(bytes.from_text(args[1]))?
  var environment: Map[Str, Bytes] = {}
  environment = environment.set("XSH_MDEV_DEV_ROOT", bytes.from_text(args[2]))
  environment = environment.set("XSH_MDEV_SYSFS", bytes.from_text(args[3]))
  environment = environment.set("XSH_MDEV_CONF", bytes.from_text(args[4]))
  environment = environment.set("XSH_MDEV_TEST_PLAIN_FILES", b"1")
  environment = environment.set("ACTION", b"add")
  environment = environment.set("DEVPATH", b"/devices/test")
  environment = environment.set("DEVNAME", b"mdev-\xff")
  environment = environment.set("SUBSYSTEM", b"test")
  let plan = process.command_argv(executable, [executable, script, p"--"], env: environment)
  let status = process.run(plan)?
  if status.exited_with(0) { 0 } else { 1 }
}
"""
  let ran = test.run_xsh(ctx, source, script_args: [ctx.xsh_bin.display(), f"{ctx.core_dir}/mdev.xsh", dev_root.display(), sysfs_root.display(), conf.display()])?
  assert ran.success, ran.stderr
  let node = Path.parse_bytes(bytes.concat([dev_root.bytes(), b"/mdev-\xff"]))?
  assert node.read_text()? == "c 1:3\n"
}

test test_mdev_rejects_invalid_arguments_and_configuration { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-errors")?
  let conf = fp"{root}/mdev.conf"
  conf.write("sda 0 660\n")

  env XSH_MDEV_CONF=$conf ACTION=add DEVPATH=/devices/test {
    let option = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- -z
    assert ! option.status.exited_with(0)
    assert "invalid option -- 'z'" in option.stderr

    let foreground = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- -f
    assert ! foreground.status.exited_with(0)
    assert "foreground mode requires daemon mode" in foreground.stderr

    let configuration = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --
    assert ! configuration.status.exited_with(0)
    assert "mdev.conf:1: uid:gid field is required" in configuration.stderr
  }
}

test test_mdev_daemon_returns_worker_setup_failure { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mdev is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-daemon-error")?
  let conf = fp"{root}/mdev.conf"
  conf.write("sda 0 660\n")

  env XSH_MDEV_CONF=$conf {
    let started = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" -- --daemon
    assert ! started.status.exited_with(0), started.stderr
    assert "mdev.conf:1: uid:gid field is required" in started.stderr
  }
}

test test_mdev_foreground_daemon_reloads_and_terminates { |ctx|
  guard system.uname()?.sysname == "Linux" and unix.id()?.euid == 0 else {
    test.skip("foreground mdev daemon requires Linux root privileges")
    return
  }

  let root = test.temp_dir(ctx, name: "mdev-foreground")?
  let dev_root = fp"{root}/dev"
  let sysfs_root = fp"{root}/sys"
  let device_dir = fp"{sysfs_root}/dev/virtual/misc/testdev"
  let conf = fp"{root}/mdev.conf"
  let marker = fp"{root}/scan-complete"
  let errors = fp"{root}/mdev.err"
  dev_root.mkdir()
  device_dir.mkdir(parents: true)
  fp"{device_dir}/dev".write("1:3\n")
  fp"{device_dir}/uevent".write("DEVNAME=testdev\nSUBSYSTEM=misc\n")
  conf.write("testdev 0:0 600 @printf scanned > \"$MARKER\"\n")

  var environment: Map[Str, Bytes] = {}
  environment = environment.set("XSH_MDEV_DEV_ROOT", dev_root.bytes())
  environment = environment.set("XSH_MDEV_SYSFS", sysfs_root.bytes())
  environment = environment.set("XSH_MDEV_CONF", conf.bytes())
  environment = environment.set("XSH_MDEV_TEST_PLAIN_FILES", b"1")
  environment = environment.set("MARKER", marker.bytes())
  let script = fp"{ctx.core_dir}/mdev.xsh"
  let command = process.command_argv(
    ctx.xsh_bin,
    [ctx.xsh_bin, script, p"--", p"--daemon", p"--foreground"],
    env: environment,
    stdout: p"/dev/null",
    stderr: errors,
  )
  let daemon = spawn command?
  defer {
    if let Ok(_) = process.kill(daemon.pid, signal: "0") {
      daemon.cancel(signal: "KILL", kill_after: 0ms)
    }
  }

  let scan_started = time.now()
  loop {
    if marker.exists()? { break }
    if let completed = process.wait_timeout([daemon], 0ms)? {
      test.fail(f"foreground daemon exited before scan completed: {completed.status.shell_code()?}; {errors.read_text()?}")
    }
    if time.now() - scan_started > 5000 {
      test.fail(f"foreground daemon did not complete its initial scan: {errors.read_text()?}")
    }
    time.sleep(10ms)
  }
  assert marker.read_text()? == "scanned"

  conf.write("testdev 0 600\n")
  process.kill(daemon.pid, signal: "HUP")
  let reload_started = time.now()
  loop {
    if "mdev.conf:1: uid:gid field is required" in errors.read_text()? { break }
    if let completed = process.wait_timeout([daemon], 0ms)? {
      test.fail(f"foreground daemon exited while reloading: {completed.status.shell_code()?}; {errors.read_text()?}")
    }
    if time.now() - reload_started > 5000 {
      test.fail(f"foreground daemon did not report its failed reload: {errors.read_text()?}")
    }
    time.sleep(10ms)
  }
  process.kill(daemon.pid, signal: "0")?
  process.kill(daemon.pid, signal: "TERM")
  guard let completed = process.wait_timeout([daemon], 3s)? else {
    test.fail("foreground daemon did not stop after TERM")
    return
  }
  assert completed.status.exited_with(0)
}
