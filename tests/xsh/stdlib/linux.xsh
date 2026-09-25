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
    let interfaces = linux.interfaces()?.collect()
    test.eq(interfaces[0].name, "eth0")?
    test.eq(interfaces[0].addresses[0].family, "inet")?
    let routes = linux.routes()?.collect()
    test.eq(routes[0].dst, "default")?
    test.eq(routes[0].gateway, "192.0.2.1")?
    test.ok(linux.meminfo()?.total > 0)?
    test.eq(linux.modules()?.collect()[0].name, "xsh_demo")?
    test.contains(linux.dmesg()?.collect()[0], "xsh")?
    test.ok(linux.is_mountpoint(/proc)?)?
    test.eq(linux.disk_usage(/)?.collect()[0].device, "rootfs")?
    test.eq(linux.block_devices()?.collect()[0].name, "vda")?
    let sysctl_value = linux.sysctl_get("kernel.pid_max")?
    test.eq(sysctl_value, "1")?
    linux.sysctl_set("kernel.pid_max", sysctl_value)?
    let attrs = linux.file_attrs(seed)?
    test.ok(attrs.immutable and attrs.append_only)?
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
    let rfkill = linux.rfkill_list()?.collect()
    test.eq(rfkill[0].type, "wlan")?
    linux.rfkill_block(rfkill[0].id)?
    linux.rfkill_unblock(rfkill[0].id)?
    let loop_device = linux.loop_attach(seed)?
    linux.loop_detach(loop_device)?
    test.eq(linux.loop_list()?.collect()[0].device, loop_device)?
    linux.mkswap(seed)?
    linux.swapon(seed, priority: 1)?
    linux.swapoff(seed)?
    test.eq(linux.blkid(seed)?.type, "ext4")?
    test.eq(linux.modinfo("demo")?.params[0].name, "debug")?
    linux.modprobe("demo", params: "debug=1")?
    linux.depmod("dry-run")?
    test.eq(linux.open_files(123)?.collect()[0].type, "file")?
    let table = linux.partition_table(seed)?
    test.eq(table.partitions[0].name, "root")?
    linux.write_partition_table(seed, table)?
    test.eq(linux.fsck(seed, fstype: "ext4")?.status, 0)?
    let uevents = linux.uevent_stream()?

    for event in uevents {
      test.eq(event.action, "add")?
      test.eq(event.subsystem, "block")?
      test.eq(event.env[0].name, "ACTION")?
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
  test.contains(log_text, "\"op\":\"mount\"")?
  test.contains(log_text, "\"op\":\"meminfo\"")?
  test.contains(log_text, "\"op\":\"routes\"")?
  test.contains(log_text, "\"op\":\"set_ipv4_address\"")?
  test.contains(log_text, "\"op\":\"add_default_ipv4_route\"")?
  test.contains(log_text, "\"op\":\"is_mountpoint\"")?
  test.contains(log_text, "\"op\":\"disk_usage\"")?
  test.contains(log_text, "\"op\":\"sysctl_set\"")?
  test.contains(log_text, "\"op\":\"set_file_attrs\"")?
  test.contains(log_text, "\"op\":\"mknod\"")?
  test.contains(log_text, "\"op\":\"loop_attach\"")?
  test.contains(log_text, "\"op\":\"swapon\"")?
  test.contains(log_text, "\"op\":\"modprobe\"")?
  test.contains(log_text, "\"op\":\"write_partition_table\"")?
  test.contains(log_text, "\"op\":\"uevent_stream\"")?
  test.contains(log_text, "\"signal\":\"TERM\"")?
  test.contains(log_text, "\"except_pid1\":\"true\"")?
  test.contains(log_text, "\"op\":\"link_down\"")?
  test.contains(log_text, "\"op\":\"flush_ipv4_addresses\"")?
  test.contains(log_text, "\"op\":\"del_default_ipv4_route\"")?
  test.contains(log_text, "\"op\":\"dhcp_send_release\"")?
  test.contains(log_text, "\"op\":\"read_device\"")?
  test.contains(log_text, "\"op\":\"kill_all\"")?
  test.contains(log_text, "\"op\":\"poweroff\"")?
  test.contains(log_text, "\"op\":\"reboot\"")?
}

# The message a failed `linux.meminfo` call reports.
#
# The kind is asserted with `test.error_kind`; this exposes the text naming the
# field the failure is about. The result type is spelled out because the entry
# reports a record on success.
pure meminfo_failure(result: Result[LinuxMemInfo]) -> Str {
  match result {
    Ok(_) => return ""
    Err(failure) => return failure.message
  }
}

