proc test_linux_dry_run_covers_module_surface(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "linux")?
  let log = fp"${root}/linux.jsonl"
  let seed = fp"${root}/seed"
  let random = fp"${root}/random"
  fs.write(seed, "seed")?

  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_SIGNAL=USR2 XSH_LINUX_DRY_RUN_LOG=$log {
    linux.write_device(/dev/urandom, seed)?
    linux.read_device(/dev/urandom, random, bytes: 4)?
    linux.mount("proc", /proc, fstype: "proc", options: ["nosuid"])?
    linux.mount_all()?
    linux.umount_all(types: ["tmpfs"])?
    linux.swapon_all()?
    linux.swapoff_all()?
    test.eq(linux.root_device()?, "rootfs")?
    linux.link_up("lo")?
    linux.link_down("eth0")?
    linux.set_ipv4_address("eth0", "192.0.2.10", "255.255.255.0")?
    linux.flush_ipv4_addresses("eth0")?
    linux.add_default_ipv4_route("192.0.2.1", interface: "eth0")?
    linux.del_default_ipv4_route("192.0.2.1", interface: "eth0")?
    linux.dhcp_send_release("eth0", "192.0.2.10", "192.0.2.1")?
    test.eq(linux.interfaces()?[0].name, "eth0")?
    test.eq(linux.routes()?[0].gateway, "192.0.2.1")?
    test.ok(linux.meminfo()?.total > 0)?
    test.eq(linux.modules()?[0].name, "xsh_demo")?
    test.contains(linux.dmesg()?[0], "xsh")?
    test.ok(linux.is_mountpoint(/proc)?)?
    test.eq(linux.disk_usage(/)?[0].device, "rootfs")?
    test.eq(linux.block_devices()?[0].name, "vda")?
    let sysctl_value = linux.sysctl_get("kernel.pid_max")?
    linux.sysctl_set("kernel.pid_max", sysctl_value)?
    let attrs = linux.file_attrs(seed)?
    linux.set_file_attrs(seed, attrs.flags)?
    let version = linux.file_version(seed)?
    linux.set_file_version(seed, version)?
    linux.chroot(root)?
    linux.mknod(fp"${root}/null", "char", 1, 3)?
    linux.insmod(fp"${root}/demo.ko", params: "debug=1")?
    linux.rmmod("demo", force: true)?
    linux.pivot_root(root, fp"${root}/oldroot")?
    linux.switch_root(root, /sbin/init)?
    let epoch_ms = linux.hwclock()?
    linux.set_hwclock(epoch_ms)?
    linux.set_system_clock(epoch_ms)?
    let rfkill = linux.rfkill_list()?
    linux.rfkill_block(rfkill[0].id)?
    linux.rfkill_unblock(rfkill[0].id)?
    let loop_device = linux.loop_attach(seed)?
    linux.loop_detach(loop_device)?
    test.eq(linux.loop_list()?[0].device, loop_device)?
    linux.mkswap(seed)?
    linux.swapon(seed, priority: 1)?
    linux.swapoff(seed)?
    test.eq(linux.blkid(seed)?.type, "ext4")?
    test.eq(linux.modinfo("demo")?.params[0].name, "debug")?
    linux.modprobe("demo", params: "debug=1")?
    linux.depmod("dry-run")?
    test.eq(linux.open_files(123)?[0].type, "file")?
    let table = linux.partition_table(seed)?
    test.eq(table.partitions[0].name, "root")?
    linux.write_partition_table(seed, table)?
    test.eq(linux.fsck(seed, fstype: "ext4")?.status, 0)?
    let uevents = linux.uevent_stream()?

    for event in uevents {
      test.eq(event.action, "add")?
      test.eq(event.subsystem, "block")?
      break
    }

    linux.sysctl_load_dirs([/etc/sysctl.d], fallback: /etc/sysctl.conf)?
    linux.kill_all(signal: "TERM", except_pid1: true)?
    linux.halt()?
    linux.poweroff()?
    linux.reboot()?
  } ?

  test.eq(random.read_bytes()?, b"\0\0\0\0")?
  let log_text = log.read_text()?
  test.contains(log_text, "\"op\":\"link_down\"")?
  test.contains(log_text, "\"op\":\"flush_ipv4_addresses\"")?
  test.contains(log_text, "\"op\":\"del_default_ipv4_route\"")?
  test.contains(log_text, "\"op\":\"dhcp_send_release\"")?
  test.contains(log_text, "\"op\":\"read_device\"")?
  test.contains(log_text, "\"op\":\"kill_all\"")?
  test.contains(log_text, "\"op\":\"poweroff\"")?
  test.contains(log_text, "\"op\":\"reboot\"")?
}
