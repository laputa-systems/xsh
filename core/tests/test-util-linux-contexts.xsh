use support.uu
use support.util_linux as ul
use support.util_linux_contexts as contexts

type IdentityStatus = {Uid: Str, Gid: Str, CapEff: Str, CapPrm: Str, CapInh: Str, CapAmb: Str, CapBnd: Str}
type IdentityReady = {pid: Int, inode: Int, uid: Int, status: IdentityStatus}

test test_util_linux_context_outside_holder_is_invisible_to_private_lock_listing { |ctx|
  let s = uu.scene(ctx)?
  let holder = contexts.outside_holder()?
  let inside = ul.start_keeper(s, fp"{ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py",
    ["hold", "flock-ex", uu.at(s, "inside-holder").display(), "none"])?
  defer ul.stop_keeper(inside)
  let ready = json.decode(inside.ready)?
  let inside_inode = json.get(ready, ["inode"])?.require(Int)?
  let inside_pid = json.get(ready, ["pid"])?.require(Int)?
  assert inside_inode != holder.inode
  for reference in [true, false] {
    let observation = if reference {
      ul.observe_reference(s, "lslocks", [p"--json", p"--output", p"PID,INODE,PATH"])?
    } else {
      ul.observe_candidate(s, "lslocks", [p"--json", p"--output", p"PID,INODE,PATH"])?
    }
    assert observation.termination == ul.Exited(0), observation.stderr.utf8()?
    let rows = json.get(json.decode(observation.stdout.utf8()?)?, ["locks"])?.require(List[Any])?
    let visible = [row for row in rows if json.get(row, ["inode"])?.require(Int)? == inside_inode]
    assert visible.len() == 1, "private holder is absent from its own namespace"
    assert json.get(visible[0], ["pid"])?.require(Int)? == inside_pid
    for row in rows {
      assert json.get(row, ["inode"])?.require(Int)? != holder.inode, observation.stdout.utf8()?
    }
    assert ! (holder.path.display() in observation.stdout.utf8()?)
  }
  contexts.release_keeper(inside)
}

test test_util_linux_context_second_uid_holder_disappears_after_owned_release { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o777)
  let target = uu.at(s, "private-owner")
  let keeper = contexts.start_identity_keeper(s,
    fp"{ctx.core_dir}/tests/fixtures/util-linux/contexts/identity_lock.py", [target.display()])?
  defer ul.stop_keeper(keeper)
  let owner = json.decode(keeper.ready)?.require(IdentityReady)?
  assert owner.uid == 10002 and owner.status.CapEff == "0000000000000000"
  assert fs.stat(target)?.ino == owner.inode
  let options = ["--json", "--pid", f"{owner.pid}", "--output", "PID,INODE,PATH"]
  let before = ul.reference(s, "lslocks", options)?
  uu.succeeds(before)
  assert json.get(json.decode(before.stdout.utf8()?)?, ["locks"])?.require(List[Any])?.len() == 1
  let observer_reference = ul.reference_as_user(s, "lslocks", options, uid: 10001)?
  let observer_candidate = ul.candidate_as_user(s, "lslocks", options, uid: 10001)?
  ul.compare(observer_reference, observer_candidate)
  let visible = json.get(json.decode(observer_reference.stdout.utf8()?)?, ["locks"])?.require(List[Any])?
  assert visible.len() == 1, "inaccessible holder did not exercise the default fallback row"
  for selector in ["--noinaccessible", "-i"] {
    let inaccessible_options = options.extend([selector])
    let filtered_reference = ul.reference_as_user(s, "lslocks", inaccessible_options, uid: 10001)?
    let filtered_candidate = ul.candidate_as_user(s, "lslocks", inaccessible_options, uid: 10001)?
    uu.succeeds(filtered_reference)
    ul.compare(filtered_reference, filtered_candidate)
    assert filtered_reference.stdout == b"" and filtered_reference.stderr == b""
  }
  contexts.release_keeper(keeper)
  let after_reference = ul.reference(s, "lslocks", options)?
  let after_candidate = uu.invoke(s, "lslocks", options)?
  uu.succeeds(after_reference)
  ul.compare(after_reference, after_candidate)
  assert after_reference.stdout == b"" and after_reference.stderr == b""
}

test test_util_linux_context_terminal_preserves_both_streams_width_and_exit { |ctx|
  let s = uu.scene(ctx)?
  let script = fp"{ctx.core_dir}/tests/fixtures/util-linux/contexts/terminal_bytes.py"
  let observed = contexts.terminal_observe(s, "capture-control", [], [p"/usr/bin/python3", script], 37)?
  assert observed.termination == ul.Exited(7)
  assert observed.stdout == b"width=37\r\n\x00\xff\x1b[31mout\x1b[0m\r\n"
  assert observed.stderr == b"width=37\r\n\x00\xfeerr\r\n"
}

test test_util_linux_context_terminal_reference_width_and_escaping { |ctx|
  let s = uu.scene(ctx)?
  let target = uu.at(s, "long-name-with-tab\tand-control-\x1b-tail")
  target.write("fixture")
  for command in ["mountpoint", "namei"] {
    let args = [target]
    for columns in [32, 120] {
      let reference = contexts.terminal_reference(s, command, args, columns:)?
      let candidate = contexts.terminal_candidate(s, command, args, columns:)?
      assert ul.observed_equal(reference, candidate), f"{command}: terminal bytes differ"
    }
  }
}

type Capabilities = {CapEff: Str, CapPrm: Str, CapInh: Str, CapAmb: Str, CapBnd: Str}
type IpcIdentityReady = {pid: Int, id: Int, uid: Int, capabilities: Capabilities}