proc test_linux_text_entries_require_a_gate() [process, env, error] {
  # Both variables are emptied here so the test does not depend on the
  # environment it runs in. Neither empties to an accepted true value, so both
  # entries refuse before they open any host file, and the refusal names the
  # variables that would open a gate.
  env XSH_LINUX_DRY_RUN="" XSH_LINUX_REAL="" {
    test.error_kind(linux.meminfo(), "linux-unimplemented")?
    test.error_kind(linux.modules(), "linux-unimplemented")?
    test.contains(meminfo_failure(linux.meminfo()), "XSH_LINUX_REAL=1")?
  } ?
}

proc test_linux_halt_requires_an_explicit_mode(ctx: TestContext) [error] {
  let output = test.run_script(
    ctx,
    "let _ = linux.halt()?\n",
    [],
    {XSH_LINUX_DRY_RUN: "", XSH_LINUX_REAL: ""},
  )?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "linux-unimplemented")?
}

proc test_linux_text_dry_run_values_and_log(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "linux-text-dry-run")?
  let log = fp"${root}/linux.jsonl"

  # The entries report fixed values while the dry-run gate is open —
  # even with the real gate also open — and each call appends one line naming
  # its operation to the log file.
  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN_LOG=$log {
    let memory = linux.meminfo()?
    test.eq(memory.total, 1024 * 1024 * 1024)?
    test.eq(memory.free, 256 * 1024 * 1024)?
    test.eq(memory.available, 512 * 1024 * 1024)?
    test.eq(memory.buffers, 64 * 1024 * 1024)?
    test.eq(memory.cached, 128 * 1024 * 1024)?
    test.eq(memory.swap_total, 512 * 1024 * 1024)?
    test.eq(memory.swap_free, 384 * 1024 * 1024)?

    let modules = linux.modules()?.collect()
    test.eq(modules.len(), 1)?
    test.eq(modules[0].name, "xsh_demo")?
    test.eq(modules[0].size, 4096)?
    test.eq(modules[0].used_by, ["xsh_dep"])?
  } ?

  test.eq(
    log.read_text()?,
    """{"op":"meminfo"}
{"op":"modules"}
""",
  )?
}

proc test_linux_dry_run_disk_usage_and_sysctl_records() [process, env, error] {
  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_SYSCTL_VALUE=65535 {
    let root_usage = linux.disk_usage()?.collect()
    let tmp_usage = linux.disk_usage(/tmp)?.collect()
    test.eq(root_usage[0].device, "rootfs")?
    test.eq(root_usage[0].mount, "/")?
    test.eq(root_usage[0].fstype, "tmpfs")?
    test.eq(root_usage[0].total, 1073741824)?
    test.eq(root_usage[0].used, 268435456)?
    test.eq(root_usage[0].available, 805306368)?
    test.eq(tmp_usage[0].mount, "/tmp")?
    test.ok(linux.is_mountpoint(/proc)?)?
    test.ok(! linux.is_mountpoint(/tmp)?)?
    test.eq(linux.sysctl_get("kernel.pid_max")?, "65535")?
  } ?
}

proc test_linux_dry_run_file_attrs_decode_seed_flags(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "linux-file-attrs")?
  let seed = fp"${root}/seed"
  fs.write(seed, "seed")?
  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_FILE_ATTRS_FLAGS=250111 XSH_LINUX_FILE_VERSION=7 {
    let attrs = linux.file_attrs(seed)?
    let version = linux.file_version(seed)?
    linux.set_file_attrs(seed, attrs.flags)?
    linux.set_file_version(seed, version)?
    test.eq(attrs.flags, 250111)?
    test.eq(version, 7)?
    test.ok(attrs.indexed_directory)?
    test.ok(attrs.secure_deletion)?
    test.ok(attrs.undelete)?
    test.ok(attrs.sync)?
    test.ok(attrs.dirsync)?
    test.ok(attrs.immutable)?
    test.ok(attrs.append_only)?
    test.ok(attrs.no_dump)?
    test.ok(attrs.no_atime)?
    test.ok(attrs.compression_requested)?
    test.ok(attrs.journaled_data)?
    test.ok(attrs.no_tailmerging)?
    test.ok(attrs.top_of_directory_hierarchies)?
  } ?
}

