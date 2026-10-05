test test_linux_fake_covers_module_surface { |ctx|
  let root = test.temp_dir(ctx, name: "linux")?
  let log = fp"{root}/linux.jsonl"
  let seed = fp"{root}/seed"
  let random = fp"{root}/random"
  seed.write("seed")

  test.linux_fake(ctx, {log: log})
  linux.write_device(/dev/urandom, seed)
  linux.read_device(/dev/urandom, random, bytes: 4)
  linux.mount("proc", /proc, fstype: "proc", options: ["nosuid"])
  linux.mount_all()
  linux.umount_all(types: ["tmpfs"])
  linux.swapon_all()
  linux.swapoff_all()
  assert linux.root_device()? == "rootfs"
  linux.link_up("lo")
  linux.link_down("eth0")
  linux.set_ipv4_address("eth0", "192.0.2.10", "255.255.255.0")
  linux.flush_ipv4_addresses("eth0")
  linux.add_default_ipv4_route("192.0.2.1", interface: "eth0")
  linux.del_default_ipv4_route("192.0.2.1", interface: "eth0")
  linux.dhcp_send_release("eth0", "192.0.2.10", "192.0.2.1")
  let interfaces = linux.interfaces()?.collect()
  assert interfaces[0].name == "eth0"
  assert interfaces[0].addresses[0].family == "inet"
  let routes = linux.routes()?.collect()
  assert routes[0].dst == "default"
  assert routes[0].gateway == "192.0.2.1"
  let network = linux.network_dump()?
  assert network.state == "complete"
  assert network.links[0].name == "eth0"
  assert network.addresses[0].address == "192.0.2.10"
  assert network.routes[0].table == 254
  assert network.rules[0].priority == 32766
  assert linux.meminfo()?.total > 0
  assert linux.modules()?.collect()[0].name == "xsh_demo"
  assert "xsh" in linux.dmesg()?.collect()[0]
  assert linux.is_mountpoint(/proc)?
  assert linux.disk_usage(/)?.collect()[0].device == "rootfs"
  assert linux.block_devices()?.collect()[0].name == "vda"
  let sysctl_value = linux.sysctl_get("kernel.pid_max")?
  assert sysctl_value == "1"
  linux.sysctl_set("kernel.pid_max", sysctl_value)
  let attrs = linux.file_attrs(seed)?
  assert attrs.immutable and attrs.append_only
  linux.set_file_attrs(seed, attrs.flags)
  let version = linux.file_version(seed)?
  linux.set_file_version(seed, version)
  linux.chroot(root)
  linux.mknod(fp"{root}/null", "char", 1, 3)
  linux.insmod(fp"{root}/demo.ko", params: "debug=1")
  linux.rmmod("demo", force: true)
  linux.pivot_root(root, fp"{root}/oldroot")
  linux.switch_root(root, /sbin/init)
  let epoch_ms = linux.hwclock()?
  linux.set_hwclock(epoch_ms)
  linux.set_system_clock(epoch_ms)
  let rfkill = linux.rfkill_list()?.collect()
  assert rfkill[0].type == "wlan"
  linux.rfkill_block(rfkill[0].id)
  linux.rfkill_unblock(rfkill[0].id)
  let loop_device = linux.loop_attach(seed)?
  linux.loop_detach(loop_device)
  assert linux.loop_list()?.collect()[0].device == loop_device
  linux.mkswap(seed)
  linux.swapon(seed, priority: 1)
  linux.swapoff(seed)
  assert linux.blkid(seed)?.type == "ext4"
  assert linux.modinfo("demo")?.params[0].name == "debug"
  linux.modprobe("demo", params: "debug=1")
  linux.depmod("dry-run")
  assert linux.open_files(123)?.collect()[0].type == "file"
  let table = linux.partition_table(seed)?
  assert table.partitions[0].name == "root"
  linux.write_partition_table(seed, table)
  assert linux.fsck(seed, fstype: "ext4")?.status == 0
  let uevents = linux.uevent_stream()?

  for event in uevents {
    assert event.action == "add"
    assert event.subsystem == "block"
    assert event.env[0].name == "ACTION"
    break
  }

  linux.sysctl_load_dirs([/etc/sysctl.d], fallback: /etc/sysctl.conf)
  linux.kill_all(signal: "TERM", except_pid1: true)
  linux.halt()
  linux.poweroff()
  linux.reboot()

  assert random.read_bytes()? == b"\0\0\0\0"
  let log_text = log.read_text()?
  assert "\"op\":\"mount\"" in log_text
  assert "\"op\":\"meminfo\"" in log_text
  assert "\"op\":\"routes\"" in log_text
  assert "\"op\":\"set_ipv4_address\"" in log_text
  assert "\"op\":\"add_default_ipv4_route\"" in log_text
  assert "\"op\":\"is_mountpoint\"" in log_text
  assert "\"op\":\"disk_usage\"" in log_text
  assert "\"op\":\"sysctl_set\"" in log_text
  assert "\"op\":\"set_file_attrs\"" in log_text
  assert "\"op\":\"mknod\"" in log_text
  assert "\"op\":\"loop_attach\"" in log_text
  assert "\"op\":\"swapon\"" in log_text
  assert "\"op\":\"modprobe\"" in log_text
  assert "\"op\":\"write_partition_table\"" in log_text
  assert "\"op\":\"uevent_stream\"" in log_text
  assert "\"signal\":\"TERM\"" in log_text
  assert "\"except_pid1\":\"true\"" in log_text
  assert "\"op\":\"link_down\"" in log_text
  assert "\"op\":\"flush_ipv4_addresses\"" in log_text
  assert "\"op\":\"del_default_ipv4_route\"" in log_text
  assert "\"op\":\"dhcp_send_release\"" in log_text
  assert "\"op\":\"read_device\"" in log_text
  assert "\"op\":\"kill_all\"" in log_text
  assert "\"op\":\"poweroff\"" in log_text
  assert "\"op\":\"reboot\"" in log_text
}

