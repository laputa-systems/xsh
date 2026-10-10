type Probe = {status: Int, stdout: Str}

pure inode_of(link: Str) -> Int {
  let found = rx"[0-9]+".find(link)
  return 0 when found.is_empty()
  found[0].text.parse_int() ?? 0
}

# Runs `script` under sh in the namespaces the call changes and returns what it
# wrote to stdout; an unprivileged kernel without user namespaces skips.
proc probe(
  ctx: TestContext,
  script: Str,
  unshare: List[Str],
  join: List[Path] = [],
  fork = false,
) [fs, process, error] -> Result[Probe] {
  let root = test.temp_dir(ctx, name: "ns-probe")?
  let out = fp"{root}/out"
  let plan = process.command_argv("sh", ["sh", "-c", script], root, {}, b"", out)
  let status = linux.run_in_namespaces(plan, unshare:, join:, map_root_user: "user" in unshare, fork:)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?})
}

proc require_user_namespaces() [process, env, error] -> Result[Bool] {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("namespaces are Linux-only")
    return false
  }
  let plan = process.command_argv("true", ["true"])
  match linux.run_in_namespaces(plan, unshare: ["user"], map_root_user: true) {
    Ok(status) => {
      guard status.shell_code()? == 0 else {
        test.skip("the kernel refuses unprivileged user namespaces")
        return false
      }
      true
    }
    Err(failure) => {
      test.skip(f"the kernel refuses unprivileged user namespaces: {failure.message}")
      false
    }
  }
}

test test_linux_namespaces_lists_the_callers_own_namespaces {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("namespaces are Linux-only")
    return
  }
  let pid = process.current_pid()?
  let own = linux.namespaces(pid)?
  for name in ["mnt", "uts", "ipc", "net", "pid", "user", "cgroup"] {
    let rows = [row for row in own if row.type == name]
    assert rows.len() == 1, f"one {name} namespace for the process"
    let row = rows[0]
    assert row.ns == inode_of(fp"/proc/self/ns/{name}".readlink()?.display())
    assert row.nprocs >= 1
    assert row.pid <= pid
    assert row.path.display() == f"/proc/{row.pid}/ns/{name}"
    assert row.pns >= 0 and row.ons >= 0
    assert row.uid >= 0
    assert row.command != ""
  }
}

test test_linux_namespaces_orders_by_inode_and_counts_every_process {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("namespaces are Linux-only")
    return
  }
  let all = linux.namespaces()?
  assert !all.is_empty()
  var previous = 0
  for row in all {
    assert row.ns > previous, "the list is ordered by inode"
    previous = row.ns
  }
  let own = linux.namespaces(process.current_pid()?)?
  for row in own {
    let wide = [entry for entry in all if entry.ns == row.ns and entry.type == row.type]
    assert wide.len() == 1
    # Other processes come and go, so the two counts cannot be compared; the
    # process itself holds every namespace it is listed for.
    assert wide[0].nprocs >= 1 and row.nprocs >= 1
  }
  assert own.len() <= all.len()
}

test test_linux_namespaces_of_a_missing_process_is_empty {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("namespaces are Linux-only")
    return
  }
  assert linux.namespaces(2147483646)?.is_empty()
}

test test_linux_namespaces_reports_a_bound_namespace_file { |ctx|
  guard require_user_namespaces() else { return }
  let root = test.temp_dir(ctx, name: "ns-bound")?
  let bound = fp"{root}/net"
  bound.write("")
  # A mount namespace of its own lets the child bind its network namespace
  # file without touching the caller's mounts.
  let helper = test.temp_file(ctx, name: "bound.xsh", contents: bytes.from_text(f"""linux.mount("/proc/self/ns/net", fp"{bound}", options: ["bind"])?
let rows = linux.namespaces(process.current_pid()?)?
for row in rows {{
  if row.type == "net" {{ print row.nsfs.len() }}
}}
"""))?
  let out = fp"{root}/out"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, helper], root, {}, b"", out)
  let status = linux.run_in_namespaces(plan, unshare: ["user", "mnt", "net"], map_root_user: true)?
  assert status.exited_with(0), out.read_text()?
  assert out.read_text()? == "1\n"
}

test test_linux_run_in_namespaces_maps_the_caller_to_root { |ctx|
  guard require_user_namespaces() else { return }
  let ran = probe(ctx, "id -u; cat /proc/self/uid_map | tr -s ' '", ["user"])?
  assert ran.status == 0
  let outer = unix.id()?.uid
  assert ran.stdout == f"0\n 0 {outer} 1\n", ran.stdout
}

test test_linux_run_in_namespaces_unshares_each_kind { |ctx|
  guard require_user_namespaces() else { return }
  let here = fp"/proc/self/ns/uts".readlink()?.display()
  let there = probe(ctx, "readlink /proc/self/ns/uts", ["user", "uts"])?
  assert there.status == 0
  assert there.stdout.trim() != here
  let kept = probe(ctx, "readlink /proc/self/ns/net", ["user", "uts"])?
  assert kept.stdout.trim() == fp"/proc/self/ns/net".readlink()?.display()
}

