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

#[cfg(target_os = "linux")]
#[test]
fn xsht_syscall_trace_includes_summary_when_ptrace_available() {
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
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
        eprintln!(
            "skipped: ptrace syscall tracing unavailable: {}",
            stderr.trim()
        );
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
    let mut child = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
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
        eprintln!(
            "skipped: ptrace syscall tracing unavailable: {}",
            stderr.trim()
        );
        return;
    }

    assert!(status.success(), "{stderr}");
    assert_eq!(String::from_utf8(stdout).unwrap(), "hi\n");
    assert!(stderr.contains("top_syscalls_by_count:"), "{stderr}");
}

#[cfg(target_os = "linux")]
#[test]
fn run_cpumax_uses_real_cgroup_v2_when_available() {
    let Some(root) = writable_real_cgroup_root() else {
        eprintln!("skipped: no writable cgroups v2 root for the cpumax fixture");
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
    let mut command = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"));
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
let root = fp"/tmp/xsh-linux-real-{process.current_pid()?}"
root.mkdir()?
let source = fp"{root}/source.bin"
let dest = fp"{root}/dest.bin"
let copy = fp"{root}/copy.bin"
defer root.remove_dir()
defer copy.remove(missing_ok: true)
defer dest.remove(missing_ok: true)
defer source.remove(missing_ok: true)
source.write(b"abcdef")?
copy.write(b"------")?

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
      test.ok((loops |> count()) >= 0)?
    }
    Err(err) => {
      test.error_kind(err, "linux-loop")?
    }
  }
  let open_files = linux.open_files(process.current_pid()?)?.collect()
  test.ok((open_files |> count()) >= 0)?

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
"#,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
