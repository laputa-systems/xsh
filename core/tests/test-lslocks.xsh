use support.uu
use support.util_linux as ul

type LockReady = {
  method: Str,
  pid: Int,
  fd: Int,
  duplicate_fds: List[Int],
  device: Str,
  inode: Int,
  path: Str,
  lock_id: Int,
}

type WaiterReady = {
  method: Str,
  pid: Int,
  fd: Int,
  duplicate_fds: List[Int],
  device: Str,
  inode: Int,
  path: Str,
  lock_id: Int,
  owner_pid: Int,
}

proc compared(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let reference = ul.reference(s, "lslocks", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lslocks", args, vars: vars, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok(candidate)
}

proc parity(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let _ = compared(s, args, vars:)?
  Ok()
}

proc upstream_lock_case(s: uu.Scene, method: Str, holders: Bool) [fs, process, error] -> Result[Unit, Error] {
  let path = uu.at(s, f"lock-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let descriptor_mode = if holders { "duplicate" } else { "none" }
  let keeper = ul.start_keeper(s, script, ["hold", method, path.display(), descriptor_mode])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  assert ready.method == method and ready.pid > 0 and ready.fd == 17
  assert ready.duplicate_fds == (if holders { [18] } else { [] })
  assert ready.path == path.display() and ready.inode > 0 and ready.lock_id > 0
  let pid = f"{ready.pid}"
  let typed = compared(s, ["--json", "--pid", pid, "--output", "PID,TYPE"])?
  let typed_document = json.decode(typed.stdout.utf8() ?? "")?
  let typed_rows = json.get(typed_document, ["locks"])?.require(List[Any])?
  let expected_type = if method.starts_with("flock-") { "FLOCK" } else if method.starts_with("posix-") { "POSIX" } else if method.starts_with("ofd-") { "OFDLCK" } else { "LEASE" }
  let expected_rows = if method.ends_with("-rw") { 2 } else { 1 }
  assert typed_rows.len() == expected_rows
  for index in range(typed_rows.len()) {
    assert json.get(typed_document, ["locks", index, "PID"])?.require(Int)? == ready.pid
    assert json.get(typed_document, ["locks", index, "TYPE"])?.require(Str)? == expected_type
  }
  let columns = if holders {
    "COMMAND,TYPE,SIZE,MODE,START,END,HOLDERS"
  } else {
    "COMMAND,TYPE,SIZE,MODE,START,END"
  }
  parity(s, ["--raw", "--noheadings", "--pid", pid, "--output", columns])?
  unless holders {
    parity(s, ["--raw", "--noheadings", "--pid", pid, "--output", "PATH"])?
  }
  ul.keeper_send(keeper, "DONE")
  Ok()
}

proc blocked_lock_case(s: uu.Scene, method: Str) [fs, process, error] -> Result[Unit, Error] {
  let path = uu.at(s, f"blocked-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let owner = ul.start_keeper(s, script, ["hold", method, path.display(), "none"])?
  defer ul.stop_keeper(owner)
  let owner_ready = json.decode(owner.ready)?.require(LockReady)?
  assert owner_ready.method == method and owner_ready.pid > 0 and owner_ready.inode > 0
  let waiter = ul.start_keeper(s, script, ["wait", method, path.display(), f"{owner_ready.pid}"])?
  defer ul.stop_keeper(waiter)
  let waiter_ready = json.decode(waiter.ready)?.require(WaiterReady)?
  assert waiter_ready.method == method and waiter_ready.pid > 0
  assert waiter_ready.owner_pid == owner_ready.pid and waiter_ready.inode == owner_ready.inode
  assert waiter_ready.device == owner_ready.device and waiter_ready.lock_id > 0
  let args = ["--json", "--pid", f"{waiter_ready.pid}", "--output", "PID,TYPE,MODE,BLOCKER,PATH"]
  let candidate = compared(s, args)?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
  assert json.get(document, ["locks", 0, "PID"])?.require(Int)? == waiter_ready.pid
  assert json.get(document, ["locks", 0, "TYPE"])?.require(Str)? == (if method == "flock-ex" { "FLOCK" } else if method == "posix--w" { "POSIX" } else { "OFDLCK" })
  assert json.get(document, ["locks", 0, "MODE"])?.require(Str)? == "WRITE*"
  assert json.get(document, ["locks", 0, "BLOCKER"])?.require(Int)? == owner_ready.pid
  assert json.get(document, ["locks", 0, "PATH"])?.require(Str)? == path.display()
  ul.keeper_send(owner, "DONE")
  Ok()
}

proc all_columns_json_case(s: uu.Scene) [fs, process, env, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let path = uu.at(s, f"json-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let args = ["--json", "--pid", f"{ready.pid}", "--output-all"]
  let candidate = compared(s, args)?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
  assert json.get(document, ["locks", 0, "COMMAND"])?.require(Str)? != ""
  assert json.get(document, ["locks", 0, "PID"])?.require(Int)? == ready.pid
  assert json.get(document, ["locks", 0, "TYPE"])?.require(Str)? == "POSIX"
  assert json.get(document, ["locks", 0, "SIZE"])?.require(Str)? == "1"
  assert json.get(document, ["locks", 0, "INODE"])?.require(Int)? == ready.inode
  assert json.get(document, ["locks", 0, "MAJ:MIN"])?.require(Str)? == ready.device
  assert json.get(document, ["locks", 0, "MODE"])?.require(Str)? == "READ"
  assert json.get(document, ["locks", 0, "M"])?.require(Bool)? == false
  assert json.get(document, ["locks", 0, "START"])?.require(Int)? == 0
  assert json.get(document, ["locks", 0, "END"])?.require(Int)? == 0
  assert json.get(document, ["locks", 0, "PATH"])?.require(Str)? == path.display()
  assert json.get(document, ["locks", 0, "BLOCKER"])? == null
  assert json.get(document, ["locks", 0, "HOLDERS"])?.require(List[Str])?.len() == 1
  ul.keeper_send(keeper, "DONE")
  Ok()
}

proc byte_size_case(s: uu.Scene) [fs, process, env, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let path = uu.at(s, f"bytes-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let human_size = compared(s, ["--json", "--pid", f"{ready.pid}", "--output", "SIZE"])?
  let human_document = json.decode(human_size.stdout.utf8() ?? "")?
  assert json.get(human_document, ["locks", 0, "SIZE"])?.require(Str)? == "1"
  let byte_size = compared(s, ["--json", "--bytes", "--pid", f"{ready.pid}", "--output", "SIZE"])?
  let byte_document = json.decode(byte_size.stdout.utf8() ?? "")?
  assert json.get(byte_document, ["locks", 0, "SIZE"])?.require(Int)? == 1
  ul.keeper_send(keeper, "DONE")
  Ok()
}

proc ranged_lock_case(s: uu.Scene, method: Str, expected: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  let path = uu.at(s, f"ranges-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let candidate = compared(s, ["--json", "--pid", f"{ready.pid}", "--output", "MODE,START,END"])?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  let rows = json.get(document, ["locks"])?.require(List[Any])?
  var actual: List[Str] = []
  for index in range(rows.len()) {
    let mode = json.get(document, ["locks", index, "MODE"])?.require(Str)?
    let start = json.get(document, ["locks", index, "START"])?.require(Int)?
    let end = json.get(document, ["locks", index, "END"])?.require(Int)?
    actual += [f"{start}:{end}:{mode}"]
  }
  let actual_sorted = actual |> sort() |> collect()
  let wanted = expected |> sort() |> collect()
  assert actual_sorted == wanted, f"{method}: expected {wanted}, got {actual_sorted}"
  ul.keeper_send(keeper, "DONE")
  Ok()
}

test test_lslocks_list_columns_modes { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [["-H"], ["--list-columns", "--raw"], ["-H", "--json"]] {
    parity(s, args)?
  }
}

test test_lslocks_raw_noheadings_and_json { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [["--raw"], ["--noheadings"], ["--json"], ["--noheadings", "--json"]] {
    parity(s, args)?
  }
  parity(s, ["--json", "--raw"])?
}

test test_lslocks_defaults_and_lslocks_columns { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  parity(s, [])?
  parity(s, ["--output-all"])?
  parity(s, ["--output-all"], {LSLOCKS_COLUMNS: "TYPE"})?
  parity(s, [], {LSLOCKS_COLUMNS: "TYPE,PID,COMMAND"})?
  parity(s, [], {LSLOCKS_COLUMNS: "+BLOCKER"})?
  parity(s, ["--output", "PATH,TYPE"], {LSLOCKS_COLUMNS: "PID"})?
}

test test_lslocks_output_column_names_and_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [
    ["--output", "path,type"], ["--output", "+BLOCKER"],
    ["--output", "PID,PID"], ["--output", ""], ["--output", "UNKNOWN"],
    ["--output", "PID", "--output", "TYPE"],
  ] {
    parity(s, args)?
  }
}

test test_lslocks_help_and_usage_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [
    ["-h"], ["--help"], ["--output"], ["--pid", "bad"], ["--unknown"],
    ["--json", "--raw"], ["operand"],
  ] {
    parity(s, args)?
  }
}

test test_lslocks_smartcols_json_environment { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for value in ["compact", "lines", "unknown"] {
    parity(s, ["--json"], {LIBSMARTCOLS_JSON: value})?
  }
  parity(s, ["--json"])?
}

test test_lslocks_filter_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for expression in ["TYPE ==", "UNKNOWN == \"POSIX\""] {
    parity(s, ["--filter", expression])?
  }
}

test test_lslocks_version_provenance { |ctx|
  let s = uu.scene(ctx)?
  let reference = ul.reference(s, "lslocks", ["--version"], timeout: 5s)?
  let candidate = uu.invoke(s, "lslocks", ["--version"], timeout: 5s)?
  ul.version_identity(reference, candidate)
}

test test_lslocks_upstream_flock_sh { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-sh", false)?
}

test test_lslocks_upstream_flock_ex { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-ex", false)?
}

test test_lslocks_upstream_posix_read { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix-r-", false)?
}

test test_lslocks_upstream_posix_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix--w", false)?
}

test test_lslocks_upstream_posix_read_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix-rw", false)?
}

test test_lslocks_upstream_ofd_read { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-r-", false)?
}

test test_lslocks_upstream_ofd_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd--w", false)?
}