test test_util_linux_context_second_uid_ipc_creator_and_observer { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o755)
  let keeper = contexts.start_identity_keeper(s,
    fp"{ctx.core_dir}/tests/fixtures/util-linux/contexts/identity_ipc.py", [])?
  defer ul.stop_keeper(keeper)
  let owner = json.decode(keeper.ready)?.require(IpcIdentityReady)?
  assert owner.uid == 10002 and owner.capabilities.CapEff == "0000000000000000"
  let entries = [line.fields() for line in p"/proc/sysvipc/shm".read_text()?.lines()]
  let owned = [fields for fields in entries if fields.len() > 9 and fields[1] == f"{owner.id}"]
  assert owned.len() == 1
  assert owned[0][4] == f"{owner.pid}" and owned[0][7] == "10002" and owned[0][9] == "10002"
  let options = ["--shmems", "--id", f"{owner.id}", "--json", "--output", "ID,UID,CUID,CPID,SIZE"]
  let owner_reference = ul.reference_as_user(s, "lsipc", options, uid: 10002)?
  let owner_candidate = ul.candidate_as_user(s, "lsipc", options, uid: 10002)?
  uu.succeeds(owner_reference)
  ul.compare(owner_reference, owner_candidate)
  let observer_reference = ul.reference_as_user(s, "lsipc", options, uid: 10001)?
  let observer_candidate = ul.candidate_as_user(s, "lsipc", options, uid: 10001)?
  ul.compare(observer_reference, observer_candidate)
  let terminal_args = [p"--shmems", p"--id", fp"{owner.id}", p"--output", p"ID,UID,CUID,CPID,SIZE"]
  for columns in [24, 120] {
    let reference = contexts.terminal_reference(s, "lsipc", terminal_args, columns:)?
    let candidate = contexts.terminal_candidate(s, "lsipc", terminal_args, columns:)?
    assert reference == candidate, f"lsipc terminal columns={columns}"
  }
  contexts.release_keeper(keeper)
  let released_reference = ul.reference(s, "lsipc", options)?
  let released_candidate = uu.invoke(s, "lsipc", options)?
  uu.fails(released_reference)
  ul.compare(released_reference, released_candidate)
}

test test_util_linux_context_terminal_lock_path_truncation { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let script = fp"{ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let requests = [
    ["hold", "flock-ex", uu.at(s, "long-lock-path-with-tab\tand-a-control-\x1b-character").display(), "none"],
    ["hold-bytes", "flock-ex", s.root.display(), "6c6f6e672d6e6f6e757466382dff2d6c6f636b2d70617468", "none"],
  ]
  for request in requests {
    let keeper = ul.start_keeper(s, script, request)?
    defer ul.stop_keeper(keeper)
    let pid = json.get(json.decode(keeper.ready)?, ["pid"])?.require(Int)?
    let selected = [p"--pid", fp"{pid}", p"--output", p"PID,PATH"]
    let narrow_reference = contexts.terminal_reference(s, "lslocks", selected, columns: 24)?
    let wide_reference = contexts.terminal_reference(s, "lslocks", selected, columns: 120)?
    assert narrow_reference.stdout != wide_reference.stdout, "reference did not exercise terminal truncation"
    for args in [selected, selected.extend([p"-u"])] {
      for columns in [24, 120] {
        let reference = contexts.terminal_reference(s, "lslocks", args, columns:)?
        let candidate = contexts.terminal_candidate(s, "lslocks", args, columns:)?
        assert reference == candidate, f"lslocks terminal columns={columns} fixture={request[0]} untruncated={args.len() > selected.len()}"
      }
    }
    contexts.release_keeper(keeper)
  }
}

test test_util_linux_context_terminal_rooted_memory { |ctx|
  let s = uu.scene(ctx)?
  let root = fp"{ctx.core_dir}/tests/fixtures/util-linux/memory/configured"
  let args = [p"--sysroot", root, p"--output", p"RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE", p"--summary=never"]
  for columns in [24, 120] {
    let reference = contexts.terminal_reference(s, "lsmem", args, columns:)?
    let candidate = contexts.terminal_candidate(s, "lsmem", args, columns:)?
    assert reference == candidate, f"lsmem terminal columns={columns}"
  }
}

test test_util_linux_context_owned_cancellation_releases_lock_and_ipc { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o777)
  let target = uu.at(s, "cancelled-owner")
  let holder = contexts.start_identity_keeper(s,
    fp"{ctx.core_dir}/tests/fixtures/util-linux/contexts/identity_lock.py", [target.display()])?
  defer ul.stop_keeper(holder)
  let lock = json.decode(holder.ready)?.require(IdentityReady)?
  let creator = contexts.start_identity_keeper(s,
    fp"{ctx.core_dir}/tests/fixtures/util-linux/contexts/identity_ipc.py", [])?
  defer ul.stop_keeper(creator)
  let segment = json.decode(creator.ready)?.require(IpcIdentityReady)?
  assert fs.stat(target)?.ino == lock.inode
  assert lock.pid != segment.pid
  holder.child.cancel(signal: "TERM", kill_after: 2s)
  creator.child.cancel(signal: "TERM", kill_after: 2s)
  for line in p"/proc/locks".read_text()?.lines() {
    let fields = line.fields()
    assert fields.len() < 6 or ! fields[5].ends_with(f":{lock.inode}")
  }
  for line in p"/proc/sysvipc/shm".read_text()?.lines() {
    let fields = line.fields()
    assert fields.len() < 2 or fields[1] != f"{segment.id}"
  }
}
