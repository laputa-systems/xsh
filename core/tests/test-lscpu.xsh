use core.lib.hardware
use core.lib.system_report as report

test test_lscpu_fixture_preserves_cpu_identity_and_offline_selection { |ctx|
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  for name in ["possible", "present"] { root.write(fp"sys/devices/system/cpu/{name}", "0-1\n") }
  root.write(p"sys/devices/system/cpu/online", "0\n")
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/cpuinfo", "processor : 0\nvendor_id : Acme\nmodel name : Example CPU\n\nprocessor : 1\nvendor_id : Acme\nmodel name : Example CPU\n")
  for index in range(2) {
    root.mkdir(fp"sys/devices/system/cpu/cpu{index}/topology", parents: true)
    root.write(fp"sys/devices/system/cpu/cpu{index}/topology/physical_package_id", "0\n")
    root.write(fp"sys/devices/system/cpu/cpu{index}/topology/core_id", f"{index}\n")
  }
  let section = hardware.cpu_from_root(root, "x86_64")?
  let lines = hardware.cpu_rows(section, ["CPU", "CORE", "SOCKET", "ONLINE"], "all", true)?
  assert "0,0,0,Y" in lines
  assert "1,1,0,N" in lines
  let offline = hardware.cpu_rows(section, ["CPU", "ONLINE"], "offline", true)?
  assert offline[-1] == "1,N"
  let summary = hardware.cpu_summary(section, "x86_64")
  assert {field: "Architecture:", data: "x86_64"} in summary
  assert {field: "CPU(s):", data: "2"} in summary
  assert {field: "Core(s) per socket:", data: "2"} in summary
  let sparse: report.CpuSection = {...section, cpus: [
    {...section.cpus[0], package_id: 4, core_id: 7, die_id: 5, numa_node: 2},
    {...section.cpus[1], package_id: 9, core_id: 7, die_id: 5, numa_node: 8},
  ]}
  let columns = ["CPU", "SOCKET", "CORE", "DIE", "NODE"]
  assert hardware.cpu_rows(sparse, columns, "all", true)?[-1] == "1,1,1,1,1"
  assert hardware.cpu_rows(sparse, columns, "all", true, true)?[-1] == "1,9,7,5,8"
  assert hardware.cpu_rows(sparse, ["CACHE"], "all", true, true) is Err(_)
  let caches: List[report.CpuCache] = [
    {id: 0, sysfs_index: 0, owner_cpu_id: 0, level: 1, kind: "Data", size_bytes: 32768, line_size_bytes: 64, sets: 64, shared_cpus: [0]},
    {id: 1, sysfs_index: 1, owner_cpu_id: 0, level: 1, kind: "Instruction", size_bytes: 32768, line_size_bytes: 64, sets: 64, shared_cpus: [0]},
    {id: 2, sysfs_index: 0, owner_cpu_id: 1, level: 1, kind: "Data", size_bytes: 32768, line_size_bytes: 64, sets: 64, shared_cpus: [1]},
    {id: 3, sysfs_index: 1, owner_cpu_id: 1, level: 1, kind: "Instruction", size_bytes: 32768, line_size_bytes: 64, sets: 64, shared_cpus: [1]},
  ]
  let cached: report.CpuSection = {...section, caches: caches, cpus: [
    {...section.cpus[0], cache_indices: [0, 1], cache_ids: [0, 1]},
    {...section.cpus[1], cache_indices: [2, 3], cache_ids: [2, 3]},
  ]}
  assert hardware.cpu_rows(cached, ["CPU", "CACHE"], "all", true)? == ["# CPU,CACHE", "0,0:0", "1,1:1"]
  assert {field: "L1d cache:", data: "64 KiB (2 instances)"} in hardware.cpu_summary(cached, "x86_64")
  assert {field: "L1d cache:", data: "65536 (2 instances)"} in hardware.cpu_summary(cached, "x86_64", true)
  let folder = root.host_path()?
  let script = fp"{ctx.core_dir}/lscpu.xsh"
  let out = fp"{folder}/stdout"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--parse=CPU,ONLINE", "--all", "--sysroot", folder.display()],
    folder, {}, b"", out, fp"{folder}/stderr", timeout: 5s)
  assert process.run(command)?.exited_with(0)
  assert out.read_text()? == "# CPU,ONLINE\n0,Y\n1,N\n"
  root.write(p"sys/devices/system/cpu/cpu0/topology/physical_package_id", "4\n")
  root.write(p"sys/devices/system/cpu/cpu1/topology/physical_package_id", "9\n")
  for index in range(2) { root.write(fp"sys/devices/system/cpu/cpu{index}/topology/core_id", "7\n") }
  let physical = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--parse=CPU,SOCKET,CORE", "--physical", "--all", "--sysroot", folder.display()],
    folder, {}, b"", out, fp"{folder}/stderr", timeout: 5s)
  assert process.run(physical)?.exited_with(0)
  assert out.read_text()? == "# CPU,SOCKET,CORE\n0,4,7\n1,9,7\n"
}
