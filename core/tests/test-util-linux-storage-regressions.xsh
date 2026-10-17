type CpuSummaryField = {field: Str, data: Str}
type CpuSummaryDocument = {lscpu: List[CpuSummaryField]}
type CommandOutput = {status: Int, stdout: Str, stderr: Str}
type NamespaceRecord = {
  ns: Int, type: Str, path: Path, nprocs: Int, pid: Int, ppid: Int, uid: Int, command: Str,
  pns: Int, ons: Int, netnsid: Int?, nsfs: List[Path],
}
type NamespaceHolder = {pid: Int, release: Path, handle: ProcessHandle}

proc rooted_storage_command(ctx: TestContext, source: Str, args: List[Str], name: Str) [fs, process, env, error] -> Result[CommandOutput] {
  let result = test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir}, b"", name)?
  Ok({status: result.status, stdout: result.stdout, stderr: result.stderr})
}

proc write_storage_fixture(root: FsRoot) [fs, error] {
  root.mkdir(p"sys/class/block", parents: true)
  for name in ["disk", "disk1"] {
    let path = if name == "disk" { "sys/devices/disk" } else { "sys/devices/disk/disk1" }
    root.mkdir(fp"{path}/queue", parents: true)
    root.write(fp"{path}/dev", if name == "disk" { "8:0\n" } else { "8:1\n" })
    root.write(fp"{path}/size", if name == "disk" { "100\n" } else { "50\n" })
    root.write(fp"{path}/ro", "0\n")
    root.write(fp"{path}/removable", "0\n")
    root.symlink(
      target: if name == "disk" { p"../../devices/disk" } else { p"../../devices/disk/disk1" },
      path: fp"sys/class/block/{name}",
    )
  }
  root.write(p"sys/devices/disk/disk1/partition", "1\n")
  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", "1 0 0:1 / / rw - rootfs rootfs rw\n42 1 8:1 / /owned/a\\040b rw - ext4 /dev/disk/by-id/partition rw\n")
}

proc write_cpu_fixture(root: FsRoot) [fs, error] {
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  root.write(p"sys/devices/system/cpu/possible", "0-1\n")
  root.write(p"sys/devices/system/cpu/present", "0-1\n")
  root.write(p"sys/devices/system/cpu/online", "0\n")
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/cpuinfo", "processor : 0\nvendor_id : Acme\nmodel name : Example CPU\n\nprocessor : 1\nvendor_id : Acme\nmodel name : Example CPU\n")
  for index in range(2) {
    root.mkdir(fp"sys/devices/system/cpu/cpu{index}/topology", parents: true)
    root.write(fp"sys/devices/system/cpu/cpu{index}/topology/physical_package_id", "0\n")
    root.write(fp"sys/devices/system/cpu/cpu{index}/topology/core_id", f"{index}\n")
  }
}

proc run_applet(ctx: TestContext, name: Str, args: List[Str]) [fs, process, error] -> Result[CommandOutput] {
  let root = test.temp_dir(ctx, name: f"util-linux-{name}")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/{name}.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc require_user_namespaces(ctx: TestContext) [fs, process, error] -> Result[Bool] {
  let probe = run_applet(ctx, "unshare", ["-r", "true"])?
  if probe.status != 0 {
    test.skip(f"the kernel refuses unprivileged user namespaces: {probe.stderr.trim()}")
    return false
  }
  true
}

proc hold_user_namespace(ctx: TestContext) [fs, process, time, error] -> Result[NamespaceHolder] {
  let root = test.temp_dir(ctx, name: "lsns-user-holder")?
  let ready = fp"{root}/ready"
  let release = fp"{root}/release"
  let source = """cli main(ready: Path, release: Path) {
  ready.write("up")
  for _ in range(0, 1500) {
    break when release.exists()?
    time.sleep(20ms)
  }
}
"""
  let holder = test.temp_file(ctx, name: "lsns-user-holder.xsh", contents: bytes.from_text(source))?
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/unshare.xsh".display(), "-r", ctx.xsh_bin.display(), holder.display(), ready.display(), release.display()]
  let handle = spawn process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"})?
  for _ in range(0, 500) {
    break when ready.exists()
    time.sleep(10ms)
  }
  if !ready.exists() {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    return Err(error.failure("the namespace holder did not start"))
  }
  var found = null
  for entry in process.list()? {
    if entry.argv.find(holder.display()) != null and entry.pid != handle.pid { found = entry.pid }
  }
  guard let pid = found else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    return Err(error.failure("the namespace holder is not running"))
  }
  Ok({pid: pid, release: release, handle: handle})
}

