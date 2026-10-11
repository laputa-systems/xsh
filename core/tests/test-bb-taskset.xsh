use support.uu

# origin: busybox taskset/taskset (get from pid 1)
test test_bb_taskset_taskset_get_from_pid_1_ce4e9812 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "taskset", ["-p", "1"], stdout: p"/dev/null", timeout: 5s)?
  uu.succeeds(r)
}

# origin: busybox taskset/taskset (set_aff, needs CAP_SYS_NICE)
test test_bb_taskset_taskset_set_aff_needs_CAP_SYS_NICE_95ac36d5 { |ctx|
  let s = uu.scene(ctx)?
  let query = uu.argv(s, "taskset", [])?
  let args = ["0x1", "/bin/sh", "-c", "\"$@\" -p \"$$\"", "affinity-query"].extend([word.display() for word in query])
  let r = uu.invoke(s, "taskset", args, timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_contains(r, "current affinity mask: 1")
}