test test_linux_entries_reach_the_host_without_a_gate {
  # No environment variable or flag stands between a script and the host: on
  # Linux the entries run for real, and elsewhere they report the platform.
  # The destructive probe targets an interface that does not exist, so it
  # fails on every host without changing anything.
  if system.uname()?.sysname == "Linux" {
    assert linux.meminfo()?.total > 0
    let interfaces = linux.interfaces()? |> collect()
    assert interfaces |> any .name == "lo"
    assert linux.link_up("xsh-absent0") is Err(_)
  } else {
    test.error_kind(linux.meminfo(), "linux-unsupported")
    test.error_kind(linux.interfaces(), "linux-unsupported")
    test.error_kind(linux.link_up("xsh-absent0"), "linux-unsupported")
    test.error_kind(linux.halt(), "linux-unsupported")
  }
}

test test_linux_fake_rejects_unknown_settings { |ctx|
  test.error_kind(test.linux_fake(ctx, {dry_run: "1"}), "test-linux-fake")
  test.error_kind(test.linux_fake(ctx, {log: true}), "test-linux-fake")
}

test test_linux_fake_text_values_and_log { |ctx|
  let root = test.temp_dir(ctx, name: "linux-fake-text")?
  let log = fp"{root}/linux.jsonl"

  # The entries report fixed values while the fake is installed, and each call
  # appends one line naming its operation to the log file.
  test.linux_fake(ctx, {log: log})
  let memory = linux.meminfo()?
  assert memory.total == 1GiB
  assert memory.free == 256MiB
  assert memory.available == 512MiB
  assert memory.buffers == 64MiB
  assert memory.cached == 128MiB
  assert memory.swap_total == 512MiB
  assert memory.swap_free == 384MiB

  let modules = linux.modules()?.collect()
  assert modules.len() == 1
  assert modules[0].name == "xsh_demo"
  assert modules[0].size == 4096
  assert modules[0].used_by == ["xsh_dep"]

  assert log.read_text()? == """{"op":"meminfo"}
{"op":"modules"}
"""
}

test test_linux_fake_disk_usage_and_sysctl_records { |ctx|
  test.linux_fake(ctx, {sysctl_value: "65535"})
  let root_usage = linux.disk_usage()?.collect()
  let tmp_usage = linux.disk_usage(/tmp)?.collect()
  assert root_usage[0].device == "rootfs"
  assert root_usage[0].mount == "/"
  assert root_usage[0].fstype == "tmpfs"
  assert root_usage[0].total == 1073741824
  assert root_usage[0].used == 268435456
  assert root_usage[0].available == 805306368
  assert tmp_usage[0].mount == "/tmp"
  assert linux.is_mountpoint(/proc)?
  assert ! linux.is_mountpoint(/tmp)?
  assert linux.sysctl_get("kernel.pid_max")? == "65535"
}

test test_linux_fake_file_attrs_decode_seed_flags { |ctx|
  let root = test.temp_dir(ctx, name: "linux-file-attrs")?
  let seed = fp"{root}/seed"
  seed.write("seed")
  test.linux_fake(ctx, {file_attrs_flags: 250111, file_version: 7})
  let attrs = linux.file_attrs(seed)?
  let version = linux.file_version(seed)?
  linux.set_file_attrs(seed, attrs.flags)
  linux.set_file_version(seed, version)
  assert attrs.flags == 250111
  assert version == 7
  assert attrs.indexed_directory
  assert attrs.secure_deletion
  assert attrs.undelete
  assert attrs.sync
  assert attrs.dirsync
  assert attrs.immutable
  assert attrs.append_only
  assert attrs.no_dump
  assert attrs.no_atime
  assert attrs.compression_requested
  assert attrs.journaled_data
  assert attrs.no_tailmerging
  assert attrs.top_of_directory_hierarchies
}

