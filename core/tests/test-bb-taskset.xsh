use support.uu

# origin: busybox taskset/taskset (get from pid 1)
test test_bb_taskset_taskset_get_from_pid_1_ce4e9812 { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("taskset needs Linux CPU affinity")
    return
  }
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "taskset", ["-p", "1"], stdout: p"/dev/null", timeout: 5s)?
  uu.succeeds(r)
}

# origin: busybox taskset/taskset (set_aff, needs CAP_SYS_NICE)
test test_bb_taskset_taskset_set_aff_needs_CAP_SYS_NICE_95ac36d5 { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("taskset needs Linux CPU affinity")
    return
  }
  let s = uu.scene(ctx)?
  let mask = lowest_allowed_cpu_mask()
  let query = uu.argv(s, "taskset", [])?
  let args = [f"0x{mask}", "/bin/sh", "-c", "\"$@\" -p \"$$\"", "affinity-query"].extend([word.display() for word in query])
  let r = uu.invoke(s, "taskset", args, timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_contains(r, f"current affinity mask: {mask}")
}

proc lowest_allowed_cpu_mask() [fs, error] -> Str {
  var allowed = ""
  for line in p"/proc/self/status".read_text()?.lines() {
    if line.starts_with("Cpus_allowed_list:") {
      allowed = line.split(":")[1].trim()
    }
  }
  let first = allowed.split(",")[0].split("-")[0].parse_int()?
  var mask = ["1", "2", "4", "8"][first % 4]
  var zeros = first / 4
  while zeros > 0 {
    mask = mask + "0"
    zeros -= 1
  }
  mask
}