test test_util_linux_rooted_storage_commands_join_block_and_mount_identity { |ctx|
  let dir = test.temp_dir(ctx, name: "util-linux-storage-join")?
  let root = fs.open_root(dir)?
  defer root.close()
  write_storage_fixture(root)
  let lsblk_source = "use lib.storage\nproc main(...argv: List[Str]) { let root = fs.open_root(fp\"{argv[0]}\")?; defer root.close(); storage.lsblk_from_root(root, [\"-Jb\", \"-o\", \"NAME,MAJ:MIN,SIZE,TYPE,MOUNTPOINT\"]) }"
  let findmnt_source = "use lib.storage\nproc main(...argv: List[Str]) { let root = fs.open_root(fp\"{argv[0]}\")?; defer root.close(); storage.findmnt_from_root(root, [\"-J\", \"-l\", \"-S\", \"/dev/disk/by-id/partition\", \"-o\", \"SOURCE,TARGET,MAJ:MIN,FSTYPE\"]) }"
  let lsblk = rooted_storage_command(ctx, lsblk_source, [f"{dir}"], "lsblk")?
  assert lsblk.status == 0, lsblk.stderr
  let devices = json.get(json.decode(lsblk.stdout)?, ["blockdevices"])?.require(List[Any])?
  assert devices.len() == 1
  let partitions = json.get(devices[0], ["children"])?.require(List[Any])?
  assert partitions.len() == 1
  let partition = partitions[0]
  assert json.get(partition, ["name"])?.require(Str)? == "disk1"
  let block_device_id = json.get(partition, ["maj:min"])?.require(Str)?
  assert block_device_id == "8:1"
  assert json.get(partition, ["size"])?.require(Int)? == 25600
  assert json.get(partition, ["type"])?.require(Str)? == "part"
  let block_mount = json.get(partition, ["mountpoint"])?.require(Str)?

  let findmnt = rooted_storage_command(ctx, findmnt_source, [f"{dir}"], "findmnt")?
  assert findmnt.status == 0, findmnt.stderr
  let mounts = json.get(json.decode(findmnt.stdout)?, ["filesystems"])?.require(List[Any])?
  assert mounts.len() == 1
  let mount = mounts[0]
  assert json.get(mount, ["source"])?.require(Str)? == "/dev/disk/by-id/partition"
  let mount_target = json.get(mount, ["target"])?.require(Str)?
  assert mount_target == "/owned/a b"
  let mount_device_id = json.get(mount, ["maj:min"])?.require(Str)?
  assert mount_device_id == "8:1"
  assert json.get(mount, ["fstype"])?.require(Str)? == "ext4"
  assert block_device_id == mount_device_id
  assert block_mount == mount_target
}

test test_util_linux_lscpu_json_summary_keeps_field_values_as_strings { |ctx|
  let dir = test.temp_dir(ctx, name: "util-linux-lscpu-json")?
  let root = fs.open_root(dir)?
  defer root.close()
  write_cpu_fixture(root)
  let source = fp"{ctx.core_dir}/lscpu.xsh".read_text()?
  let output = test.run_script(ctx, source, ["--json", "--sysroot", f"{dir}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "lscpu")?
  assert output.success, output.stderr
  let document = json.decode(output.stdout)?.require(CpuSummaryDocument)?
  assert {field: "Architecture:", data: "x86_64"} in document.lscpu
  assert {field: "CPU(s):", data: "2"} in document.lscpu
  assert {field: "On-line CPU(s) list:", data: "0"} in document.lscpu
}

test test_util_linux_lsns_selects_and_releases_a_live_user_namespace { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold_user_namespace(ctx)?
  defer holder.handle.cancel(signal: "TERM", kill_after: 2s)
  defer holder.release.write("")
  let inode = fs.stat(fp"/proc/{holder.pid}/ns/user")?.ino
  assert inode != fs.stat(p"/proc/self/ns/user")?.ino
  let selected = run_applet(ctx, "lsns", ["-p", f"{holder.pid}", "-t", "user", "-n", "-r", "-o", "NS,TYPE,NPROCS"])?
  assert selected.status == 0, selected.stderr
  assert selected.stdout == f"{inode} user 1\n", selected.stdout

  holder.release.write("")
  guard let completed = process.wait_timeout([holder.handle], 5s)? else {
    holder.handle.cancel(signal: "KILL", kill_after: 0ms)
    assert false, "the namespace holder did not exit after release"
    return
  }
  assert completed.status.exited_with(0)
  assert ! (fp"/proc/{holder.pid}/ns/user".exists())
  let remaining: List[NamespaceRecord] = linux.namespaces(null)?
  assert [entry for entry in remaining if entry.type == "user" and entry.ns == inode].is_empty()
}