test test_lslocks_upstream_ofd_read_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-rw", false)?
}

test test_lslocks_upstream_lease_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "lease-w", false)?
}

test test_lslocks_upstream_flock_sh_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-sh", true)?
}

test test_lslocks_upstream_flock_ex_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-ex", true)?
}

test test_lslocks_upstream_ofd_read_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-r-", true)?
}

test test_lslocks_upstream_ofd_write_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd--w", true)?
}

test test_lslocks_upstream_ofd_read_write_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-rw", true)?
}

test test_lslocks_posix_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix-r-", true)?
}

test test_lslocks_upstream_lease_write_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "lease-w", true)?
}

test test_lslocks_blocker_flock { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  blocked_lock_case(s, "flock-ex")?
}

test test_lslocks_blocker_posix { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  blocked_lock_case(s, "posix--w")?
}

test test_lslocks_blocker_ofd { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  blocked_lock_case(s, "ofd--w")?
}

test test_lslocks_all_columns_types_and_json { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  all_columns_json_case(s)?
}

test test_lslocks_bytes_and_human_size { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  byte_size_case(s)?
}

test test_lslocks_ranges_and_eof { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  ranged_lock_case(s, "posix-rw", ["0:0:READ", "2:2:WRITE"])?
  ranged_lock_case(s, "posix-eof-w", ["2:0:WRITE"])?
  ranged_lock_case(s, "ofd-rw", ["0:0:READ", "2:2:WRITE"])?
  ranged_lock_case(s, "ofd-eof-w", ["2:0:WRITE"])?
}
