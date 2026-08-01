use super::common::*;

#[cfg(target_os = "linux")]
struct RealCgroupRoot {
    path: PathBuf,
    from_test_env: bool,
}

#[cfg(target_os = "linux")]
impl RealCgroupRoot {
    fn mount_path(&self) -> PathBuf {
        if let Some(path) = std::env::var_os("XSH_TEST_CGROUP_MOUNT") {
            return PathBuf::from(path);
        }
        if self.path.starts_with("/sys/fs/cgroup") {
            return PathBuf::from("/sys/fs/cgroup");
        }
        if self.from_test_env {
            self.path.clone()
        } else {
            PathBuf::from("/sys/fs/cgroup")
        }
    }
}

#[cfg(target_os = "linux")]
fn writable_real_cgroup_root() -> Option<RealCgroupRoot> {
    if let Some(root) = std::env::var_os("XSH_TEST_CGROUP_ROOT") {
        let path = PathBuf::from(root);
        return Some(
            usable_real_cgroup_root(path.clone(), true).unwrap_or_else(|| {
                panic!(
                    "XSH_TEST_CGROUP_ROOT is not a writable cgroups v2 root: {}",
                    path.display()
                )
            }),
        );
    }
    current_cgroup_root().and_then(|root| usable_real_cgroup_root(root, false))
}

#[cfg(target_os = "linux")]
fn current_cgroup_root() -> Option<PathBuf> {
    let text = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            return Some(Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/')));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn usable_real_cgroup_root(path: PathBuf, from_test_env: bool) -> Option<RealCgroupRoot> {
    if !path.join("cgroup.controllers").exists() {
        return None;
    }
    if from_test_env
        && write_cgroup_control(&path.join("cgroup.subtree_control"), "+cpu\n").is_err()
    {
        return None;
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let probe = path.join(format!(
        "xsh-test-cgroup-probe-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&probe).ok()?;
    let writable = write_cgroup_control(&probe.join("cpu.max"), "max 100000\n").is_ok();
    let _ = std::fs::remove_dir(&probe);
    writable.then_some(RealCgroupRoot {
        path,
        from_test_env,
    })
}

#[cfg(target_os = "linux")]
fn write_cgroup_control(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.write_all(content.as_bytes())
}

#[cfg(not(target_os = "linux"))]
#[test]
fn xsht_trace_rejects_syscalls_on_non_linux() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "trace",
            "--syscalls",
            "tests/fixtures/runtime/cli-simple.xsh",
        ])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("`--syscalls` is only supported on Linux")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn xsht_syscall_trace_includes_summary_when_ptrace_available() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "trace",
            "--syscalls",
            "--trace-top-syscalls",
            "3",
            "tests/fixtures/runtime/cli-simple.xsh",
        ])
        .output()
        .expect("run xsht");

    let stderr = String::from_utf8(output.stderr).unwrap();
    if !output.status.success() && stderr.contains("syscall tracing setup failed") {
        return;
    }

    assert!(output.status.success(), "{stderr}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
    assert!(stderr.contains("trace summary"), "{stderr}");
    assert!(stderr.contains("syscall_count="), "{stderr}");
    assert!(stderr.contains("top_syscalls_by_count:"), "{stderr}");
    assert!(stderr.contains("per_program_top_syscalls:"), "{stderr}");
    assert!(stderr.contains("per_process_top_syscalls:"), "{stderr}");
}

#[cfg(target_os = "linux")]
#[test]
fn xsht_syscall_trace_subprocess_spawn_reaches_exec() {
    let script = write_temp_script(
        "syscall-trace-subprocess-spawn",
        r#"
let out = run.text printf "%s\n" "hi" ?
print ${out.trim()}
"#,
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "--syscalls", "--trace-top-syscalls", "3"])
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run xsht");

    let status = wait_child_status(&mut child, Duration::from_secs(10));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("child stdout")
        .read_to_end(&mut stdout)
        .expect("read child stdout");
    child
        .stderr
        .take()
        .expect("child stderr")
        .read_to_end(&mut stderr)
        .expect("read child stderr");
    let _ = std::fs::remove_file(script);

    let stderr = String::from_utf8(stderr).unwrap();
    if !status.success() && stderr.contains("syscall tracing setup failed") {
        return;
    }

    assert!(status.success(), "{stderr}");
    assert_eq!(String::from_utf8(stdout).unwrap(), "hi\n");
    assert!(stderr.contains("top_syscalls_by_count:"), "{stderr}");
}