proc test_linux_dry_run_rejects_invalid_seed_inputs() [process, env, error] {
  env XSH_LINUX_DRY_RUN=1 {
    match linux.sysctl_get("kernel..pid_max") {
      Ok(_) => { test.ok(false, "invalid sysctl name was accepted")? }
      Err(failure) => {
        test.error_kind(failure, "linux-sysctl")?
        test.contains(failure.message, "invalid")?
      }
    }
    match linux.sysctl_set("../kernel.pid_max", "1") {
      Ok(_) => { test.ok(false, "invalid sysctl path was accepted")? }
      Err(failure) => {
        test.error_kind(failure, "linux-sysctl")?
        test.contains(failure.message, "invalid")?
      }
    }
    for flags in [-1, 4294967296] {
      match linux.set_file_attrs(/tmp/file, flags) {
        Ok(_) => { test.ok(false, "invalid file attribute flags were accepted")? }
        Err(failure) => {
          test.error_kind(failure, "linux-file-attrs")?
          test.contains(failure.message, "between 0 and 4294967295")?
        }
      }
    }
    match linux.set_file_version(/tmp/file, -1) {
      Ok(_) => { test.ok(false, "invalid file version was accepted")? }
      Err(failure) => {
        test.error_kind(failure, "linux-file-version")?
        test.contains(failure.message, "between 0 and 4294967295")?
      }
    }
    test.error_kind(linux.kill_all(signal: "BOGUS"), "invalid-signal")?
    match linux.mknod(/tmp/file, "socket", 0, 0) {
      Ok(_) => { test.ok(false, "invalid node kind was accepted")? }
      Err(failure) => {
        test.error_kind(failure, "linux-mknod")?
        test.contains(failure.message, "block")?
      }
    }
  } ?
}

proc test_linux_dry_run_log_appends_in_place(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    # This checks Linux's in-place append behavior.
    test.skip("the dry-run log is appended on Linux only")
    return
  }

  let root = test.temp_dir(ctx, name: "linux-log-append")?
  let log = fp"${root}/linux.jsonl"

  # The log already holds bytes that are not valid UTF-8, with no trailing
  # newline. The baseline appends to the open file, so those bytes have to
  # survive; a read-concatenate-rewrite log would lose or replace them.
  let seeded = bytes.from_ints([255, 254, 10])?
  fs.write(log, seeded)?

  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$log {
    let _ = linux.meminfo()?
  } ?
  let once = log.read_bytes()?
  test.ok(once.starts_with(seeded))?
  test.ok(once.len() > seeded.len())?

  # A second call appends exactly one more record: the file grows by the same
  # number of bytes again and nothing before it is truncated.
  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$log {
    let _ = linux.meminfo()?
  } ?
  let twice = log.read_bytes()?
  test.ok(twice.starts_with(seeded))?
  test.eq(twice.len() - once.len(), once.len() - seeded.len())?

  # A destination whose parent directories do not exist yet is created.
  let fresh = fp"${root}/missing/deeper/linux.jsonl"
  env XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$fresh {
    let _ = linux.meminfo()?
  } ?
  test.ok(fresh.exists()?)?
  test.contains(fresh.read_text() ?? "", "\"op\":\"meminfo\"")?

  # A directory destination raises the native logging error.
  let blocked = fp"${root}/a-directory"
  fs.mkdir(blocked)?
  let failed = test.run_script(
    ctx,
    "linux.meminfo()?",
    args: [],
    env: {
      XSH_LINUX_DRY_RUN: "1",
      XSH_LINUX_DRY_RUN_LOG: blocked.display(),
    },
  )?
  test.ok(!failed.success)?
  test.contains(failed.stderr, "linux-dry-run-log")?
  test.eq(blocked.metadata()?.kind, "dir")?
}

proc test_linux_text_log_failure_kind(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    test.skip("Linux dry-run logging is tested on Linux only")
    return
  }

  let root = test.temp_dir(ctx, name: "linux-text-log")?
  let blocked = fp"${root}/file"
  fs.write(blocked, "not a directory")?
  let blocked_log = fp"${blocked}/linux.jsonl"
  let meminfo_failed = test.run_script(
    ctx,
    "linux.meminfo()?",
    args: [],
    env: {
      XSH_LINUX_DRY_RUN: "1",
      XSH_LINUX_DRY_RUN_LOG: blocked_log.display(),
    },
  )?
  test.ok(!meminfo_failed.success)?
  test.contains(meminfo_failed.stderr, "linux-dry-run-log")?

  # The retained native modules arm raises a dry-run log failure. A nested
  # script observes that process boundary without losing the failure kind.
  let failed = test.run_script(
    ctx,
    "linux.modules()?",
    args: [],
    env: {
      XSH_LINUX_DRY_RUN: "1",
      XSH_LINUX_DRY_RUN_LOG: blocked_log.display(),
    },
  )?
  test.ok(!failed.success)?
  test.contains(failed.stderr, "linux-dry-run-log")?
}