test test_linux_run_in_namespaces_fork_relays_the_status_and_signal { |ctx|
  guard require_user_namespaces() else { return }
  let first = probe(ctx, "echo $$", ["user", "pid"], fork: true)?
  assert first.status == 0
  assert first.stdout == "1\n", "the command is the first process of the pid namespace"
  assert probe(ctx, "exit 7", ["user", "pid"], fork: true)?.status == 7
  let root = test.temp_dir(ctx, name: "ns-signal")?
  let plan = process.command_argv("sh", ["sh", "-c", "kill -TERM $$"], root)
  let status = linux.run_in_namespaces(plan, unshare: ["user", "uts"], map_root_user: true, fork: true)?
  assert status.signaled()
  assert status.signal_number()? == 15
}

test test_linux_run_in_namespaces_joins_namespace_files_in_order { |ctx|
  guard require_user_namespaces() else { return }
  let root = test.temp_dir(ctx, name: "ns-join")?
  let ready = fp"{root}/ready"
  let release = fp"{root}/release"
  let pidfile = fp"{root}/pid"
  # The holder lives in its own user and UTS namespaces until released.
  let holder_body = f"echo $$ > '{pidfile}'; touch '{ready}'; n=0; while [ ! -e '{release}' ] && [ $n -lt 500 ]; do sleep 0.02; n=$((n+1)); done"
  let holder_script = test.temp_file(ctx, name: "holder.xsh", contents: bytes.from_text(f"""let status = linux.run_in_namespaces(process.command_argv("sh", ["sh", "-c", "{holder_body.replace("\"", with: "\\\"")}"]), unshare: ["user", "uts"], map_root_user: true)?
exit status.shell_code()?
"""))?
  let handle = spawn process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, holder_script], root)?
  for _ in range(0, 500) {
    break when ready.exists()
    time.sleep(10ms)
  }
  assert ready.exists()?, "the namespace holder did not start"
  let holder = pidfile.read_text()?.trim() as Int
  let out = fp"{root}/joined"
  let plan = process.command_argv("sh", ["sh", "-c", "readlink /proc/self/ns/uts; id -u"], root, {}, b"", out)
  let joined = linux.run_in_namespaces(plan, join: [fp"/proc/{holder}/ns/user", fp"/proc/{holder}/ns/uts"])?
  assert joined.exited_with(0), out.read_text()?
  let lines = out.read_lines()?
  assert lines[0] == fp"/proc/{holder}/ns/uts".readlink()?.display()
  # Entering the user namespace as its owner gives the caller its root.
  assert lines[1] == "0"
  release.write("")
  let finished = process.wait_timeout([handle], 5s)?
  assert finished != null, "the namespace holder did not exit"
}

test test_linux_run_in_namespaces_reports_the_failing_step {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("namespaces are Linux-only")
    return
  }
  let plan = process.command_argv("true", ["true"])
  match linux.run_in_namespaces(plan, join: [fp"/proc/self/ns/xsh-missing"]) {
    Ok(status) => assert false, f"a missing namespace file must fail, got {status.kind}"
    Err(failure) => {
      assert failure.message == "cannot open /proc/self/ns/xsh-missing: No such file or directory", failure.message
    }
  }
  if user.current()?.uid != 0 {
    match linux.run_in_namespaces(plan, unshare: ["net"]) {
      Ok(status) => assert false, f"an unprivileged net unshare must fail, got {status.kind}"
      Err(failure) => assert failure.message == "unshare failed: Operation not permitted", failure.message
    }
  }
}

test test_linux_run_in_namespaces_rejects_requests_that_would_do_nothing {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("namespaces are Linux-only")
    return
  }
  let plan = process.command_argv("true", ["true"])
  test.error_kind(linux.run_in_namespaces(plan, unshare: ["bogus"]), "invalid-argument")
  test.error_kind(linux.run_in_namespaces(plan, unshare: ["user"], propagation: "bogus"), "invalid-argument")
  test.error_kind(linux.run_in_namespaces(plan, unshare: ["user"], propagation: "private"), "invalid-argument")
  test.error_kind(linux.run_in_namespaces(plan, unshare: ["user"], mount_proc: p"/proc"), "invalid-argument")
  test.error_kind(linux.run_in_namespaces(plan, unshare: ["uts"], map_root_user: true), "invalid-argument")
  test.error_kind(linux.run_in_namespaces(plan, uid: -1), "invalid-argument")
}

test test_linux_namespace_entries_are_not_part_of_the_linux_fake { |ctx|
  let plan = process.command_argv("true", ["true"])
  test.linux_fake(ctx, {log: fp"{test.temp_dir(ctx, name: "fake")?}/log"})
  test.error_kind(linux.namespaces(), "linux-fake-unsupported")
  test.error_kind(linux.run_in_namespaces(plan, unshare: ["user"], map_root_user: true), "linux-fake-unsupported")
}