test test_linux_fake_rejects_invalid_seed_inputs { |ctx|
  test.linux_fake(ctx)
  match linux.sysctl_get("kernel..pid_max") {
    Ok(_) => assert false, "invalid sysctl name was accepted"
    Err(failure) => {
      test.error_kind(failure, "linux-sysctl")
      assert "invalid" in failure.message
    }
  }

  match linux.sysctl_set("../kernel.pid_max", "1") {
    Ok(_) => assert false, "invalid sysctl path was accepted"
    Err(failure) => {
      test.error_kind(failure, "linux-sysctl")
      assert "invalid" in failure.message
    }
  }

  for flags in [-1, 4294967296] {
    match linux.set_file_attrs(/tmp/file, flags) {
      Ok(_) => assert false, "invalid file attribute flags were accepted"
      Err(failure) => {
        test.error_kind(failure, "linux-file-attrs")
        assert "between 0 and 4294967295" in failure.message
      }
    }
  }

  match linux.set_file_version(/tmp/file, -1) {
    Ok(_) => assert false, "invalid file version was accepted"
    Err(failure) => {
      test.error_kind(failure, "linux-file-version")
      assert "between 0 and 4294967295" in failure.message
    }
  }

  test.error_kind(linux.kill_all(signal: "BOGUS"), "invalid-signal")
  match linux.mknod(/tmp/file, "socket", 0, 0) {
    Ok(_) => assert false, "invalid node kind was accepted"
    Err(failure) => {
      test.error_kind(failure, "linux-mknod")
      assert "block" in failure.message
    }
  }
}

test test_linux_fake_log_appends_in_place { |ctx|
  let root = test.temp_dir(ctx, name: "linux-log-append")?
  let log = fp"{root}/linux.jsonl"

  # The log already holds bytes that are not valid UTF-8, with no trailing
  # newline. The baseline appends to the open file, so those bytes have to
  # survive; a read-concatenate-rewrite log would lose or replace them.
  let seeded = bytes.from_ints([255, 254, 10])?
  log.write(seeded)

  test.linux_fake(ctx, {log: log})
  let _ = linux.meminfo()?
  let once = log.read_bytes()?
  assert once.starts_with(seeded)
  assert once.len() > seeded.len()

  # A second call appends exactly one more record: the file grows by the same
  # number of bytes again and nothing before it is truncated.
  test.linux_fake(ctx, {log: log})
  let _ = linux.meminfo()?
  let twice = log.read_bytes()?
  assert twice.starts_with(seeded)
  assert twice.len() - once.len() == once.len() - seeded.len()

  # A destination whose parent directories do not exist yet is created.
  let fresh = fp"{root}/missing/deeper/linux.jsonl"
  test.linux_fake(ctx, {log: fresh})
  let _ = linux.meminfo()?
  assert fresh.exists()?
  assert "\"op\":\"meminfo\"" in (fresh.read_text() ?? "")

  # A directory destination raises the native logging error.
  let blocked = fp"{root}/a-directory"
  blocked.mkdir()
  test.linux_fake(ctx, {log: blocked})
  let failed = test.run_script(ctx, "let _ = linux.meminfo()?")?
  assert ! failed.success
  assert "linux-fake-log" in failed.stderr
  assert blocked.is_dir()?
}

test test_linux_text_log_failure_kind { |ctx|
  let root = test.temp_dir(ctx, name: "linux-text-log")?
  let blocked = fp"{root}/file"
  blocked.write("not a directory")
  let blocked_log = fp"{blocked}/linux.jsonl"
  test.linux_fake(ctx, {log: blocked_log})
  let meminfo_failed = test.run_script(ctx, "let _ = linux.meminfo()?")?
  assert ! meminfo_failed.success
  assert "linux-fake-log" in meminfo_failed.stderr

  # The retained native modules arm raises a fake log failure. A nested
  # script observes that process boundary without losing the failure kind.
  let failed = test.run_script(ctx, "let _ = linux.modules()?")?
  assert ! failed.success
  assert "linux-fake-log" in failed.stderr
}