proc test_linux_meminfo_reads_the_host_text() [process, env, error] {
  if system.uname()?.sysname != "Linux" {
    # The entry reads `/proc/meminfo` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("linux.meminfo reads /proc/meminfo on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than values a host could change: every reported count is a
  # kilobyte count scaled to bytes, and a free or available count is part of the
  # total it is reported next to.
  env XSH_LINUX_REAL=1 {
    let memory = linux.meminfo()?
    test.ok(memory.total > 0)?
    test.eq(memory.total % 1024, 0)?
    test.ok(memory.free >= 0 and memory.free <= memory.total)?
    test.ok(memory.available >= 0 and memory.available <= memory.total)?
    test.ok(memory.buffers >= 0 and memory.cached >= 0)?
    test.ok(memory.swap_free >= 0 and memory.swap_free <= memory.swap_total)?
  } ?
}

proc test_linux_modules_streams_the_host_text() [process, env, error] {
  if system.uname()?.sysname != "Linux" {
    # The entry reads `/proc/modules` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("linux.modules reads /proc/modules on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than the modules a host could load. A consumer that stops
  # after one record reads only that record; a host that reports no module
  # leaves the loop empty rather than failing.
  env XSH_LINUX_REAL=1 {
    let records = linux.modules()?
    for entry in records {
      test.ok(entry.name != "")?
      test.ok(entry.size >= 0)?
      for dependent in entry.used_by {
        test.ok(dependent != "")?
      }

      break
    }
  } ?

  # The whole text interprets into records with the same shape.
  env XSH_LINUX_REAL=1 {
    for entry in linux.modules()?.collect() {
      test.ok(entry.name != "")?
      test.ok(entry.size >= 0)?
      for dependent in entry.used_by {
        test.ok(dependent != "")?
      }
    }
  } ?
}

# The kernel-module policy the entry now implements: the query normalizes like
# the scan does, the presentation keeps the first value of each field and every
# `parm` value, and `depmod` writes one line per module with the dependencies
# the tree actually holds.
#
# The nested run is what makes the fixture reach the entry: `XSH_MODULES_DIR`
# selects the tree, and it is read from the *process* environment by the
# retained scan, so a scoped `env` block around the call would not be visible
# to it.
proc test_linux_module_policy_uses_the_configured_tree(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    # On other platforms the binding is still native, so there is nothing to
    # add here.
    test.skip("linux.modinfo reads a module tree on Linux only")
    return
  }

  let root = test.temp_dir(ctx, name: "linux-modules")?
  fs.write(
    fp"${root}/demo-name.ko",
    "description=Demo module\0license=MIT\0version=2\0depends=dep,missing\0parm=debug:Enable debug (bool)\0parm=mode:Mode (charp)\0",
  )?
  fs.write(
    fp"${root}/dep.ko",
    "description=dep module\0license=GPL\0version=1\0",
  )?

  # The nested source is a template rather than an f-string: its `${...}`
  # interpolations belong to the nested script, so the outer checker must not
  # resolve them, and only the tree's path is substituted.
  let template = r"""
proc main() [io, fs, error] {
  let info = linux.modinfo("demo-name")?
  print f"${info.name}|${info.description}|${info.license}|${info.version}"
  for param in info.params {
    print f"param=${param.name}|${param.type}|${param.description}"
  }
  print f"explicit=${linux.modinfo(p"{root}/demo-name.ko".display())?.name}"
  match linux.modinfo("nothing-here") {
    Ok(_) => { print "unexpected" }
    Err(failure) => { print f"missing=${failure.message}" }
  }
  linux.depmod("")?
  print fs.read_text(p"{root}/modules.dep")?
}
"""
  let source = template.replace("{root}", f"${root}")

  let environment = {XSH_LINUX_REAL: "1", XSH_MODULES_DIR: f"${root}"}

  # Arguments are positional-only, so the empty argv and stdin are spelled out.
  let nested = test.run_script(
    ctx,
    source,
    [],
    environment,
    b"",
    "linux-module-policy",
  )?
  test.ok(nested.success, nested.stderr)?
  test.eq(
    nested.stdout,
    f"""demo_name|Demo module|MIT|2
param=debug|bool|Enable debug
param=mode|charp|Mode
explicit=demo_name
missing=module not found
demo-name.ko: dep.ko
dep.ko:

""",
  )?
}
