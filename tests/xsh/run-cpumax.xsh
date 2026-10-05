# `--cpumax` and `cpu_max` place the child in a transient cgroup scope below
# `XSH_CGROUP_ROOT`. These tests point that root at an ordinary directory, so
# they need no cgroup privileges, and require the scope to be gone afterwards.

test test_run_cpumax_leaves_no_scope_under_the_cgroup_root { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("--cpumax uses cgroups, which are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "run-cpumax-cgroup")?
  let _ = test.expect(ctx, "run --cpumax=80 true\n", status: 0, env: {XSH_CGROUP_ROOT: root})?
  assert (fs.children(root)? |> count()) == 0
}

test test_spawn_and_command_plan_cpumax_leave_no_scope { |ctx|
  let root = test.temp_dir(ctx, name: "spawn-cpumax-cgroup")?
  let output = test.expect(
    ctx,
    r"""let first = spawn run --cpumax=80 true ?
let first_status = wait first?
let command = process.command {
  cpu_max = 80
  run true
}
let second = spawn command?
let second_status = wait second?
print ${first_status.ok} ${second_status.ok}
""",
    status: 0,
    env: {XSH_CGROUP_ROOT: root},
  )?
  assert output.stdout == "true true\n"
  assert (fs.children(root)? |> count()) == 0
}
