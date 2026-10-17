use support.uu
use support.util_linux as ul

type LockReady = {method: Str, pid: Int, fd: Int, duplicate_fds: List[Int], device: Str, inode: Int, path: Str, lock_id: Int}
type WaiterReady = {method: Str, pid: Int, fd: Int, duplicate_fds: List[Int], device: Str, inode: Int, path: Str, lock_id: Int, owner_pid: Int}

test test_util_linux_comparison_rejects_observable_mutants { |_ctx|
  let reference: uu.Ran = {util: "lslocks", args: ["--raw"], status: 0,
    stdout: b"holder=21 blocker=22 inode=101\n", stderr: b""}
  assert ul.equal(reference, reference, reference_state: b"owned=101", candidate_state: b"owned=101")
  for mutant in [
    {...reference, stdout: b"holder=21 blocker=22 inode=101 \n"},
    {...reference, stderr: b"warning\n"},
    {...reference, status: 1},
    {...reference, stdout: b"holder=22 blocker=21 inode=101\n"},
    {...reference, stdout: b"holder=21 blocker=22 inode=102\n"},
    {...reference, stdout: b"holder=PID blocker=PID inode=INODE\n"},
  ] {
    assert ! ul.equal(reference, mutant)
  }
  assert ! ul.equal(reference, reference, reference_state: b"owned=101", candidate_state: b"owned=102")
}

test test_util_linux_reference_preserves_raw_path_arguments { |ctx|
  let s = uu.scene(ctx)?
  let operand = uu.at_bytes(s, b"raw-\xff")?
  operand.write("fixture")
  let result = ul.reference_paths(s, "mountpoint", [p"--quiet", operand])?
  assert result.status == 32
  assert result.stdout == b"" and result.stderr == b""
}

test test_util_linux_observation_distinguishes_signal_from_shell_exit { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tests/support")
  fp"{s.ctx.core_dir}/tests/support/uu-launch.xsh".copy(to: uu.at(s, "tests/support/uu-launch.xsh"))
  uu.write(s, "status.xsh", "let environment: Map[Str, Bytes] = {}\nunix.exec_env(process.command_argv(p\"/bin/sh\", [p\"/bin/sh\", p\"-c\", Path(args[0])]), environment)\n")?
  let staged: uu.Scene = {ctx: {...ctx, core_dir: s.root}, root: s.root}
  let observed_signal = ul.observe_candidate(staged, "status", [p"kill -TERM $$"])?
  let observed_exit = ul.observe_candidate(staged, "status", [p"exit 143"])?
  assert observed_signal.termination == ul.Signaled(15), observed_signal.stderr.utf8()?
  assert observed_exit.termination == ul.Exited(143)
  assert ! ul.observed_equal(observed_signal, {...observed_signal, termination: observed_exit.termination})
}

test test_util_linux_private_namespace_and_unprivileged_identity { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o755)
  uu.mkdir(s, "tests/support")
  fp"{s.ctx.core_dir}/tests/support/uu-launch.xsh".copy(to: uu.at(s, "tests/support/uu-launch.xsh"))
  uu.write(s, "identity.xsh", "let lines = p\"/proc/self/status\".read_text()?.split(\"\\n\")\nfor line in lines { if line.starts_with(\"Uid:\") or line.starts_with(\"CapEff:\") { print $line } }\n")?
  let staged: uu.Scene = {ctx: {...ctx, core_dir: s.root}, root: s.root}
  for uid in [10001, 10002] {
    let result = ul.candidate_as_user(staged, "identity", [], uid:)?
    uu.succeeds(result)
    uu.stdout_is(result, f"Uid:\t{uid}\t{uid}\t{uid}\t{uid}\nCapEff:\t0000000000000000\n")
  }
}

test test_util_linux_lock_fixture_preserves_kernel_holder_identity { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let script = fp"{ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  for method in ["flock-ex", "posix--w", "ofd--w", "lease-w"] {
    let target = uu.at(s, method)
    let keeper = ul.start_keeper(s, script, ["hold", method, target.display(), "none"])?
    defer ul.stop_keeper(keeper)
    let held = json.decode(keeper.ready)?.require(LockReady)?
    let metadata = fs.stat(target)?
    assert metadata.ino == held.inode
    let descriptors = [entry for entry in linux.open_files(held.pid)? if entry.dev == metadata.dev and entry.inode == held.inode]
    assert ! descriptors.is_empty()
    let reference = ul.reference(s, "lslocks", ["--pid", f"{held.pid}", "--raw", "--noheadings", "--output", "PID,INODE,TYPE,PATH"])?
    uu.succeeds(reference)
    uu.stdout_contains(reference, f"{held.pid} {held.inode} ")
    uu.stdout_contains(reference, target.display())
    ul.keeper_send(keeper, "DONE")
    ul.finish_keeper(keeper)
  }
}