test test_linux_meminfo_reads_the_host_text {
  guard system.uname()?.sysname == "Linux" else {
    # The entry reads `/proc/meminfo` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("linux.meminfo reads /proc/meminfo on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than values a host could change: every reported count is a
  # kilobyte count scaled to bytes, and a free or available count is part of the
  # total it is reported next to.
  let memory = linux.meminfo()?
  assert memory.total > 0
  assert memory.total % 1024 == 0
  assert memory.free >= 0 and memory.free <= memory.total
  assert memory.available >= 0 and memory.available <= memory.total
  assert memory.buffers >= 0 and memory.cached >= 0
  assert memory.swap_free >= 0 and memory.swap_free <= memory.swap_total
}

test test_linux_modules_streams_the_host_text {
  guard system.uname()?.sysname == "Linux" else {
    # The entry reads `/proc/modules` on Linux only; on other platforms the
    # binding is still native, so there is nothing to add here.
    test.skip("linux.modules reads /proc/modules on Linux only")
    return
  }

  # The host text is read-only, so the test states the invariants of the reading
  # policy rather than the modules a host could load. A consumer that stops
  # after one record reads only that record; a host that reports no module
  # leaves the loop empty rather than failing.
  let records = linux.modules()?
  for entry in records {
    assert entry.name != ""
    assert entry.size >= 0
    for dependent in entry.used_by {
      assert dependent != ""
    }

    break
  }

  # The whole text interprets into records with the same shape.
  for entry in linux.modules()?.collect() {
    assert entry.name != ""
    assert entry.size >= 0
    for dependent in entry.used_by {
      assert dependent != ""
    }
  }
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
test test_linux_module_policy_uses_the_configured_tree { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    # On other platforms the binding is still native, so there is nothing to
    # add here.
    test.skip("linux.modinfo reads a module tree on Linux only")
    return
  }

  let root = test.temp_dir(ctx, name: "linux-modules")?
  fp"{root}/demo-name.ko".write(
    "description=Demo module\0license=MIT\0version=2\0depends=dep,missing\0parm=debug:Enable debug (bool)\0parm=mode:Mode (charp)\0",
  )
  fp"{root}/dep.ko".write(
    "description=dep module\0license=GPL\0version=1\0",
  )

  # The nested source is a template rather than an f-string: its `${...}`
  # interpolations belong to the nested script, so the outer checker must not
  # resolve them, and only the tree's path is substituted.
  let nested_template = """\nproc main() [io, fs, error] {
  let info = linux.modinfo(\"demo-name\")?
  print f\"{info.name}|{info.description}|{info.license}|{info.version}\"
  for param in info.params {
    print f\"param={param.name}|{param.type}|{param.description}\"
  }
  print f\"explicit={linux.modinfo(p\"{ROOT_DIR}/demo-name.ko\".display())?.name}\"
  match linux.modinfo(\"nothing-here\") {
    Ok(_) => { print \"unexpected\" }
    Err(failure) => { print f\"missing={failure.message}\" }
  }
  linux.depmod(\"\")?
  print fs.read_text(p\"{ROOT_DIR}/modules.dep\")?
}
"""
  let source = nested_template.replace("{ROOT_DIR}", with: f"{root}")

  let environment = {XSH_MODULES_DIR: f"{root}"}

  # Arguments are positional-only, so the empty argv and stdin are spelled out.
  let nested = test.run_script(
    ctx,
    source,
    [],
    environment,
    b"",
    "linux-module-policy",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = nested
    assert assertion_condition, assertion_message
  }
  assert nested.stdout == f"""demo_name|Demo module|MIT|2
param=debug|bool|Enable debug
param=mode|charp|Mode
explicit=demo_name
missing=module not found
demo-name.ko: dep.ko
dep.ko:

"""
}

test test_linux_open_files_tracks_a_live_child_descriptor { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("linux.open_files reads live Linux process descriptors")
    return
  }

  let root = test.temp_dir(ctx, name: "linux-open-files")?
  let source = fp"{root}/source.txt"
  let ready = fp"{root}/ready"
  let release = fp"{root}/release"
  let closed = fp"{root}/closed"
  let stop = fp"{root}/stop"
  source.write("payload")
  let child = spawn process.command_argv(
    "sh",
    [
      "sh",
      "-c",
      "exec 3<\"$1\"; touch \"$2\"; while [ ! -e \"$3\" ]; do sleep 0.02; done; exec 3<&-; touch \"$4\"; while [ ! -e \"$5\" ]; do sleep 0.02; done",
      "sh",
      source,
      ready,
      release,
      closed,
      stop,
    ],
  )?

  for _ in range(0, 500) {
    break when ready.exists()
    time.sleep(10ms)
  }

  assert ready.exists()?, "child did not open its descriptor"

  let before = linux.open_files(child.pid)?.collect()
  assert before |> any .path == source, "open descriptor must be visible"

  release.write("")
  for _ in range(0, 500) {
    break when closed.exists()
    time.sleep(10ms)
  }

  assert closed.exists()?, "child did not close its descriptor"
  let after = linux.open_files(child.pid)?.collect()
  assert ! after.is_empty(), "child must still be visible"
  assert ! (after |> any .path == source), "closed descriptor must disappear"

  stop.write("")
  assert wait child?.exited_with(0)
}