#[test]
fn linux_module_primitives_are_declared_but_runtime_gated() {
    let output = run_temp_script("linux-gated", "let _ = linux.halt()?\n");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("linux-unimplemented"), "{stderr}");
}

#[cfg(target_os = "linux")]
#[test]
fn run_cpumax_uses_real_cgroup_v2_when_available() {
    let Some(root) = writable_real_cgroup_root() else {
        return;
    };
    let script = write_temp_script(
        "run-cpumax-real-cgroup",
        r#"
let out = run.text --cpumax=25 sh -c r"""
cg=$(sed -n 's/^0:://p' /proc/self/cgroup)
mount=${XSH_TEST_CGROUP_MOUNT:-/sys/fs/cgroup}
printf 'cg=%s\n' "$cg"
cat "${mount}/${cg#/}/cpu.max"
""" ?
print ${out}
"#,
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsh"));
    command.arg(&script);
    let mount_path = root.mount_path();
    command.env("XSH_TEST_CGROUP_MOUNT", &mount_path);
    if root.from_test_env {
        command.env("XSH_CGROUP_ROOT", &root.path);
    } else {
        command.env_remove("XSH_CGROUP_ROOT");
    }
    let output = command.output().expect("run xsh");
    let _ = std::fs::remove_file(script);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cgroup = stdout
        .lines()
        .find_map(|line| line.strip_prefix("cg="))
        .expect("child cgroup line");
    assert!(cgroup.contains("xsh-run-"), "{stdout}");
    assert!(
        stdout.lines().any(|line| line == "25000 100000"),
        "{stdout}"
    );
    let cgroup_path = mount_path.join(cgroup.trim_start_matches('/'));
    assert!(
        !cgroup_path.exists(),
        "cgroup was not cleaned up: {}",
        cgroup_path.display()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_real_read_only_surfaces_work_in_container() {
    let output = run_temp_script(
        "linux-real-read-only",
        r#"
let root = fp"/tmp/xsh-linux-real-${process.current_pid()?}"
fs.mkdir(root)?
let source = fp"${root}/source.bin"
let dest = fp"${root}/dest.bin"
let copy = fp"${root}/copy.bin"
defer root.remove_dir()
defer fs.remove(copy, missing_ok: true)
defer fs.remove(dest, missing_ok: true)
defer fs.remove(source, missing_ok: true)
fs.write(source, b"abcdef")?
fs.write(copy, b"------")?

env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {
  linux.read_device(source, dest, bytes: 3)?
  linux.write_device(copy, source)?
  test.eq(dest.read_bytes()?, b"abc")?
  test.eq(copy.read_bytes()?, b"abcdef")?
  test.ok(linux.interfaces()?.collect().len() > 0)?
  let routes = linux.routes()?.collect()
  test.ok(routes.len() >= 0)?
  test.ok(linux.meminfo()?.total > 0)?
  let modules = linux.modules()?.collect()
  test.ok(modules.len() >= 0)?
  test.ok(linux.is_mountpoint(/)?)?
  test.ok(linux.disk_usage(/)?.collect()[0].total > 0)?
  let all_usage = linux.disk_usage()?.collect()
  test.ok(all_usage.len() > 0)?
  test.ok(linux.root_device()?.trim() != "")?
  let sysctl_value = linux.sysctl_get("kernel.ostype")?
  test.ok(sysctl_value != "")?
  match linux.loop_list() {
    Ok(loops) => {
      test.ok(loops |> count() >= 0)?
    }
    Err(err) => {
      test.error_kind(err, "linux-loop")?
    }
  }
  let open_files = linux.open_files(process.current_pid()?)?.collect()
  test.ok(open_files |> count() >= 0)?

  match linux.file_attrs(source) {
    Ok(attrs) => {
      match linux.set_file_attrs(source, attrs.flags) {
        Ok(_) => {}
        Err(err) => {
          test.error_kind(err, "linux-file-attrs")?
        }
      }
    }
    Err(err) => {
      test.error_kind(err, "linux-file-attrs")?
    }
  }

  match linux.file_version(source) {
    Ok(version) => {
      match linux.set_file_version(source, version) {
        Ok(_) => {}
        Err(err) => {
          test.error_kind(err, "linux-file-version")?
        }
      }
    }
    Err(err) => {
      test.error_kind(err, "linux-file-version")?
    }
  }

  test.error_kind(linux.dhcp_socket("bad/interface"), "linux-dhcp-socket")?
  test.error_kind(linux.link_up("bad/interface"), "linux-link-up")?
  test.error_kind(linux.set_ipv4_address("lo", "not-an-ip", "255.255.255.0"), "linux-set-ipv4-address")?
  test.error_kind(linux.add_default_ipv4_route("not-an-ip"), "linux-add-default-ipv4-route")?
} ?
"#,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_real_chroot_reports_real_error() {
    let output = run_temp_script(
        "linux-real-chroot-error",
        r#"
let missing = fp"/tmp/xsh-missing-chroot-${process.current_pid()?}"
env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {
  test.error_kind(linux.chroot(missing), "linux-chroot")?
} ?
"#,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn linux_module_dry_run_primitives_are_observable() {
    let root = temp_path("linux-dry-run");
    let log = root.join("linux.jsonl");
    let source = format!(
        "\
let root = Path({})
let log = fp\"${{root}}/linux.jsonl\"
let seed = fp\"${{root}}/seed\"
let random = fp\"${{root}}/random\"
fs.mkdir(root, parents: true)?
fs.write(seed, \"seed\")?
env XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_SIGNAL=USR2 XSH_LINUX_DRY_RUN_LOG=(log) {{
  linux.write_device(/dev/urandom, seed)?
  linux.read_device(/dev/urandom, random, bytes: 4)?
  linux.mount(\"proc\", /proc, fstype: \"proc\", options: [\"nosuid\"])?
  linux.mount_all()?
  linux.umount_all(types: [\"tmpfs\"])?
  linux.swapon_all()?
  linux.swapoff_all()?
  let root_device = linux.root_device()?
  linux.link_up(\"lo\")?
  linux.set_ipv4_address(\"eth0\", \"192.0.2.10\", \"255.255.255.0\")?
  linux.add_default_ipv4_route(\"192.0.2.1\", interface: \"eth0\")?
  let interfaces = linux.interfaces()?.collect()
  let routes = linux.routes()?.collect()
  let meminfo = linux.meminfo()?
  let modules = linux.modules()?.collect()
  let dmesg = linux.dmesg()?.collect()
  let is_mount = linux.is_mountpoint(/proc)?
  let usage = linux.disk_usage(/)?.collect()
  let sysctl_value = linux.sysctl_get(\"kernel.pid_max\")?
  linux.sysctl_set(\"kernel.pid_max\", sysctl_value)?
  let attrs = linux.file_attrs(seed)?
  linux.set_file_attrs(seed, attrs.flags)?
  linux.chroot(root)?
  linux.mknod(fp\"${{root}}/null\", \"char\", 1, 3)?
  linux.insmod(fp\"${{root}}/demo.ko\", params: \"debug=1\")?
  linux.rmmod(\"demo\", force: true)?
  linux.pivot_root(root, fp\"${{root}}/oldroot\")?
  linux.switch_root(root, /sbin/init)?
  let epoch_ms = linux.hwclock()?
  linux.set_hwclock(epoch_ms)?
  linux.set_system_clock(epoch_ms)?
  let rfkill = linux.rfkill_list()?.collect()
  linux.rfkill_block(rfkill[0].id)?
  linux.rfkill_unblock(rfkill[0].id)?
  let loop_device = linux.loop_attach(seed)?
  linux.loop_detach(loop_device)?
  let loops = linux.loop_list()?.collect()
  linux.mkswap(seed)?
  linux.swapon(seed, priority: 1)?
  linux.swapoff(seed)?
  let blkid = linux.blkid(seed)?
  let info = linux.modinfo(\"demo\")?
  linux.modprobe(\"demo\", params: \"debug=1\")?
  linux.depmod(\"dry-run\")?
  let open_files = linux.open_files(123)?.collect()
  let table = linux.partition_table(seed)?
  linux.write_partition_table(seed, table)?
  let fsck = linux.fsck(seed, fstype: \"ext4\")?
  let uevents = linux.uevent_stream()?
  var uevent_action = \"\"
  var uevent_subsystem = \"\"
  var uevent_env = \"\"
  for event in uevents {{
    uevent_action = event.action
    uevent_subsystem = event.subsystem
    uevent_env = event.env[0].name
    break
  }}
  linux.sysctl_load_dirs([/etc/sysctl.d], fallback: /etc/sysctl.conf)?
  linux.kill_all(signal: \"TERM\", except_pid1: true)?
  linux.halt()?
  linux.poweroff()?
  linux.reboot()?
  print ${{root_device}} ${{interfaces[0].name}} ${{interfaces[0].addresses[0].family}} ${{routes[0].dst}} ${{routes[0].gateway}} ${{meminfo.total > 0}} ${{modules[0].name}} ${{dmesg[0]}} ${{is_mount}} ${{usage[0].device}} ${{sysctl_value}} ${{attrs.immutable}} ${{attrs.append_only}} ${{rfkill[0].type}} ${{loops[0].device == loop_device}} ${{blkid.type}} ${{info.params[0].name}} ${{open_files[0].type}} ${{table.partitions[0].name}} ${{fsck.status}} ${{uevent_action}} ${{uevent_subsystem}} ${{uevent_env}}
}} ?
let random_bytes = random.read_bytes()?
let log_text = fs.read_text(log)?
print ${{random_bytes.len()}} ${{\"poweroff\" in log_text}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("linux-dry-run", &source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "rootfs eth0 inet default 192.0.2.1 true xsh_demo xsh dry-run kernel message true rootfs 1 true true wlan true ext4 debug file root 0 add block ACTION\n4 true\n"
    );
    let log_text = std::fs::read_to_string(log).expect("read linux dry-run log");
    assert!(log_text.contains("\"op\":\"kill_all\""), "{log_text}");
    assert!(log_text.contains("\"signal\":\"TERM\""), "{log_text}");
    assert!(log_text.contains("\"except_pid1\":\"true\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"mount\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"meminfo\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"routes\""), "{log_text}");
    assert!(
        log_text.contains("\"op\":\"set_ipv4_address\""),
        "{log_text}"
    );
    assert!(
        log_text.contains("\"op\":\"add_default_ipv4_route\""),
        "{log_text}"
    );
    assert!(log_text.contains("\"op\":\"is_mountpoint\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"disk_usage\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"sysctl_set\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"set_file_attrs\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"mknod\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"loop_attach\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"swapon\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"modprobe\""), "{log_text}");
    assert!(
        log_text.contains("\"op\":\"write_partition_table\""),
        "{log_text}"
    );
    assert!(log_text.contains("\"op\":\"uevent_stream\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"reboot\""), "{log_text}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn linux_module_dry_run_records_cover_seed_replacement_shapes() {
    let source = "\
env XSH_LINUX_DRY_RUN=1 XSH_LINUX_SYSCTL_VALUE=65535 {
  let meminfo = linux.meminfo()?
  let modules = linux.modules()?.collect()
  let root_usage = linux.disk_usage()?.collect()
  let tmp_usage = linux.disk_usage(/tmp)?.collect()
  let sysctl_value = linux.sysctl_get(\"kernel.pid_max\")?
  print ${meminfo.total} ${meminfo.free} ${meminfo.available} ${meminfo.buffers} ${meminfo.cached} ${meminfo.swap_total} ${meminfo.swap_free}
  print ${modules[0].name} ${modules[0].size} ${modules[0].used_by[0]} ${modules[0].used_by.len()}
  print ${root_usage[0].device} ${root_usage[0].mount} ${root_usage[0].fstype} ${root_usage[0].total} ${root_usage[0].used} ${root_usage[0].available}
  print ${tmp_usage[0].mount} ${linux.is_mountpoint(/proc)?} ${linux.is_mountpoint(/tmp)?} ${sysctl_value}
} ?
";

    let output = run_temp_script("linux-dry-run-records", source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "1073741824 268435456 536870912 67108864 134217728 536870912 402653184\nxsh_demo 4096 xsh_dep 1\nrootfs / tmpfs 1073741824 268435456 805306368\n/tmp true false 65535\n"
    );
}

#[test]
fn linux_module_dry_run_file_attrs_cover_seed_flag_set() {
    let root = temp_path("linux-file-attrs-dry-run");
    let source = format!(
        "\
let root = Path({})
let seed = fp\"${{root}}/seed\"
fs.mkdir(root, parents: true)?
fs.write(seed, \"seed\")?
env XSH_LINUX_DRY_RUN=1 XSH_LINUX_FILE_ATTRS_FLAGS=250111 XSH_LINUX_FILE_VERSION=7 {{
  let attrs = linux.file_attrs(seed)?
  let version = linux.file_version(seed)?
  linux.set_file_attrs(seed, attrs.flags)?
  linux.set_file_version(seed, version)?
  print ${{attrs.flags}} ${{version}} ${{attrs.indexed_directory}} ${{attrs.secure_deletion}} ${{attrs.undelete}} ${{attrs.sync}} ${{attrs.dirsync}} ${{attrs.immutable}} ${{attrs.append_only}} ${{attrs.no_dump}} ${{attrs.no_atime}} ${{attrs.compression_requested}} ${{attrs.journaled_data}} ${{attrs.no_tailmerging}} ${{attrs.top_of_directory_hierarchies}}
}} ?
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("linux-file-attrs-dry-run", &source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "250111 7 true true true true true true true true true true true true true\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn linux_module_dry_run_rejects_seed_parity_invalid_inputs() {
    let source = "\
env XSH_LINUX_DRY_RUN=1 {
  match linux.sysctl_get(\"kernel..pid_max\") {
    Err(e) => {
      test.error_kind(e, \"linux-sysctl\")?
      print \"linux-sysctl\" ${\"invalid\" in e.message}
    }
  }
  match linux.sysctl_set(\"../kernel.pid_max\", \"1\") {
    Err(e) => {
      test.error_kind(e, \"linux-sysctl\")?
      print \"linux-sysctl\" ${\"invalid\" in e.message}
    }
  }
  match linux.set_file_attrs(/tmp/file, -1) {
    Err(e) => {
      test.error_kind(e, \"linux-file-attrs\")?
      print \"linux-file-attrs\" ${\"between 0 and 4294967295\" in e.message}
    }
  }
  match linux.set_file_attrs(/tmp/file, 4294967296) {
    Err(e) => {
      test.error_kind(e, \"linux-file-attrs\")?
      print \"linux-file-attrs\" ${\"between 0 and 4294967295\" in e.message}
    }
  }
  match linux.set_file_version(/tmp/file, -1) {
    Err(e) => {
      test.error_kind(e, \"linux-file-version\")?
      print \"linux-file-version\" ${\"between 0 and 4294967295\" in e.message}
    }
  }
  match linux.kill_all(signal: \"BOGUS\") {
    Err(e) => {
      test.error_kind(e, \"invalid-signal\")?
      print \"invalid-signal\"
    }
  }
  match linux.mknod(/tmp/file, \"socket\", 0, 0) {
    Err(e) => {
      test.error_kind(e, \"linux-mknod\")?
      print \"linux-mknod\" ${\"block\" in e.message}
    }
  }
} ?
";

    let output = run_temp_script("linux-dry-run-invalid", source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "linux-sysctl true\nlinux-sysctl true\nlinux-file-attrs true\nlinux-file-attrs true\nlinux-file-version true\ninvalid-signal\nlinux-mknod true\n"
    );
}

#[test]
fn unix_module_dry_run_primitives_are_observable() {
    let root = temp_path("linux-unix-dry-run");
    let log = root.join("unix.jsonl");
    let source = format!(
        "\
let root = Path({})
let log = fp\"${{root}}/unix.jsonl\"
fs.mkdir(root, parents: true)?
let command = process.command_argv(\"demo\", [\"demo\", \"arg\"])
env XSH_UNIX_DRY_RUN=1 XSH_UNIX_DRY_RUN_SIGNAL=USR1 XSH_UNIX_DRY_RUN_PID=42 XSH_UNIX_UPTIME_SECONDS=17 XSH_UNIX_DRY_RUN_LOG=(log) {{
  let reaped = unix.reap_child_events()?.collect()
  let uptime = unix.uptime_seconds()?
  let tty = unix.tty()?
  let identity = unix.id()?
  let attrs = unix.tty_attrs()?
  unix.set_tty_attrs(attrs)?
  unix.set_hostname(\"xsh\")?
  let child = unix.spawn_process_group(command)?
  let logged_child = unix.spawn_logged_process_group(command, command)?
  let tty_child = unix.spawn_with_tty(command, tty: \"tty1\")?
  unix.kill_process_group(child.pid, \"TERM\")?
  unix.exec(command)?
  print ${{reaped.len()}} ${{uptime}} ${{tty}} ${{identity.groups[0].name}} ${{attrs.raw}} ${{child.pid}} ${{child.new_session}} ${{logged_child.pid}} ${{logged_child.log_pid}} ${{tty_child.pid}} ${{tty_child.new_session}}
}} ?
let log_text = fs.read_text(log)?
print ${{\"set_hostname\" in log_text}} ${{\"spawn_process_group\" in log_text}} ${{\"spawn_logged_process_group\" in log_text}} ${{\"exec\" in log_text}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("unix-dry-run", &source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "0 17 /dev/tty root true 1000 false 1001 1002 1003 true\ntrue true true true\n"
    );
    let log_text = std::fs::read_to_string(log).expect("read unix dry-run log");
    assert!(
        log_text.contains("\"op\":\"reap_child_events\""),
        "{log_text}"
    );
    assert!(
        log_text.contains("\"op\":\"kill_process_group\""),
        "{log_text}"
    );
    assert!(log_text.contains("\"op\":\"tty\""), "{log_text}");
    assert!(log_text.contains("\"op\":\"set_tty_attrs\""), "{log_text}");
    assert!(log_text.contains("\"new_session\":\"false\""), "{log_text}");
    assert!(log_text.contains("\"new_session\":\"true\""), "{log_text}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unix_module_dry_run_child_events_are_typed() {
    let output = run_temp_script(
        "unix-dry-run-child-events",
        "\
type ChildEvent = {pid: Int, status: Status}
env XSH_UNIX_DRY_RUN=1 XSH_UNIX_DRY_RUN_EVENT_KIND=child XSH_UNIX_DRY_RUN_PID=42 XSH_UNIX_DRY_RUN_CHILD_PID=43 XSH_UNIX_DRY_RUN_STATUS_KIND=signal XSH_UNIX_DRY_RUN_STATUS_CODE=15 {
  let child_events: List[ChildEvent] = unix.reap_child_events()?.collect()
  print ${child_events[0].pid} ${child_events[0].status.signaled()} ${child_events[0].status.signal_number()?}
} ?
",
    );

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "43 true 15\n");
}

#[test]
fn unix_uptime_seconds_is_real_by_default() {
    let output = run_temp_script(
        "unix-real-uptime",
        "\
let uptime = unix.uptime_seconds()?
print ${uptime >= 0}
",
    );

    if cfg!(any(target_os = "linux", target_os = "macos")) {
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
    } else {
        assert_eq!(output.status.code(), Some(3));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("unix-unsupported"), "{stderr}");
    }
}

#[test]
fn unix_set_hostname_requires_dry_run_or_real_mode() {
    let output = run_temp_script(
        "unix-set-hostname-gated",
        "\
match unix.set_hostname(\"xsh\") {
  Err(e) => {
    test.error_kind(e, \"unix-real-required\")?
    print \"unix-real-required\"
  }
}
",
    );

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "unix-real-required\n"
    );
}

#[test]
fn unix_exec_replaces_child_xsh_process() {
    let helper = env!("CARGO_BIN_EXE_xsh-test-show-argv");
    let source = format!(
        "\
let command = process.command_argv(Path({}), [\"show-argv\", \"ok\"])
unix.exec(command)?
print \"not-reached\"
",
        xsh_string_literal(helper)
    );

    let output = run_temp_script("unix-exec", &source);

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "6f6b\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_reap_child_events_reports_exit_status() {
    let source = "\
type ChildEvent = {pid: Int, status: Status}
let command = process.command_argv(\"false\", [\"false\"])
let child = unix.spawn_process_group(command)?
var events: List[ChildEvent] = []
var tries = 0
while events.len() == 0 and tries < 100 {
  time.sleep(10ms)?
  events = unix.reap_child_events()?.collect()
  tries += 1
}
print ${events[0].pid == child.pid} ${events[0].status.exited_with(1)}
";

    let output = run_temp_script("unix-reap-child-exit-status", source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true true\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_reap_child_events_reports_signal_status() {
    let marker = temp_path("linux-unix-child-signal-ready");
    let _ = std::fs::remove_file(&marker);
    let sleeper = env!("CARGO_BIN_EXE_xsh-test-sleeper");
    let source = format!(
        "\
type ChildEvent = {{pid: Int, status: Status}}
let marker = Path({})
let term = process.signal(\"TERM\")?
let command = process.command_argv(Path({}), [\"sleeper\", marker.display()])
let child = unix.spawn_process_group(command)?
var ready_tries = 0
while ! fs.exists(marker)? and ready_tries < 100 {{
  time.sleep(10ms)?
  ready_tries += 1
}}
unix.kill_process_group(child.pid, \"TERM\")?
var events: List[ChildEvent] = []
var tries = 0
while events.len() == 0 and tries < 100 {{
  time.sleep(10ms)?
  events = unix.reap_child_events()?.collect()
  tries += 1
}}
print ${{events[0].pid == child.pid}} ${{events[0].status.signaled()}} ${{events[0].status.signal_number()? == term.number}}
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(sleeper)
    );

    let output = run_temp_script("unix-reap-child-signal-status", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true\n"
    );
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_spawn_process_group_can_be_signaled_without_killing_parent() {
    let marker = temp_path("linux-process-group-ready");
    let _ = std::fs::remove_file(&marker);
    let sleeper = env!("CARGO_BIN_EXE_xsh-test-sleeper");
    let source = format!(
        "\
let marker = Path({})
let command = process.command_argv(Path({}), [\"sleeper\", marker.display()])
let child = unix.spawn_process_group(command)?
var tries = 0
while ! fs.exists(marker)? and tries < 100 {{
  time.sleep(10ms)?
  tries += 1
}}
unix.kill_process_group(child.pid, \"TERM\")?
time.sleep(50ms)?
let reaped = unix.reap_child_events()?.collect()
match process.kill(child.pid, signal: \"0\") {{
  Err(e) => {{
    test.error_kind(e, \"process-missing\")?
    print ${{child.detach}} ${{child.new_session}} ${{child.ignore_hup}} ${{fs.exists(marker)?}} ${{reaped.len() >= 0}} \"process-missing\"
  }}
}}
",
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(sleeper)
    );

    let output = run_temp_script("unix-process-group", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true false true true true process-missing\n"
    );
    let _ = std::fs::remove_file(marker);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_spawn_logged_process_group_pipes_stdout_and_stderr() {
    let root = temp_path("linux-unix-logged-process-group");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create log root");
    let log = root.join("service.log");
    let source = format!(
        "\
let log = Path({})
let command = process.command_argv(\"sh\", [\"sh\", \"-c\", \"printf service-out; printf service-err >&2\"])
let logger = process.command_argv(\"sh\", [\"sh\", \"-c\", f\"cat > ${{log.display()}}\"] )
let child = unix.spawn_logged_process_group(command, logger)?
var events: List[Record] = []
var tries = 0
while events.len() < 2 and tries < 100 {{
  time.sleep(10ms)?
  events = events.extend(unix.reap_child_events()?.collect())
  tries += 1
}}
let log_text = fs.read_text(log)?
print ${{child.pid > 0}} ${{child.log_pid > 0}} ${{\"service-out\" in log_text}} ${{\"service-err\" in log_text}}
",
        xsh_string_literal(log.to_str().unwrap())
    );

    let output = run_temp_script("unix-logged-process-group", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true true\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn unix_spawn_with_tty_uses_tty_dir_and_new_session() {
    let root = temp_path("linux-unix-spawn-tty");
    let tty_dir = root.join("tty");
    let marker = root.join("session");
    let tty_file = tty_dir.join("tty-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&tty_dir).expect("create tty dir");
    std::fs::write(&tty_file, "").expect("create tty file");
    let helper = env!("CARGO_BIN_EXE_xsh-test-session");
    let source = format!(
        "\
let tty_dir = Path({})
let marker = Path({})
let command = process.command_argv(Path({}), [\"session\", marker.display()])
env XSH_UNIX_TTY_DIR=(tty_dir) {{
  let child = unix.spawn_with_tty(command, tty: \"tty-test\")?
  var tries = 0
  while ! fs.exists(marker)? and tries < 100 {{
    time.sleep(10ms)?
    tries += 1
  }}
  let _reaped = unix.reap_child_events()?
  print ${{child.detach}} ${{child.new_session}} ${{child.ignore_hup}}
}} ?
",
        xsh_string_literal(tty_dir.to_str().unwrap()),
        xsh_string_literal(marker.to_str().unwrap()),
        xsh_string_literal(helper)
    );

    let output = run_temp_script("unix-spawn-tty", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true true\n"
    );
    let session_text = std::fs::read_to_string(&marker).expect("read session marker");
    let fields = session_text.split_whitespace().collect::<Vec<_>>();
    assert_eq!(fields.len(), 2, "{session_text:?}");
    assert_eq!(fields[0], fields[1], "{session_text:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn linux_module_real_syscalls_are_gated_by_platform_and_privilege() {
    #[cfg(target_os = "linux")]
    {
        let target = temp_path("linux-real-mount");
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target).expect("create linux real mount target");
        let source = format!(
            "\
let target = Path({})
env XSH_LINUX_REAL=1 XSH_LINUX_DRY_RUN=0 {{
  linux.mount(\"none\", target, fstype: \"tmpfs\", options: [\"nosuid\", \"nodev\"])?
  print mounted
}} ?
",
            xsh_string_literal(target.to_str().unwrap())
        );

        let output = run_temp_script("linux-real-mount", &source);
        if output.status.success() {
            assert_eq!(String::from_utf8(output.stdout).unwrap(), "mounted\n");
            unmount_linux(&target);
        } else {
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(stderr.contains("linux-mount"), "{stderr}");
            assert!(
                stderr.contains("Operation not permitted")
                    || stderr.contains("permission denied")
                    || stderr.contains("Permission denied"),
                "{stderr}"
            );
        }
        let _ = std::fs::remove_dir_all(target);
    }
}