test test_util_linux_rooted_memory_reference_cannot_use_host_sysfs { |ctx|
  let s = uu.scene(ctx)?
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/devices/system/memory/memory0", parents: true)
  root.write(p"sys/devices/system/memory/block_size_bytes", "10000000\n")
  root.write(p"sys/devices/system/memory/memory0/state", "online\n")
  root.write(p"sys/devices/system/memory/memory0/removable", "1\n")
  root.write(p"sys/devices/system/memory/memory0/valid_zones", "Normal\n")
  assert root.read_text(p"sys/devices/system/memory/block_size_bytes")? == "10000000\n"
  let reference = ul.reference(s, "lsmem", ["--sysroot", root.host_path()?.display(), "--raw", "--noheadings", "--bytes", "--output", "BLOCK,SIZE,STATE", "--summary=never"])?
  uu.succeeds(reference)
  uu.stdout_is(reference, "0 268435456 online\n")
  root.remove(p"sys/devices/system/memory/block_size_bytes")
  let missing = ul.reference(s, "lsmem", ["--sysroot", root.host_path()?.display()])?
  uu.fails(missing)
  uu.stdout_is(missing, "")
}

test test_util_linux_blocked_ofd_reference_preserves_kernel_and_holder_ids { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let script = fp"{ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let target = uu.at(s, "blocked-ofd")
  let owner = ul.start_keeper(s, script, ["hold", "ofd--w", target.display(), "none"])?
  defer ul.stop_keeper(owner)
  let owner_ready = json.decode(owner.ready)?.require(LockReady)?
  let waiter = ul.start_keeper(s, script, ["wait", "ofd--w", target.display(), f"{owner_ready.pid}"])?
  defer ul.stop_keeper(waiter)
  let waiter_ready = json.decode(waiter.ready)?.require(WaiterReady)?
  assert owner_ready.inode == waiter_ready.inode and owner_ready.pid != waiter_ready.pid
  let reference = ul.reference(s, "lslocks", ["--json", "--output", "PID,TYPE,MODE,BLOCKER,INODE,MAJ:MIN"])?
  uu.succeeds(reference)
  let rows = json.get(json.decode(reference.stdout.utf8()?)?, ["locks"])?.require(List[Any])?
  let blocked = [row for row in rows if json.get(row, ["mode"])?.require(Str)? == "WRITE*"]
  assert blocked.len() == 1, reference.stdout.utf8()?
  assert json.get(blocked[0], ["pid"])?.require(Int)? == -1, reference.stdout.utf8()?
  assert json.get(blocked[0], ["blocker"])?.require(Int)? == owner_ready.pid, reference.stdout.utf8()?
  let selected = ul.reference(s, "lslocks", ["--json", "--pid", f"{waiter_ready.pid}", "--output", "PID,TYPE,MODE,BLOCKER,INODE,MAJ:MIN"])?
  uu.succeeds(selected)
  assert json.get(json.decode(selected.stdout.utf8()?)?, ["locks"])?.require(List[Any])?.is_empty(), selected.stdout.utf8()?
  ul.keeper_send(owner, "DONE")
  ul.finish_keeper(owner)
  ul.finish_keeper(waiter)
}

test test_util_linux_private_bind_reference_observes_same_device_mount { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let source = uu.at(s, "source")
  let target = uu.at(s, "target")
  source.mkdir()
  target.mkdir()
  assert fs.stat(source)?.dev == fs.stat(target)?.dev
  run mount --bind $source $target
  defer run umount -- $target
  let reference = ul.reference(s, "mountpoint", ["--quiet", target.display()])?
  uu.succeeds(reference)
  uu.symlink(s, target.display(), "mount-link")
  let nofollow = ul.reference(s, "mountpoint", ["--quiet", "--nofollow", uu.at(s, "mount-link").display()])?
  uu.fails_with_code(nofollow, 32)
}
